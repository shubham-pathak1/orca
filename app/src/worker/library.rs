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
    types::{PlaybackCommand, Query, Track},
};
use std::{
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
    pub analysis_jobs: LatestJob<(u64, String)>,
    pub lyrics_jobs: LatestJob<(u64, String)>,
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
    if settings.auto_artwork {
        let _ = operations.try_send(OperationRequest::MissingArtwork);
    }
    let mut session = read_session(&directory.join("slint-session.json")).unwrap_or_default();
    let mut session_writer = SessionWriter::default();
    let mut playback = PlaybackState {
        navigation: std::mem::take(&mut session.navigation),
        ..Default::default()
    };
    let mut catalog = CatalogState::default();
    let mut saved = Instant::now();
    let mut last_revision = 0;
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
    let roots = backend.statistics().map(|s| s.roots).unwrap_or_default();
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
    let _ = events.send(Event::Ready);
    while !worker_stop.load(Ordering::Relaxed) {
        let mut shutdown = false;
        if let Ok(request) = requests.recv_timeout(Duration::from_millis(200)) {
            let refresh_queue = matches!(
                &request,
                Request::Play(..) | Request::Queue(..) | Request::Collection(..)
            ) || matches!(&request,Request::Command(action) if action.refresh_queue());
            let result: Result<(), String> = (|| {
                match request {
                    Request::Browse(at, new_query, kind) => {
                        catalog.browse(&backend, &events, at, new_query, kind, &query_revision)?
                    }
                    Request::Page(at, offset) => {
                        catalog.page(&backend, &events, at, offset, &query_revision)?
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
                    Request::Reconcile => {
                        reconcile = true;
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
                        for folder in backend.statistics()?.roots {
                            if !pending_scans.contains(&folder) {
                                pending_scans.push_back(folder);
                            }
                        }
                    }
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
                            analysis_jobs.submit((at, playback.path.clone()));
                            if audio {
                                lyrics_jobs.submit((at, playback.path.clone()));
                            }
                        }
                    }
                    Request::Context(target) => {
                        let _ = events.send(Event::Playlists(backend.groups("playlists", "")?));
                        let _ = events.send(Event::Context(backend.track(&target)?));
                    }
                    Request::Playlist(edit) => {
                        let action = backend.edit_playlist(edit)?;
                        let _ = events.send(Event::Playlists(backend.groups("playlists", "")?));
                        let _ = events.send(Event::PlaylistChanged(action));
                    }
                    Request::Settings(value) => {
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
                        let enabled = value.auto_artwork && !settings.auto_artwork;
                        settings = value;
                        auto_artwork.store(settings.auto_artwork, Ordering::Relaxed);
                        write_json(&directory.join("slint-settings.json"), &settings)?;
                        if enabled {
                            let _ = operations.try_send(OperationRequest::MissingArtwork);
                        }
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
                    .upcoming(&playback.path)
                    .into_iter()
                    .take(20)
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
                    .neighbors(current)
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
            let roots = backend.statistics().map(|s| s.roots).unwrap_or_default();
            pending_scans.retain(|root| roots.contains(root));
            if let Some(watcher) = watcher.as_mut() {
                for error in watcher.sync(&roots) {
                    let _ = events.send(Event::Error(error));
                }
            }
            if let Ok(paths) = backend.context_paths(&Query::default()) {
                let allowed: std::collections::HashSet<_> = paths.into_iter().collect();
                playback.navigation.prune(&allowed);
                if !playback.preloaded.is_empty() && !allowed.contains(&playback.preloaded) {
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
                if !playback.path.is_empty() && !allowed.contains(&playback.path) {
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
                    .upcoming(&playback.path)
                    .into_iter()
                    .take(20)
                    .filter_map(|p| backend.track(&p).ok())
                    .collect();
                let _ = events.send(Event::Queue(rows));
                let _ = events.send(Event::Statistics(backend.statistics().unwrap_or_default()));
                let _ = events.send(Event::LibraryChanged);
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
                    playback.navigation.record(&playback.path);
                    if !playback.path.is_empty() {
                        let at = revision.fetch_add(1, Ordering::Relaxed) + 1;
                        if let Ok(track) = backend.track(&playback.path) {
                            let _ = events.send(Event::Now(track));
                        }
                        analysis_jobs.submit((at, playback.path.clone()));
                        if audio {
                            lyrics_jobs.submit((at, playback.path.clone()));
                        }
                        let tracks = playback
                            .navigation
                            .upcoming(&playback.path)
                            .into_iter()
                            .take(20)
                            .filter_map(|p| backend.track(&p).ok())
                            .collect();
                        let _ = events.send(Event::Queue(tracks));
                        let neighbors = playback
                            .navigation
                            .neighbors(&playback.path)
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
                    if let Some(next) = planned.next(&playback.path, false, true) {
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
                    if let Some(next) = planned.next(&playback.path, false, true) {
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
                    let _ =
                        events.send(Event::Statistics(backend.statistics().unwrap_or_default()));
                    let _ = events.send(Event::LibraryChanged);
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
                                let roots =
                                    backend.statistics().map(|s| s.roots).unwrap_or_default();
                                for error in watcher.sync(&roots) {
                                    let _ = events.send(Event::Error(error));
                                }
                            }
                        }
                    } else if was_scanning && settings.auto_artwork {
                        let _ = operations.try_send(OperationRequest::MissingArtwork);
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
