//! Library service loop owns catalog state, playback coordination and scan scheduling.
use super::state::{CatalogState, PlaybackState};
use super::{catalog_letter_index, metadata, read_json, shutdown_volume, write_json};
use crate::jobs::LatestJob;
use crate::{
    operation_dispatch,
    persistence::{read_session, Session, SessionWriter},
    playback_flow::{cancel_preload, submit_play},
    protocol::{Event, PlayerAction, Request, Settings},
};
use orca_services::{
    new_backend,
    operation_types::OperationRequest,
    types::{PlaybackCommand, Track},
};
use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
pub(super) struct LibraryWorker {
    pub directory: PathBuf,
    pub audio: bool,
    pub worker_stop: Arc<AtomicBool>,
    pub revision: Arc<AtomicU64>,
    pub query_revision: Arc<AtomicU64>,
    pub auto_artwork: Arc<AtomicBool>,
    pub events: Sender<Event>,
    pub requests: Receiver<Request>,
    pub operations: mpsc::SyncSender<OperationRequest>,
    pub analysis_jobs: LatestJob<(u64, String, bool)>,
    pub lyrics_jobs: LatestJob<(u64, String, bool)>,
}
pub(super) fn run(worker: LibraryWorker) {
    let LibraryWorker {
        directory,
        audio,
        worker_stop,
        revision,
        query_revision,
        auto_artwork,
        events,
        requests,
        operations,
        analysis_jobs,
        lyrics_jobs,
    } = worker;
    let Ok(mut backend) = new_backend(&directory.to_string_lossy(), audio) else {
        let _ = events.send(Event::Error(
            "Cannot open the Slint library database".into(),
        ));
        return;
    };
    let mut settings: Settings =
        read_json(&directory.join("slint-settings.json")).unwrap_or_default();
    auto_artwork.store(settings.auto_artwork, Ordering::Relaxed);
    let mut session = read_session(&directory.join("slint-session.json")).unwrap_or_default();
    if !session.navigation.order.is_empty() {
        let ids = backend.ids_for_paths(&session.navigation.order);
        let removed: Vec<_> = session.navigation.removed.iter().cloned().collect();
        let played: Vec<_> = session.navigation.played.iter().cloned().collect();
        if let (Ok(ids), Ok(removed_ids), Ok(played_ids)) = (
            ids,
            backend.ids_for_paths(&removed),
            backend.ids_for_paths(&played),
        ) {
            let current_id = backend.id_for_path(&session.path).ok().flatten();
            session.navigation.migrate_legacy(
                ids,
                current_id,
                removed_ids.into_iter().collect(),
                played_ids.into_iter().collect(),
            );
        }
    }
    let mut session_writer = SessionWriter::default();
    let mut playback = PlaybackState {
        navigation: std::mem::take(&mut session.navigation),
        ..Default::default()
    };
    let mut catalog = CatalogState::default();
    let mut saved = Instant::now();
    let mut last_revision = 0;
    let mut library_change_pending = false;
    let mut initial_statistics = true;
    let mut session_save_failed = false;
    let mut pending_scans = std::collections::VecDeque::<String>::new();
    let mut was_scanning = false;
    let _ = events.send(Event::Modes(
        playback.navigation.shuffle,
        playback.navigation.repeat,
    ));
    session.volume = if session.volume.is_finite() {
        session.volume.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if audio {
        if let Err(error) =
            backend.playback_command(PlaybackCommand::ReplayGain(settings.replaygain))
        {
            let _ = events.send(Event::Error(error));
        }
        if let Err(error) = backend.playback_command(PlaybackCommand::Volume(session.volume)) {
            let _ = events.send(Event::Error(error));
        }
        if !session.path.is_empty() {
            if let Err(error) = backend.playback_command(PlaybackCommand::LoadPaused {
                path: session.path.clone(),
                position_ms: session.position,
            }) {
                let _ = events.send(Event::Error(error));
            }
        }
    }
    let mut watcher = crate::watcher::LibraryWatcher::new()
        .map_err(|error| {
            let _ = events.send(Event::Error(format!(
                "Folder watching unavailable: {error}"
            )));
        })
        .ok();
    let roots = backend.library_roots().unwrap_or_default();
    if let Some(watcher) = watcher.as_mut() {
        for error in watcher.sync(&roots) {
            let _ = events.send(Event::Error(error));
        }
    }
    pending_scans.extend(
        roots
            .into_iter()
            .filter(|root| std::path::Path::new(root).is_dir()),
    );
    let mut reconcile = true;
    let mut source_mutation = false;
    let mut deferred_request = None;
    let _ = events.send(Event::Ready);
    while !worker_stop.load(Ordering::Relaxed) {
        let mut shutdown = false;
        let next_request = deferred_request
            .take()
            .or_else(|| requests.recv_timeout(Duration::from_millis(200)).ok());
        if let Some(request) = next_request {
            let refresh_queue = matches!(
                &request,
                Request::Play(..)
                    | Request::Queue(..)
                    | Request::Collection(..)
                    | Request::ArtworkRefresh(..)
            ) || matches!(&request,Request::Command(action) if action.refresh_queue());
            let result: Result<(), String> = (|| {
                match request {
                    Request::Browse(at, new_query, kind) => {
                        if let Err(error) =
                            catalog.browse(&backend, &events, at, new_query, kind, &query_revision)
                        {
                            let _ = events.send(Event::CatalogError(at, error));
                        }
                    }
                    Request::Page(at, offset) => {
                        let mut pages = VecDeque::from([(at, offset)]);
                        let mut skipped = Vec::new();
                        while let Ok(next) = requests.try_recv() {
                            match next {
                                Request::Page(page_at, page_offset) => {
                                    if !pages.contains(&(page_at, page_offset)) {
                                        pages.push_back((page_at, page_offset));
                                    }
                                    if pages.len() > 2 {
                                        if let Some(old) = pages.pop_front() {
                                            skipped.push(old);
                                        }
                                    }
                                }
                                other => {
                                    deferred_request = Some(other);
                                    break;
                                }
                            }
                        }
                        if deferred_request.is_some() {
                            skipped.extend(pages.drain(..));
                        }
                        if !skipped.is_empty() {
                            let mut by_generation =
                                std::collections::HashMap::<u64, Vec<u32>>::new();
                            for (page_at, page_offset) in skipped {
                                by_generation.entry(page_at).or_default().push(page_offset);
                            }
                            for (generation, offsets) in by_generation {
                                let _ = events.send(Event::PagesSkipped(generation, offsets));
                            }
                        }
                        for (page_at, page_offset) in pages {
                            catalog.page(
                                &backend,
                                &events,
                                page_at,
                                page_offset,
                                &query_revision,
                            )?;
                        }
                    }
                    Request::Play(target, from_queue, play_query) => {
                        playback.play(&backend, audio, target, from_queue, play_query)?
                    }
                    Request::Collection(action, collection) => {
                        playback.collection(&backend, audio, action, collection)?
                    }
                    Request::Command(PlayerAction::Letter(letter)) => {
                        let index = if catalog.query.kind.is_empty() && catalog.kind != "songs" {
                            catalog_letter_index(
                                &backend.groups(&catalog.kind, &catalog.query.search)?,
                                &letter,
                            )
                        } else {
                            backend.locate(&catalog.query, &letter)?
                        };
                        let _ = events.send(Event::Jump(catalog.generation, index));
                    }
                    Request::Command(action) => {
                        playback.command(&backend, &events, audio, action)?
                    }
                    Request::Seek(value) if audio => {
                        if !value.is_finite() {
                            return Err("Invalid seek position".into());
                        }
                        cancel_preload(
                            &mut playback.preloaded,
                            &mut playback.planned_navigation,
                            || backend.playback_command(PlaybackCommand::ClearQueued),
                        )?;
                        backend.playback_command(PlaybackCommand::Seek(
                            value.clamp(0.0, 86_400_000.0) as u64,
                        ))?;
                    }
                    Request::Volume(value) if audio => {
                        backend.playback_command(PlaybackCommand::Volume(
                            value.clamp(0.0, 1.0) as f32
                        ))?;
                        session.volume = value.clamp(0.0, 1.0) as f32;
                    }
                    Request::Folder(folder) if !pending_scans.contains(&folder) => {
                        pending_scans.push_back(folder);
                    }
                    Request::FolderPreview {
                        root,
                        current,
                        adding,
                    } => {
                        let root = std::path::Path::new(&root)
                            .canonicalize()
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .into_owned();
                        let current = std::path::Path::new(&current)
                            .canonicalize()
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .into_owned();
                        let adding = adding
                            && !backend.library_roots()?.iter().any(|existing| {
                                std::path::Path::new(existing).canonicalize().ok()
                                    == std::path::Path::new(&root).canonicalize().ok()
                            });
                        let nodes =
                            backend.folder_children(&current, adding.then_some(root.as_str()))?;
                        if adding && root == current && nodes.is_empty() {
                            let root = backend.add_folder_with_rules(&root, &[])?;
                            if !pending_scans.contains(&root) {
                                pending_scans.push_back(root);
                            }
                            reconcile = true;
                        } else {
                            let _ = events.send(Event::FolderChildren {
                                root,
                                current,
                                adding,
                                open: true,
                                nodes,
                            });
                        }
                    }
                    Request::AddFolder(root, rules) => {
                        let root = backend.add_folder_with_rules(&root, &rules)?;
                        if !pending_scans.contains(&root) {
                            pending_scans.push_back(root);
                        }
                        reconcile = true;
                    }
                    Request::SetFolderRule { root, path, mode } => {
                        backend.set_folder_rule(&path, &mode)?;
                        if !pending_scans.contains(&root) {
                            pending_scans.push_back(root.clone());
                        }
                        reconcile = true;
                        let nodes = backend.folder_children(
                            &std::path::Path::new(&path)
                                .parent()
                                .unwrap_or(std::path::Path::new(&root))
                                .to_string_lossy(),
                            None,
                        )?;
                        let current = std::path::Path::new(&path)
                            .parent()
                            .unwrap_or(std::path::Path::new(&root))
                            .to_string_lossy()
                            .into_owned();
                        let _ = events.send(Event::FolderChildren {
                            root,
                            current,
                            adding: false,
                            open: false,
                            nodes,
                        });
                    }
                    Request::Reconcile => {
                        reconcile = true;
                        if !playback.path.is_empty() {
                            if let Ok(track) = backend.track(&playback.path) {
                                let _ = events.send(Event::Now(track));
                            }
                        }
                    }
                    Request::ArtworkRefresh(at, offsets) => {
                        catalog.refresh_artwork(
                            &backend,
                            &events,
                            at,
                            &offsets,
                            &query_revision,
                        )?;
                        if !playback.path.is_empty() {
                            if let Ok(track) = backend.track(&playback.path) {
                                let _ = events.send(Event::Now(track));
                            }
                        }
                    }
                    Request::SourceChanged => {
                        reconcile = true;
                        source_mutation = false;
                    }
                    Request::CollectionDraft(value) => {
                        let _ = events.send(Event::CollectionDraft(value));
                    }
                    Request::Refresh => {
                        reconcile = true;
                        for folder in backend.library_roots()? {
                            if !pending_scans.contains(&folder) {
                                pending_scans.push_back(folder);
                            }
                        }
                    }
                    Request::RefreshLyrics if !playback.path.is_empty() => {
                        lyrics_jobs.submit((
                            revision.load(Ordering::Relaxed),
                            playback.path.clone(),
                            true,
                        ));
                    }
                    Request::RefreshLyrics => {}
                    Request::Queue(action) => playback.queue(&backend, audio, action)?,
                    Request::Operation(request) => {
                        if request.is_local_read() {
                            match backend.execute(&request) {
                                Ok(result) => {
                                    let _ = events.send(Event::Operation(request, result));
                                }
                                Err(error) => {
                                    let _ = events.send(Event::OperationFailed(request, error));
                                }
                            }
                        } else {
                            let removing = request.removes_source();
                            if removing {
                                backend.cancel_and_join_scan();
                                source_mutation = true;
                            }
                            if !operation_dispatch::enqueue(&operations, &events, request)
                                && removing
                            {
                                source_mutation = false;
                            }
                        }
                    }
                    Request::Metadata(request) => {
                        let edited = metadata::handle(&backend, &events, request);
                        if edited.as_deref() == Some(playback.path.as_str()) {
                            if let Ok(track) = backend.track(&playback.path) {
                                let _ = events.send(Event::Now(track));
                            }
                            let at = revision.fetch_add(1, Ordering::Relaxed) + 1;
                            analysis_jobs.submit((at, playback.path.clone(), false));
                            if audio {
                                lyrics_jobs.submit((at, playback.path.clone(), false));
                            }
                        }
                    }
                    Request::Context(target) => {
                        let _ = events.send(Event::Playlists(backend.groups("playlists", "")?));
                        let _ = events.send(Event::Context(backend.track(&target)?));
                    }
                    Request::PlaylistChoices(song_id) => {
                        let groups = backend
                            .groups("playlists", "")?
                            .into_iter()
                            .filter(|group| group.key != "liked")
                            .collect();
                        let selected = backend.song_playlist_ids(song_id)?;
                        let _ = events.send(Event::PlaylistChoices(song_id, groups, selected));
                    }
                    Request::LikedStatus(song_id) => {
                        let liked = backend.is_song_liked(song_id)?;
                        let _ = events.send(Event::LikedStatus(song_id, liked));
                    }
                    Request::ToggleLiked(song_id) => {
                        let liked = !backend.is_song_liked(song_id)?;
                        backend.set_song_liked(song_id, liked)?;
                        let _ = events.send(Event::LikedStatus(song_id, liked));
                        let _ = events.send(Event::PlaylistChanged(
                            orca_services::types::PlaylistChange::Membership,
                        ));
                    }
                    Request::Playlist(edit) => {
                        let removed_likes = match &edit {
                            orca_services::types::PlaylistEdit::RemoveMany {
                                id: None,
                                song_ids,
                            } => song_ids.clone(),
                            _ => Vec::new(),
                        };
                        let action = backend.edit_playlist(edit)?;
                        for song_id in removed_likes {
                            let _ = events.send(Event::LikedStatus(song_id, false));
                        }
                        let _ = events.send(Event::Playlists(backend.groups("playlists", "")?));
                        let _ = events.send(Event::PlaylistChanged(action));
                    }
                    Request::Settings(value) => {
                        if audio && settings.replaygain != value.replaygain {
                            backend
                                .playback_command(PlaybackCommand::ReplayGain(value.replaygain))?;
                        }
                        if settings.gapless && !value.gapless {
                            cancel_preload(
                                &mut playback.preloaded,
                                &mut playback.planned_navigation,
                                || {
                                    if audio {
                                        backend.playback_command(PlaybackCommand::ClearQueued)
                                    } else {
                                        Ok(())
                                    }
                                },
                            )?;
                        }
                        settings = value;
                        auto_artwork.store(settings.auto_artwork, Ordering::Relaxed);
                        write_json(&directory.join("slint-settings.json"), &settings)?;
                    }
                    Request::Font(font_path) => {
                        let bytes = fs::read(&font_path).map_err(|e| e.to_string())?;
                        let _ = events.send(Event::Font(font_path, bytes));
                    }
                    Request::Shutdown => shutdown = true,
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = events.send(Event::Error(error));
            }
            if shutdown {
                break;
            }
            if refresh_queue {
                let tracks = playback
                    .navigation
                    .upcoming_limited(&playback.path, 20, |id| {
                        backend.path_for_id(id).ok().flatten()
                    })
                    .into_iter()
                    .filter_map(|p| backend.track(&p).ok())
                    .collect();
                let _ = events.send(Event::Queue(tracks));
                let current = if playback.requested.is_empty() {
                    &playback.path
                } else {
                    &playback.requested
                };
                let neighbors = playback
                    .navigation
                    .neighbors_compact(current, |id| backend.path_for_id(id).ok().flatten())
                    .into_iter()
                    .filter_map(|p| backend.track(&p).ok())
                    .collect();
                let _ = events.send(Event::Neighbors(current.clone(), neighbors));
            }
        }
        if let Some(watcher) = watcher.as_mut() {
            for folder in watcher.due() {
                if !pending_scans.contains(&folder) {
                    pending_scans.push_back(folder);
                }
            }
        }
        if reconcile {
            reconcile = false;
            let roots = backend.library_roots().unwrap_or_default();
            pending_scans.retain(|root| roots.contains(root));
            if let Some(watcher) = watcher.as_mut() {
                for error in watcher.sync(&roots) {
                    let _ = events.send(Event::Error(error));
                }
            }
            {
                if playback.navigation.is_compact() {
                    if let Some(query) = playback.navigation.context_query.as_ref() {
                        if let Ok(ids) = backend.context_ids(query) {
                            let current = if playback.path.is_empty() {
                                &session.path
                            } else {
                                &playback.path
                            };
                            let current_id = backend.id_for_path(current).ok().flatten();
                            playback.navigation.prune_compact(ids, current_id, |path| {
                                backend.id_for_path(path).ok().flatten().is_some()
                            });
                        }
                    }
                } else {
                    // Prune paths retained by a legacy session without loading
                    // the full catalog into the playback state.
                    let paths: std::collections::HashSet<_> = playback
                        .navigation
                        .order
                        .iter()
                        .chain(playback.navigation.manual.iter())
                        .chain(playback.navigation.history.iter())
                        .chain(playback.navigation.removed.iter())
                        .chain(playback.navigation.played.iter())
                        .cloned()
                        .collect();
                    let allowed = paths
                        .into_iter()
                        .filter(|path| backend.id_for_path(path).ok().flatten().is_some())
                        .collect();
                    playback.navigation.prune(&allowed);
                }
                if !playback.preloaded.is_empty()
                    && backend
                        .id_for_path(&playback.preloaded)
                        .ok()
                        .flatten()
                        .is_none()
                {
                    let _ = cancel_preload(
                        &mut playback.preloaded,
                        &mut playback.planned_navigation,
                        || {
                            if audio {
                                backend.playback_command(PlaybackCommand::ClearQueued)
                            } else {
                                Ok(())
                            }
                        },
                    );
                }
                if !playback.path.is_empty()
                    && backend.id_for_path(&playback.path).ok().flatten().is_none()
                {
                    if audio {
                        let _ = backend.playback_command(PlaybackCommand::Stop);
                    }
                    playback.stopping_removed = playback.path.clone();
                    playback.path.clear();
                    playback.requested.clear();
                    playback.preloaded.clear();
                    playback.planned_navigation = None;
                    revision.fetch_add(1, Ordering::Relaxed);
                    let _ = events.send(Event::Now(Track::default()));
                }
                let rows = playback
                    .navigation
                    .upcoming_limited(&playback.path, 20, |id| {
                        backend.path_for_id(id).ok().flatten()
                    })
                    .into_iter()
                    .filter_map(|p| backend.track(&p).ok())
                    .collect();
                let _ = events.send(Event::Queue(rows));
                library_change_pending = true;
            }
        }
        match backend.snapshot() {
            Ok(mut snapshot) => {
                if snapshot.output_revision != playback.output_revision {
                    playback.output_revision = snapshot.output_revision;
                    playback.preloaded.clear();
                    playback.planned_navigation = None;
                }
                // Audio commands are asynchronous: preserve the user's requested
                // volume while restoration or decoding is still in progress.
                if audio {
                    snapshot.volume = session.volume;
                }
                if !playback.stopping_removed.is_empty() {
                    if snapshot.path == playback.stopping_removed {
                        snapshot.path.clear();
                        snapshot.playing = false;
                        snapshot.position_ms = 0;
                        snapshot.duration_ms = 0;
                        snapshot.ended = playback.ended;
                    } else {
                        playback.stopping_removed.clear();
                    }
                }
                for error in playback.errors.collect(&snapshot) {
                    if error.starts_with("playback:play:")
                        || error.starts_with("playback:restore:")
                        || error.starts_with("output:")
                    {
                        playback.requested.clear();
                    }
                    let _ = events.send(Event::Error(error));
                }
                if snapshot.path == playback.requested {
                    playback.requested.clear();
                }
                if snapshot.transitioned != playback.transitioned {
                    playback.transitioned = snapshot.transitioned;
                    if snapshot.path == playback.preloaded {
                        if let Some(planned) = playback.planned_navigation.take() {
                            playback.navigation = planned;
                        }
                    }
                    playback.planned_navigation = None;
                    playback.preloaded.clear();
                }
                if snapshot.path != playback.path {
                    playback.path = snapshot.path.clone();
                    let was_manual = playback.navigation.manual.contains(&playback.path);
                    playback.navigation.record(&playback.path);
                    if let Some(id) = backend.id_for_path(&playback.path).ok().flatten() {
                        playback.navigation.mark_id(id, was_manual);
                    }
                    if !playback.path.is_empty() {
                        let at = revision.fetch_add(1, Ordering::Relaxed) + 1;
                        if let Ok(track) = backend.track(&playback.path) {
                            let _ = events.send(Event::Now(track));
                        }
                        analysis_jobs.submit((at, playback.path.clone(), false));
                        if audio {
                            lyrics_jobs.submit((at, playback.path.clone(), false));
                        }
                        let tracks = playback
                            .navigation
                            .upcoming_limited(&playback.path, 20, |id| {
                                backend.path_for_id(id).ok().flatten()
                            })
                            .into_iter()
                            .filter_map(|p| backend.track(&p).ok())
                            .collect();
                        let _ = events.send(Event::Queue(tracks));
                        let neighbors = playback
                            .navigation
                            .neighbors_compact(&playback.path, |id| {
                                backend.path_for_id(id).ok().flatten()
                            })
                            .into_iter()
                            .filter_map(|p| backend.track(&p).ok())
                            .collect();
                        let _ = events.send(Event::Neighbors(playback.path.clone(), neighbors));
                    } else {
                        revision.fetch_add(1, Ordering::Relaxed);
                        let _ = events.send(Event::Now(Track::default()));
                    }
                }
                if snapshot.ended != playback.ended {
                    playback.ended = snapshot.ended;
                    let mut planned = playback.navigation.clone();
                    if let Some(next) = planned.next_compact(&playback.path, false, true, |id| {
                        backend.path_for_id(id).ok().flatten()
                    }) {
                        let was_manual = planned.manual.contains(&next);
                        if let Err(error) = submit_play(
                            &mut playback.navigation,
                            &mut playback.requested,
                            planned,
                            next,
                            |target| {
                                backend.playback_command(PlaybackCommand::Play((target).into()))
                            },
                        ) {
                            let _ = events.send(Event::Error(error));
                        } else if let Some(id) =
                            backend.id_for_path(&playback.requested).ok().flatten()
                        {
                            playback.navigation.mark_id(id, was_manual);
                        }
                    }
                }
                if audio
                    && settings.gapless
                    && snapshot.playing
                    && snapshot.duration_ms > 0
                    && snapshot.duration_ms.saturating_sub(snapshot.position_ms) <= 5000
                    && playback.preloaded.is_empty()
                {
                    let mut planned = playback.navigation.clone();
                    if let Some(next) = planned.next_compact(&playback.path, false, true, |id| {
                        backend.path_for_id(id).ok().flatten()
                    }) {
                        if backend
                            .playback_command(PlaybackCommand::QueueNext((&next).into()))
                            .is_ok()
                        {
                            playback.preloaded = next;
                            playback.planned_navigation = Some(planned);
                        }
                    }
                }
                if snapshot.revision != last_revision {
                    last_revision = snapshot.revision;
                    library_change_pending = true;
                }
                if library_change_pending && !snapshot.scanning {
                    library_change_pending = false;
                    let _ =
                        events.send(Event::Statistics(backend.statistics().unwrap_or_default()));
                    if !initial_statistics {
                        let _ = events.send(Event::LibraryChanged);
                    }
                    initial_statistics = false;
                }
                if !snapshot.scanning && !source_mutation {
                    if was_scanning {
                        reconcile = true;
                    }
                    if let Some(folder) = pending_scans.pop_front() {
                        if let Err(error) = backend.start_scan(&folder) {
                            let _ = events.send(Event::Error(error));
                        } else {
                            if let Some(watcher) = watcher.as_mut() {
                                let roots = backend.library_roots().unwrap_or_default();
                                for error in watcher.sync(&roots) {
                                    let _ = events.send(Event::Error(error));
                                }
                            }
                        }
                    }
                }
                was_scanning = snapshot.scanning;
                if saved.elapsed() >= Duration::from_secs(2) {
                    if audio {
                        match session_writer.save(
                            &directory.join("slint-session.json"),
                            &playback.path,
                            snapshot.position_ms,
                            snapshot.volume,
                            &playback.navigation,
                        ) {
                            Ok(()) => session_save_failed = false,
                            Err(error) => {
                                if !session_save_failed {
                                    let _ = events.send(Event::Error(format!(
                                        "Could not save playback session: {error}"
                                    )));
                                }
                                session_save_failed = true;
                            }
                        }
                    }
                    saved = Instant::now();
                }
                let _ = events.send(Event::Playback(snapshot));
            }
            Err(error) => {
                let _ = events.send(Event::Error(error));
            }
        }
    }
    if audio {
        // A last slider change may still be queued when the window closes.
        session.volume = shutdown_volume(&requests, session.volume);
        if let Ok(snapshot) = backend.snapshot() {
            session = Session {
                path: snapshot.path,
                position: snapshot.position_ms,
                volume: session.volume,
                navigation: playback.navigation,
            };
        }
        if let Err(error) = write_json(&directory.join("slint-session.json"), &session) {
            eprintln!("Could not save playback session on shutdown: {error}");
        }
    }
    backend.cancel_scan();
    backend.shutdown();
}
