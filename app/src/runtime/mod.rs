//! UI runtime: drain service events and update visible models/render assets.
//! Owns the timer's retained state; shutdown stops/drops this timer before Service.
mod events;
mod layout;
mod operations;
mod scheduling;
#[cfg(test)]
pub(crate) mod tests;
use crate::presentation::theme_accent;
use crate::{
    artwork, lyric_render, lyrics, models, playback_clock, worker::Service, AppState, LyricLine,
    OrcaTray, OrcaWindow, Song,
};
use orca_services::types::Track;
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
pub(crate) struct SeekPreview {
    pub(crate) target: f32,
    pub(crate) path: String,
    pub(crate) started: Instant,
}

pub(crate) struct RuntimeModels {
    pub(crate) service: Rc<Service>,
    pub(crate) cache: Rc<RefCell<artwork::ArtworkCache>>,
    pub(crate) lyric_renderer: Rc<RefCell<lyric_render::Renderer>>,
    pub(crate) playback_clock: Rc<RefCell<playback_clock::PlaybackClock>>,
    pub(crate) tracks: Rc<models::Tracks>,
    pub(crate) list: Rc<models::SongList>,
    pub(crate) grid: Rc<models::SongGrid>,
    pub(crate) groups: Rc<models::GroupGrid>,
    pub(crate) related: Rc<models::GroupGrid>,
    pub(crate) neighbors: Rc<RefCell<Vec<Track>>>,
    pub(crate) detail_art: Rc<RefCell<(String, String)>>,
    pub(crate) queue: Rc<VecModel<Song>>,
    pub(crate) queue_tracks: Rc<RefCell<Vec<Track>>>,
    pub(crate) now: Rc<RefCell<Track>>,
    pub(crate) lines: Rc<RefCell<Vec<LyricLine>>>,
    pub(crate) preview: Rc<RefCell<Option<SeekPreview>>>,
}
pub(crate) fn install(
    ui: &OrcaWindow,
    poll_tray: Option<slint::Weak<OrcaTray>>,
    models: RuntimeModels,
) -> Timer {
    let schedule = scheduling::ArtworkSchedule::default();
    let neighbor_revision = Cell::new(0u64);
    let artwork_epoch = Cell::new(0u64);
    let visual_settings = Cell::new(None);
    let image_geometry = Cell::new((0u32, 0u32, 0u32, false, false));
    let player_cover_edge = Cell::new(0u32);

    let poll = Timer::default();
    let weak = ui.as_weak();
    poll.start(TimerMode::Repeated, Duration::from_millis(33), move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let RuntimeModels {
            service: _,
            cache,
            lyric_renderer,
            playback_clock,
            tracks,
            list,
            grid,
            groups,
            related,
            neighbors: _,
            detail_art,
            queue,
            queue_tracks,
            now,
            lines,
            preview,
        } = &models;
        let s = ui.global::<AppState>();
        let mut update_images = false;
        let visuals = (
            s.get_browse_artwork(),
            s.get_compact_artwork(),
            s.get_full_artwork(),
            s.get_blurred_background(),
            s.get_dynamic_accent(),
        );
        if visual_settings.replace(Some(visuals)) != Some(visuals) {
            cache.borrow_mut().clear();
            artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
            schedule.neighbor_warm.set(None);
            tracks.artwork.set(visuals.0);
            groups.artwork.set(visuals.0);
            related.artwork.set(visuals.0);
            list.notify.reset();
            grid.notify.reset();
            groups.reset();
            related.reset();
            update_images = true;
        }
        let dpr = ui.window().scale_factor().max(1.0);
        s.set_display_dpr(dpr);
        let physical = ui.window().size();
        let geometry = (
            physical.width,
            physical.height,
            (dpr * 100.0) as u32,
            s.get_full_player(),
            s.get_window_hidden(),
        );
        let old_geometry = image_geometry.replace(geometry);
        update_images |= old_geometry != geometry;
        let cover_size = (s.get_player_cover_size() * dpr).round() as u32;
        update_images |= player_cover_edge.replace(cover_size) != cover_size;
        let browsing = !s.get_full_player() && !s.get_window_hidden();
        if browsing && (old_geometry.3 || old_geometry.4) {
            // Pending covers canceled while covered/hidden need fresh requests
            // once the existing browsing delegates become visible again.
            list.notify.reset();
            grid.visible_rows.borrow_mut().clear();
            grid.notify.reset();
            groups.reset();
            related.reset();
        }
        let width = ui.window().size().width as f32 / dpr;
        let sidebar = if s.get_icon_sidebar() { 56.0 } else { 132.0 };
        let available = (width - sidebar - 40.0).max(1.0);
        if tracks
            .list_edge
            .replace(models::physical_artwork_edge(28, dpr))
            != models::physical_artwork_edge(28, dpr)
        {
            list.notify.reset();
        }
        let layout::GridGeometry { columns, edge } = layout::songs(available, dpr);
        s.set_columns(columns as i32);
        s.set_image_edge(edge as i32);
        let columns_changed = grid.columns.replace(columns) != columns;
        let edge_changed = grid.edge.replace(edge) != edge;
        if columns_changed || edge_changed {
            grid.visible_rows.borrow_mut().clear();
            grid.notify.reset();
        }
        let view = s.get_view();
        groups.circular.set(view == "artists");
        let shape_changed = groups.landscape.replace(view == "genres") != (view == "genres");
        if related
            .edge
            .replace(models::physical_artwork_edge(146, dpr))
            != models::physical_artwork_edge(146, dpr)
        {
            related.reset();
        }
        let layout::GridGeometry {
            columns: group_columns,
            edge: group_edge,
        } = layout::catalog(
            width,
            available,
            dpr,
            &view,
            s.get_folder_grid(),
            !s.get_detail_key().is_empty(),
        );
        s.set_group_columns(group_columns as i32);
        let columns_changed = groups.columns.replace(group_columns) != group_columns;
        let edge_changed = groups.edge.replace(group_edge) != group_edge;
        if columns_changed || edge_changed || shape_changed {
            groups.reset();
        }
        update_images |=
            events::apply(&ui, &models, &poll_tray, &neighbor_revision, &artwork_epoch);
        if s.get_full_player()
            && !s.get_window_hidden()
            && s.get_lyrics_open()
            && s.get_playing()
            && preview.borrow().is_none()
        {
            let position = playback_clock.borrow().position(Instant::now());
            s.set_lyric_position(position);
            s.set_active_line(lyrics::active(&lines.borrow(), position));
        }
        if s.get_full_player()
            && !s.get_window_hidden()
            && s.get_lyrics_open()
            && s.get_lyrics_width() > 0.0
        {
            lyric_renderer.borrow_mut().configure(
                lyric_render::Config {
                    width: (s.get_lyrics_width() * dpr).round().clamp(32.0, 2048.0) as u32,
                    size: (s.get_lyric_font_size() * dpr).round().clamp(16.0, 160.0) as u32,
                    dpr,
                    family: s.get_font_family().into(),
                    font_path: s.get_font_path().into(),
                },
                &lines.borrow(),
            );
        }
        if lyric_renderer.borrow_mut().drain() {
            s.set_lyric_render_revision(s.get_lyric_render_revision().wrapping_add(1));
        }
        let collection_draft = scheduling::tick(
            &ui,
            &models,
            &schedule,
            scheduling::Viewport {
                browsing,
                dpr,
                cover_size,
                physical,
                update_images,
                artwork_epoch: artwork_epoch.get(),
                neighbor_revision: neighbor_revision.get(),
            },
        );
        s.set_lyric_layout_revision(lyric_renderer.borrow().layout_revision() as i32);
        let artwork_changes = cache.borrow_mut().drain();
        if !artwork_changes.is_empty() {
            if browsing && s.get_browse_artwork() {
                let indices = tracks.artwork_indices(&artwork_changes);
                if !s.get_detail_key().is_empty() || s.get_view() == "songs" && !s.get_grid() {
                    for &index in &indices {
                        list.notify.row_changed(index);
                    }
                }
                if s.get_view() == "songs" && s.get_grid() && s.get_detail_key().is_empty() {
                    grid.artwork_changed(&indices);
                } else if (s.get_detail_key().is_empty() || s.get_view() == "folders")
                    && s.get_view() != "settings"
                {
                    groups.artwork_changed(&artwork_changes);
                }
                if !s.get_detail_key().is_empty() {
                    related.artwork_changed(&artwork_changes);
                }
            }
            let current = now.borrow();
            let draft = s.get_draft();
            let detail = detail_art.borrow();
            update_images |= artwork_changes.iter().any(|key| {
                (s.get_collection_dialog()
                    && collection_draft
                        .get("cover")
                        .or_else(|| collection_draft.get("preview"))
                        .and_then(|value| value.as_str())
                        == Some(key.path.as_str()))
                    || key.path == current.artwork
                    || key.path == current.artwork_thumb
                    || key.path == current.artwork_original
                    || (s.get_metadata_open()
                        && (key.path == draft.artwork.as_str()
                            || key.path == draft.cover_to_embed.as_str()))
                    || (!s.get_detail_key().is_empty()
                        && (key.path == detail.0 || key.path == detail.1))
                    || queue_tracks
                        .borrow()
                        .iter()
                        .any(|track| key.path == track.artwork_thumb || key.path == track.artwork)
            });
        }
        if update_images && !s.get_window_hidden() {
            if s.get_collection_dialog() {
                if let Some(path) = collection_draft
                    .get("cover")
                    .or_else(|| collection_draft.get("preview"))
                    .and_then(|value| value.as_str())
                {
                    s.set_collection_cover_missing(path.is_empty() || cache.borrow().failed(path));
                    s.set_collection_cover(cache.borrow_mut().player_cover(
                        path,
                        (128.0 * dpr) as u32,
                        0,
                    ));
                }
            }
            let track = now.borrow();
            s.set_now(if s.get_compact_artwork() {
                models::song(
                    &track,
                    0,
                    models::physical_artwork_edge(52, dpr * s.get_scale()),
                    cache,
                )
            } else {
                models::text_song(&track, 0)
            });
            // Use the same small source as library list covers and queue the
            // background before the large foreground cover on song changes.
            let background_source = if track.artwork_thumb.is_empty() {
                &track.artwork
            } else {
                &track.artwork_thumb
            };
            if s.get_full_player() && s.get_blurred_background() {
                let full = cache.borrow_mut().panel_backdrop(
                    background_source,
                    physical.width,
                    physical.height,
                    0,
                );
                if full.size().width > 0
                    || background_source.is_empty()
                    || cache.borrow().failed(background_source)
                {
                    s.set_full_backdrop(full);
                }
            } else {
                s.set_full_backdrop(slint::Image::default());
            }
            let original = if track.artwork_original.is_empty() {
                &track.artwork
            } else {
                &track.artwork_original
            };
            s.set_original_cover_missing(original.is_empty() || cache.borrow().failed(original));
            let full_cover = cache.borrow_mut().player_cover(
                if s.get_full_artwork() && s.get_full_player() {
                    original
                } else {
                    ""
                },
                (s.get_player_cover_size() * dpr).round().max(8.0) as u32,
                (6.0 * dpr).round() as u32,
            );
            s.set_original_cover(if full_cover.size().width > 0 {
                full_cover
            } else if s.get_full_artwork() && s.get_full_player() {
                cache
                    .borrow_mut()
                    .cached_cover(&[original, &track.artwork, &track.artwork_thumb])
            } else {
                slint::Image::default()
            });
            let backdrop = cache.borrow_mut().get(
                if s.get_blurred_background() && !s.get_full_player() {
                    &track.artwork
                } else {
                    ""
                },
                160,
                true,
            );
            if backdrop.size().width > 0
                || track.artwork.is_empty()
                || cache.borrow().failed(&track.artwork)
                || !s.get_blurred_background()
            {
                s.set_backdrop(backdrop);
            }
            if s.get_metadata_open() {
                let draft = s.get_draft();
                let path = if draft.cover_to_embed.is_empty() {
                    &draft.artwork
                } else {
                    &draft.cover_to_embed
                };
                s.set_editor_cover_missing(path.is_empty() || cache.borrow().failed(path));
                s.set_editor_cover(cache.borrow_mut().player_cover(
                    path,
                    (200.0 * ui.window().scale_factor()) as u32,
                    0,
                ));
            }
            if !s.get_detail_key().is_empty() {
                let art = detail_art.borrow();
                let edge = ((if available < 600.0 { 96.0 } else { 148.0 }) * dpr) as u32;
                s.set_detail_cover_missing(art.0.is_empty() || cache.borrow().failed(&art.0));
                s.set_detail_cover(cache.borrow_mut().rounded(
                    if s.get_browse_artwork() { &art.0 } else { "" },
                    edge,
                    if view == "artists" {
                        edge / 2
                    } else {
                        (6.0 * dpr) as u32
                    },
                ));
                s.set_detail_backdrop(
                    cache.borrow_mut().panel_backdrop(
                        if !s.get_blurred_background() || s.get_detail_cover_missing() {
                            ""
                        } else if art.1.is_empty() {
                            &art.0
                        } else {
                            &art.1
                        },
                        (available * dpr) as u32,
                        ((if available < 600.0 {
                            240.0
                        } else if view == "albums" {
                            280.0
                        } else {
                            256.0
                        }) * dpr)
                            .round() as u32,
                        (6.0 * dpr) as u32,
                    ),
                );
            }
            let rows = queue_tracks.borrow();
            for (index, track) in rows.iter().enumerate() {
                let row = if s.get_browse_artwork() && s.get_queue_open() {
                    models::song(track, index, models::physical_artwork_edge(48, dpr), cache)
                } else {
                    models::text_song(track, index)
                };
                if index < queue.row_count() {
                    if queue.row_data(index).as_ref() != Some(&row) {
                        queue.set_row_data(index, row);
                    }
                } else {
                    queue.push(row);
                }
            }
            while queue.row_count() > rows.len() {
                queue.remove(queue.row_count() - 1);
            }
        }
        let track = now.borrow();
        if s.get_dynamic_accent() && !s.get_window_hidden() {
            cache.borrow_mut().get(&track.artwork_thumb, 48, false);
        }
        let rgb = if s.get_dynamic_accent() {
            let cache = cache.borrow();
            cache
                .accent(&track.artwork_thumb)
                .or_else(|| cache.accent(&track.artwork))
                .unwrap_or([245, 245, 245])
        } else {
            [245, 245, 245]
        };
        s.set_player_accent(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
        let rgb = theme_accent(rgb, s.get_light_theme());
        s.set_accent(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
    });
    poll
}
