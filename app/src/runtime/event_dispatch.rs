//! Drain bounded batches of worker events and reject results for stale queries or drafts.
use super::RuntimeModels;
use crate::controllers::{
    collections::stage_collection_draft, folders, preferences::register_font,
};
use crate::{
    errors, metadata, models,
    protocol::{Event, Request},
    OrcaTray, OrcaWindow, PlaylistChoice,
};
use slint::VecModel;
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
pub(super) fn drain_events(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    tray: &Option<slint::Weak<OrcaTray>>,
    neighbor_revision: &Cell<u64>,
    artwork_epoch: &Cell<u64>,
) -> bool {
    let RuntimeModels {
        worker,
        player,
        catalog,
        ..
    } = models;
    let crate::playback::state::PlayerModels {
        lyric_renderer,
        queue_tracks,
        now,
        lines,
        preview,
        neighbors,
        ..
    } = player.as_ref();
    let models::CatalogModels {
        tracks,
        list,
        grid,
        groups,
        ..
    } = catalog.as_ref();
    let columns = grid.columns.get();

    let mut update_images = false;
    let started = Instant::now();
    let mut processed = 0;
    while processed < 64 && (processed == 0 || started.elapsed() < Duration::from_millis(4)) {
        let Ok(event) = worker.rx.try_recv() else {
            break;
        };
        processed += 1;
        match event {
            Event::OutputDevices(result) => match result {
                Ok(devices) => {
                    let mut choices = vec![crate::OutputChoice {
                        id: "".into(),
                        name: "System default".into(),
                    }];
                    for device in devices {
                        choices.push(crate::OutputChoice {
                            id: device.id.into(),
                            name: format!(
                                "{}{}",
                                device.name,
                                if device.is_default { " (default)" } else { "" }
                            )
                            .into(),
                        });
                    }
                    let wanted = ui.player().get_output_device_id();
                    if !wanted.is_empty() && !choices.iter().any(|d| d.id == wanted) {
                        choices.push(crate::OutputChoice {
                            id: wanted.clone(),
                            name: "Saved device (unavailable)".into(),
                        });
                    }
                    let selected = choices.iter().position(|d| d.id == wanted).unwrap_or(0);
                    ui.player().set_output_names(
                        Rc::new(VecModel::from(
                            choices.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
                        ))
                        .into(),
                    );
                    ui.player()
                        .set_output_choices(Rc::new(VecModel::from(choices)).into());
                    ui.player().set_output_device_index(selected as i32);
                }
                Err(error) => ui.player().set_output_error(error.into()),
            },
            Event::Font(path, bytes) => match register_font(bytes) {
                Ok(family) => {
                    ui.preferences()
                        .set_custom_font_family(family.clone().into());
                    ui.preferences().set_font_family(family.into());
                    ui.preferences().set_font_path(path.into());
                    ui.preferences().invoke_save_settings();
                }
                Err(error) => ui.shell().set_error(error.into()),
            },
            Event::Ready => ui.library().invoke_browse(),
            Event::LibraryChanged => ui.library().invoke_browse(),
            Event::CollectionDraft(value) => {
                if let Ok(value) = serde_json::from_str(&value) {
                    update_images |= stage_collection_draft(ui, &value);
                }
            }
            Event::GroupGone(generation, kind, key, secondary)
                if generation == worker.latest_query.load(Ordering::Relaxed)
                    && ui.shell().get_view() == kind
                    && ui.library().get_detail_key() == key
                    && ui.library().get_detail_secondary() == secondary =>
            {
                ui.library().invoke_go_back();
            }
            Event::PagesSkipped(generation, offsets) => {
                tracks.discard_pages(generation, &offsets);
                if generation == worker.latest_query.load(Ordering::Relaxed) {
                    let first = ui.library().get_song_first().max(0) as usize;
                    let end = ui.library().get_song_end().max(0) as usize;
                    for offset in offsets {
                        if ui.shell().get_view() == "songs"
                            && ui.preferences().get_grid()
                            && ui.library().get_detail_key().is_empty()
                        {
                            let rows: Vec<_> = {
                                let visible = grid.visible_rows.borrow();
                                (offset as usize / columns.max(1)
                                    ..(offset as usize + 128).div_ceil(columns.max(1)))
                                    .filter(|row| {
                                        visible
                                            .get(row)
                                            .is_some_and(|model| model.strong_count() > 0)
                                    })
                                    .collect()
                            };
                            for row in rows {
                                grid.refresh_row(row);
                                grid.notify.row_changed(row);
                            }
                        } else {
                            let start = (offset as usize).max(first);
                            let stop = (offset as usize + 128).min(end);
                            for index in start..stop {
                                list.notify.row_changed(index);
                            }
                        }
                    }
                }
            }
            Event::Page(generation, offset, count, rows)
                if generation == worker.latest_query.load(Ordering::Relaxed) =>
            {
                super::catalog_events::apply_page(
                    ui,
                    models,
                    artwork_epoch,
                    generation,
                    offset,
                    count,
                    rows,
                );
            }
            Event::Groups(generation, rows)
                if generation == worker.latest_query.load(Ordering::Relaxed) =>
            {
                super::catalog_events::apply_groups(ui, models, artwork_epoch, rows);
            }
            Event::Statistics(stats) => {
                ui.library().set_known_artists(stats.artists as i32);
                ui.library().set_known_albums(stats.albums as i32);
                ui.library().set_known_genres(stats.genres as i32);
                ui.library().set_known_playlists(
                    stats.playlists as i32 + i32::from(ui.preferences().get_show_liked_songs()),
                );
                ui.library()
                    .set_summary(format!("{} songs", stats.songs).into());
                ui.library().set_folders(
                    Rc::new(VecModel::from(
                        stats
                            .roots
                            .into_iter()
                            .map(Into::into)
                            .collect::<Vec<slint::SharedString>>(),
                    ))
                    .into(),
                );
            }
            Event::FolderChildren {
                root,
                current,
                adding,
                open,
                nodes,
            } => {
                if !open && !ui.dialogs().get_folder_dialog() {
                    continue;
                }
                if ui.dialogs().get_folder_dialog()
                    && (ui.dialogs().get_folder_root() != root
                        || ui.dialogs().get_folder_adding() != adding)
                {
                    continue;
                }
                if !ui.dialogs().get_folder_dialog() && current == root {
                    ui.dialogs()
                        .set_folder_rules(Rc::new(VecModel::default()).into());
                }
                let rows = folders::choices(nodes, &folders::drafts(&ui.dialogs()));
                ui.dialogs().set_folder_root(root.into());
                ui.dialogs().set_folder_current(current.into());
                ui.dialogs().set_folder_adding(adding);
                ui.dialogs()
                    .set_folder_choices(Rc::new(VecModel::from(rows)).into());
                ui.dialogs().set_folder_dialog(true);
            }
            Event::Playback(snapshot) => {
                super::playback_events::apply_snapshot(ui, models, tray, snapshot)
            }
            Event::Now(track) => {
                update_images |=
                    super::playback_events::apply_track(ui, models, neighbor_revision, track);
            }
            Event::Neighbors(path, rows) if path == now.borrow().path => {
                *neighbors.borrow_mut() = rows;
                neighbor_revision.set(neighbor_revision.get().wrapping_add(1));
            }
            Event::Queue(rows) => {
                *queue_tracks.borrow_mut() = rows;
                update_images = true;
            }
            Event::Modes(shuffle, repeat) => {
                ui.player().set_shuffle(shuffle);
                ui.player().set_repeat(repeat as i32);
            }
            Event::Analysis(generation, path, text, peaks)
                if generation == worker.analysis_generation.load(Ordering::Relaxed)
                    && path == now.borrow().path =>
            {
                super::lyric_events::apply_analysis(ui, models, text, peaks);
            }
            Event::Lyrics(generation, text, status, refresh)
                if generation == worker.analysis_generation.load(Ordering::Relaxed) =>
            {
                super::lyric_events::apply_lyrics(ui, models, text, status, refresh);
            }
            Event::Romanized(generation, romanized)
                if (1..=2).contains(&ui.preferences().get_romanization_mode())
                    && generation == worker.romanization_generation.load(Ordering::Relaxed)
                    && romanized.len() == lines.borrow().len() =>
            {
                lyric_renderer.borrow_mut().set_romanization(romanized);
            }
            Event::Context(track) => {
                ui.dialogs().set_menu_song(models::text_song(&track, 0));
                ui.dialogs().set_menu_open(true);
            }
            Event::Playlists(items) => {
                ui.library().set_playlists(
                    Rc::new(models::PlaylistMenu {
                        groups: items
                            .into_iter()
                            .filter(|group| group.key != "liked")
                            .collect(),
                        cache: groups.cache.clone(),
                        edge: models::physical_artwork_edge(22, ui.shell().get_display_dpr()),
                        notify: slint::ModelNotify::default(),
                        recent: std::cell::RefCell::new(Default::default()),
                    })
                    .into(),
                );
            }
            Event::PlaylistChoices(song_id, groups, selected)
                if ui.player().get_now().id as i64 == song_id =>
            {
                ui.dialogs().set_playlist_choices(
                    Rc::new(VecModel::from(
                        groups
                            .into_iter()
                            .map(|group| PlaylistChoice {
                                key: group.key.clone().into(),
                                title: group.title.into(),
                                selected: group
                                    .key
                                    .parse::<i64>()
                                    .is_ok_and(|id| selected.contains(&id)),
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into(),
                );
                ui.dialogs().set_playlist_picker_loading(false);
            }
            Event::LikedStatus(song_id, liked) => {
                if now.borrow().id == song_id {
                    ui.player().set_liked(liked);
                }
                let changed = tracks.set_liked(song_id, liked);
                for &index in &changed {
                    list.notify.row_changed(index);
                }
                grid.artwork_changed(&changed);
            }
            Event::PlaybackCollection(collection) => {
                let collection = collection.unwrap_or_default();
                ui.player().set_playback_kind(collection.kind.into());
                ui.player().set_playback_key(collection.key.into());
                ui.player()
                    .set_playback_secondary(collection.secondary.into());
            }
            Event::Jump(generation, index)
                if generation == worker.latest_query.load(Ordering::Relaxed) =>
            {
                ui.library().set_jump_index(index as i32);
                ui.library()
                    .set_jump_revision(ui.library().get_jump_revision().wrapping_add(1));
            }
            Event::Metadata(document, fetch_cover, generation)
                if generation == ui.dialogs().get_editor_generation() =>
            {
                ui.dialogs().set_draft(metadata::from_document(document));
                ui.dialogs().set_metadata_open(true);
                ui.dialogs().set_editor_busy(false);
                ui.dialogs().set_editor_status("".into());
                update_images = true;
                if fetch_cover {
                    ui.dialogs()
                        .invoke_metadata_action("fetch-cover".into(), "".into());
                }
            }
            Event::MetadataAsset(kind, target, value, generation)
                if generation == ui.dialogs().get_editor_generation()
                    && ui.dialogs().get_metadata_open()
                    && ui.dialogs().get_draft().path == target.as_str() =>
            {
                let mut draft = ui.dialogs().get_draft();
                if kind == crate::protocol::MetadataAssetKind::Cover {
                    draft.cover_to_embed = value.into();
                    draft.remove_cover = false;
                    ui.dialogs()
                        .set_editor_status("Artwork ready — Save metadata to embed it".into());
                } else {
                    draft.lyrics = value.into();
                }
                ui.dialogs().set_draft(draft);
                update_images = true;
            }
            Event::MetadataSaved(path, generation) => {
                // New embedded covers have new content-hashed paths. Preserve
                // unrelated decoded covers while fresh catalog rows arrive.
                if generation == ui.dialogs().get_editor_generation()
                    && ui.dialogs().get_draft().path == path.as_str()
                {
                    ui.dialogs().set_editor_busy(false);
                    ui.dialogs().set_metadata_open(false);
                }
                ui.library().invoke_browse();
            }
            Event::MetadataFailed(generation, error)
                if generation == ui.dialogs().get_editor_generation() =>
            {
                ui.dialogs().set_editor_busy(false);
                ui.dialogs().set_editor_status(error.into());
            }
            Event::PlaylistChanged(result) => {
                ui.dialogs().set_playlist_dialog(false);
                if result == crate::protocol::PlaylistChange::Deleted {
                    ui.library().invoke_go_back();
                } else if result == crate::protocol::PlaylistChange::Renamed {
                    ui.library()
                        .set_detail_title(ui.dialogs().get_playlist_name());
                    ui.library().invoke_browse();
                } else if result != crate::protocol::PlaylistChange::Membership
                    || ui.shell().get_view() == "playlists"
                {
                    ui.library().invoke_browse();
                }
            }
            Event::OperationFailed(request, error) => {
                if matches!(&request, crate::protocol::OperationRequest::ArtworkBulk(_)) {
                    ui.library().set_artwork_bulk_running(false);
                    ui.library()
                        .set_artwork_bulk_progress("Paused. Please try again.".into());
                }
                eprintln!("Orca: {error}");
                if let crate::protocol::OperationRequest::CollectionEdit(_) = &request {
                    ui.dialogs().set_collection_saving(false);
                    let _ = worker.tx.send(Request::Reconcile);
                    ui.library().invoke_browse();
                }
                if let crate::protocol::OperationRequest::FetchCover(fetch) = &request {
                    if fetch.collection_draft
                        && !crate::controllers::collections::finish_collection_fetch(
                            ui,
                            fetch,
                            &errors::operation(&error),
                        )
                    {
                        continue;
                    }
                    if fetch.editor_path.is_some()
                        && (!ui.dialogs().get_metadata_open()
                            || !metadata::artwork_target_matches(
                                fetch,
                                &ui.dialogs().get_draft(),
                                ui.dialogs().get_editor_generation(),
                            ))
                    {
                        continue;
                    }
                }
                if let crate::protocol::OperationRequest::FetchLyrics(fetch) = &request {
                    if fetch.editor_path.is_some()
                        && (!ui.dialogs().get_metadata_open()
                            || fetch.editor_path.as_deref()
                                != Some(ui.dialogs().get_draft().path.as_str())
                            || fetch.editor_generation.is_some_and(|generation| {
                                generation != ui.dialogs().get_editor_generation()
                            }))
                    {
                        continue;
                    }
                }
                let message = errors::operation(&error);
                ui.shell().set_error(message.clone().into());
                if matches!(&request,crate::protocol::OperationRequest::FetchCover(fetch) if fetch.editor_path.is_some())
                    || matches!(&request,crate::protocol::OperationRequest::FetchLyrics(fetch) if fetch.editor_path.is_some())
                {
                    ui.dialogs().set_editor_status(message.into());
                    ui.dialogs().set_editor_busy(false);
                }
                // Release the library mutation guard after a failed removal too.
                if request.removes_source() {
                    let _ = worker.tx.send(Request::SourceChanged);
                }
            }
            Event::Operation(request, result) => {
                update_images |=
                    super::operation_results::apply_operation_result(ui, models, request, result);
            }
            Event::CatalogDetail(generation, request, result)
                if generation == worker.latest_query.load(Ordering::Relaxed) =>
            {
                update_images |=
                    super::operation_results::apply_operation_result(ui, models, request, result);
            }
            Event::AutomaticArtworkUpdated => {
                // Artwork paths are content-hashed. Retain decoded images for
                // unchanged paths instead of clearing every visible cover.
                let range = if ui.shell().get_view() == "songs"
                    && ui.preferences().get_grid()
                    && ui.library().get_detail_key().is_empty()
                {
                    grid.nearby_range()
                } else {
                    Some((
                        ui.library().get_song_first().max(0) as usize,
                        ui.library().get_song_end().max(0) as usize,
                    ))
                };
                let offsets = range
                    .map(|(start, end)| {
                        (start / 128..end.div_ceil(128))
                            .take(8)
                            .map(|page| (page * 128) as u32)
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = worker.tx.send(Request::ArtworkRefresh(
                    worker.latest_query.load(Ordering::Relaxed),
                    offsets,
                ));
            }
            Event::CatalogError(generation, error)
                if generation == worker.latest_query.load(Ordering::Relaxed) =>
            {
                ui.library().set_catalog_loading(false);
                eprintln!("Orca catalog: {error}");
                ui.shell().set_error(errors::friendly(&error).into());
            }
            Event::Error(error) => {
                eprintln!("Orca: {error}");
                ui.dialogs().set_playlist_picker_loading(false);
                let message = errors::friendly(&error);
                ui.player()
                    .set_audio_error_banner(if errors::is_playback_failure(&error) {
                        message.clone().into()
                    } else {
                        "".into()
                    });
                ui.shell().set_error(message.clone().into());
                *preview.borrow_mut() = None;
            }
            _ => {}
        }
    }
    update_images
}
