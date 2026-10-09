//! Apply service results to UI models. Late results are checked against current identity/revision.
use super::RuntimeModels;
use crate::controllers::{collections::stage_collection_draft, preferences::register_font};
use crate::presentation::{reset_empty_player, waveform};
use crate::{
    errors, lyrics, metadata, models,
    protocol::{Event, Request},
    AppState, CatalogGroup, LyricLine, OrcaTray, OrcaWindow,
};
use slint::{ComponentHandle, VecModel};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
pub(super) fn apply(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    poll_tray: &Option<slint::Weak<OrcaTray>>,
    neighbor_revision: &Cell<u64>,
    artwork_epoch: &Cell<u64>,
) -> bool {
    let RuntimeModels {
        service: poll_service,
        lyric_renderer,
        playback_clock,
        tracks,
        list,
        grid,
        groups,
        neighbors,
        queue_tracks,
        now,
        lines,
        preview,
        ..
    } = models;
    let columns = grid.columns.get();
    let s = ui.global::<AppState>();
    let mut update_images = false;
    for event in poll_service.rx.try_iter() {
        match event {
            Event::Font(path, bytes) => match register_font(bytes) {
                Ok(family) => {
                    s.set_font_family(family.into());
                    s.set_font_path(path.into());
                    s.invoke_save_settings();
                }
                Err(error) => s.set_error(error.into()),
            },
            Event::Ready => s.invoke_browse(),
            Event::LibraryChanged => s.invoke_browse(),
            Event::CollectionDraft(value) => {
                if let Ok(value) = serde_json::from_str(&value) {
                    update_images |= stage_collection_draft(ui, &value);
                }
            }
            Event::GroupGone(kind, key, secondary)
                if s.get_view() == kind
                    && s.get_detail_key() == key
                    && s.get_detail_secondary() == secondary =>
            {
                s.invoke_go_back();
            }
            Event::Page(at, offset, count, rows)
                if at == poll_service.latest_query.load(Ordering::Relaxed) =>
            {
                let reset = tracks.generation.get() != at || tracks.count.get() != count as usize;
                let length = rows.len();
                tracks.apply(at, offset as usize, count as usize, rows);
                artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
                if reset {
                    list.notify.reset();
                    grid.notify.reset();
                } else {
                    // Notify only the active viewport. Offscreen delegates read
                    // the latest cached page when Slint creates them.
                    if !s.get_detail_key().is_empty() || s.get_view() == "songs" && !s.get_grid() {
                        let start = (offset as usize).max(s.get_song_first().max(0) as usize);
                        let end = (offset as usize + length).min(s.get_song_end().max(0) as usize);
                        for index in start..end {
                            list.notify.row_changed(index);
                        }
                    } else if s.get_view() == "songs" && s.get_grid() {
                        let rows: Vec<_> = {
                            let visible = grid.visible_rows.borrow();
                            (offset as usize / columns
                                ..(offset as usize + length).div_ceil(columns))
                                .filter(|row| {
                                    visible
                                        .get(row)
                                        .is_some_and(|model| model.strong_count() > 0)
                                })
                                .collect()
                        };
                        for row in rows {
                            grid.notify.row_changed(row);
                        }
                    }
                }
            }
            Event::Groups(at, rows) if at == poll_service.latest_query.load(Ordering::Relaxed) => {
                s.set_catalog_summary(
                    format!(
                        "{} {}",
                        rows.len(),
                        if rows.len() == 1 {
                            s.get_view().trim_end_matches('s').to_string()
                        } else {
                            s.get_view().to_string()
                        }
                    )
                    .into(),
                );
                *groups.groups.borrow_mut() = rows;
                groups.reset();
                artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
            }
            Event::Statistics(stats) => {
                s.set_summary(
                    format!(
                        "{} songs / {} artists / {} albums",
                        stats.songs, stats.artists, stats.albums
                    )
                    .into(),
                );
                s.set_folders(
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
            Event::Playback(snapshot) => {
                if let Some(tray) = poll_tray.as_ref().and_then(slint::Weak::upgrade) {
                    tray.set_playing(snapshot.playing);
                }
                let mut seek = preview.borrow_mut();
                if seek.as_ref().is_some_and(|p| {
                    p.path != snapshot.path
                        || p.started.elapsed() > Duration::from_secs(2)
                        || (snapshot.position_ms as f32 - p.target).abs() < 750.0
                        || !snapshot.error.is_empty()
                }) {
                    *seek = None;
                }
                let position = seek
                    .as_ref()
                    .map_or(snapshot.position_ms as f32, |p| p.target);
                s.set_position(position);
                s.set_lyric_position(position);
                playback_clock.borrow_mut().update(
                    position,
                    snapshot.duration_ms as f32,
                    snapshot.playing,
                    Instant::now(),
                );
                s.set_elapsed(models::duration(position.max(0.0) as u64).into());
                s.set_duration(snapshot.duration_ms as f32);
                s.set_total(models::duration(snapshot.duration_ms).into());
                s.set_playing(snapshot.playing);
                s.set_volume(snapshot.volume);
                s.set_active_line(lyrics::active(&lines.borrow(), position));
                if !snapshot.error.is_empty() {
                    s.set_error(errors::friendly(&snapshot.error).into());
                }
            }
            Event::Now(track) => {
                if *now.borrow() == track {
                    continue;
                }
                let changed_song = now.borrow().path != track.path;
                if !changed_song {
                    // A collection/tag/artwork refresh is not a new song. Keep
                    // the current lyric layout, focus and neighbor prefetch.
                    *now.borrow_mut() = track;
                    update_images = true;
                    continue;
                }
                neighbors.borrow_mut().clear();
                neighbor_revision.set(neighbor_revision.get().wrapping_add(1));
                if track.path.is_empty() {
                    reset_empty_player(ui);
                    *preview.borrow_mut() = None;
                }
                *now.borrow_mut() = track;
                lines.borrow_mut().clear();
                lyric_renderer.borrow_mut().invalidate();
                s.set_lyrics(Rc::new(VecModel::<LyricLine>::default()).into());
                s.set_active_line(-1);
                s.set_lyrics_status(
                    if now.borrow().path.is_empty() {
                        ""
                    } else {
                        "Loading lyrics…"
                    }
                    .into(),
                );
                update_images = true;
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
                s.set_shuffle(shuffle);
                s.set_repeat(repeat as i32);
            }
            Event::Analysis(at, path, text, peaks)
                if at == poll_service.analysis_generation.load(Ordering::Relaxed)
                    && path == now.borrow().path =>
            {
                if !text.is_empty() && lines.borrow().is_empty() {
                    if s.get_metadata_open()
                        && s.get_draft().path.as_str() == now.borrow().path
                        && s.get_draft().lyrics.trim().is_empty()
                        && s.get_editor_status().starts_with("Could not connect")
                    {
                        let mut draft = s.get_draft();
                        draft.lyrics = text.clone().into();
                        s.set_draft(draft);
                        s.set_editor_status("Lyrics ready - Save metadata to embed them".into());
                    }
                    let parsed = lyrics::parse(&text);
                    s.set_lyrics(Rc::new(VecModel::from(parsed.clone())).into());
                    *lines.borrow_mut() = parsed;
                    lyric_renderer.borrow_mut().invalidate();
                    s.set_lyrics_status("".into());
                }
                s.set_waveform(waveform(&peaks));
                s.set_active_line(lyrics::active(&lines.borrow(), s.get_position()));
            }
            Event::Lyrics(at, text, status)
                if at == poll_service.analysis_generation.load(Ordering::Relaxed) =>
            {
                if !text.is_empty() {
                    if s.get_metadata_open()
                        && s.get_draft().path.as_str() == now.borrow().path
                        && s.get_draft().lyrics.trim().is_empty()
                        && s.get_editor_status().starts_with("Could not connect")
                    {
                        let mut draft = s.get_draft();
                        draft.lyrics = text.clone().into();
                        s.set_draft(draft);
                        s.set_editor_status("Lyrics ready - Save metadata to embed them".into());
                    }
                    let parsed = lyrics::parse(&text);
                    s.set_lyrics(Rc::new(VecModel::from(parsed.clone())).into());
                    *lines.borrow_mut() = parsed;
                    lyric_renderer.borrow_mut().invalidate();
                    s.set_active_line(lyrics::active(&lines.borrow(), s.get_position()));
                }
                s.set_lyrics_status(status.into());
            }
            Event::Context(track) => {
                s.set_menu_song(models::text_song(&track, 0));
                s.set_menu_open(true);
            }
            Event::Playlists(groups) => {
                s.set_playlists(
                    Rc::new(VecModel::from(
                        groups
                            .into_iter()
                            .map(|group| CatalogGroup {
                                key: group.key.into(),
                                secondary: group.secondary.into(),
                                title: group.title.into(),
                                subtitle: group.subtitle.into(),
                                count: group.count as i32,
                                cover_missing: group.artwork.is_empty(),
                                cover: slint::Image::default(),
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into(),
                );
            }
            Event::Jump(at, index) if at == poll_service.latest_query.load(Ordering::Relaxed) => {
                s.set_jump_index(index as i32);
                s.set_jump_revision(s.get_jump_revision().wrapping_add(1));
            }
            Event::Metadata(document, fetch_cover, generation)
                if generation == s.get_editor_generation() =>
            {
                s.set_draft(metadata::from_document(document));
                s.set_metadata_open(true);
                s.set_editor_busy(false);
                s.set_editor_status("".into());
                update_images = true;
                if fetch_cover {
                    s.invoke_metadata_action("fetch-cover".into(), "".into());
                }
            }
            Event::MetadataAsset(kind, target, value, generation)
                if generation == s.get_editor_generation()
                    && s.get_metadata_open()
                    && s.get_draft().path == target.as_str() =>
            {
                let mut draft = s.get_draft();
                if kind == crate::protocol::MetadataAssetKind::Cover {
                    draft.cover_to_embed = value.into();
                    draft.remove_cover = false;
                    s.set_editor_status("Artwork ready — Save metadata to embed it".into());
                } else {
                    draft.lyrics = value.into();
                }
                s.set_draft(draft);
                update_images = true;
            }
            Event::MetadataSaved(path, generation) => {
                // New embedded covers have new content-hashed paths. Preserve
                // unrelated decoded covers while fresh catalog rows arrive.
                if generation == s.get_editor_generation() && s.get_draft().path == path.as_str() {
                    s.set_editor_busy(false);
                    s.set_metadata_open(false);
                }
                s.invoke_browse();
            }
            Event::MetadataFailed(generation, error) if generation == s.get_editor_generation() => {
                s.set_editor_busy(false);
                s.set_editor_status(error.into());
            }
            Event::PlaylistChanged(result) => {
                s.set_playlist_dialog(false);
                if result == crate::protocol::PlaylistChange::Deleted {
                    s.invoke_go_back();
                } else if result == crate::protocol::PlaylistChange::Renamed {
                    s.set_detail_title(s.get_playlist_name());
                    s.invoke_browse();
                } else {
                    s.invoke_browse();
                }
            }
            Event::OperationFailed(request, error) => {
                eprintln!("Orca: {error}");
                if let crate::protocol::OperationRequest::CollectionEdit(_) = &request {
                    s.set_collection_saving(false);
                    let _ = poll_service.tx.send(Request::Reconcile);
                    s.invoke_browse();
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
                        && (!s.get_metadata_open()
                            || !metadata::artwork_target_matches(
                                fetch,
                                &s.get_draft(),
                                s.get_editor_generation(),
                            ))
                    {
                        continue;
                    }
                }
                if let crate::protocol::OperationRequest::FetchLyrics(fetch) = &request {
                    if fetch.editor_path.is_some()
                        && (!s.get_metadata_open()
                            || fetch.editor_path.as_deref() != Some(s.get_draft().path.as_str())
                            || fetch
                                .editor_generation
                                .is_some_and(|at| at != s.get_editor_generation()))
                    {
                        continue;
                    }
                }
                let message = errors::operation(&error);
                s.set_error(message.clone().into());
                if matches!(&request,crate::protocol::OperationRequest::FetchCover(fetch) if fetch.editor_path.is_some())
                    || matches!(&request,crate::protocol::OperationRequest::FetchLyrics(fetch) if fetch.editor_path.is_some())
                {
                    s.set_editor_status(message.into());
                    s.set_editor_busy(false);
                }
                // Release the library mutation guard after a failed removal too.
                if request.removes_source() {
                    let _ = poll_service.tx.send(Request::SourceChanged);
                }
            }
            Event::Operation(request, result) => {
                update_images |= super::operations::apply(ui, models, request, result);
            }
            Event::AutomaticArtworkUpdated => {
                // Artwork paths are content-hashed. Retain decoded images for
                // unchanged paths instead of clearing every visible cover.
                let _ = poll_service.tx.send(Request::Reconcile);
                s.invoke_browse();
            }
            Event::Error(error) => {
                eprintln!("Orca: {error}");
                let message = errors::friendly(&error);
                s.set_error(message.clone().into());
                *preview.borrow_mut() = None;
            }
            _ => {}
        }
    }
    update_images
}
