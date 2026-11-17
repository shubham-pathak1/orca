//! Publish decoded artwork to matching visible models and refresh player images.
use super::RuntimeModels;
use crate::media::artwork::BackdropStyle;
use crate::presentation::{player_background_tone, theme_accent};
use crate::{models, OrcaWindow};
use slint::{ComponentHandle, Model};

pub(super) fn refresh_artwork(ui: &OrcaWindow, models: &RuntimeModels, mut update_images: bool) {
    let cache = &models.cache;
    let catalog = &models.catalog;
    let models::CatalogModels {
        tracks,
        list,
        grid,
        groups,
        related,
        detail_art,
    } = catalog.as_ref();
    let player = &models.player;
    let crate::playback::state::PlayerModels {
        now,
        queue,
        queue_tracks,
        ..
    } = player.as_ref();
    let dpr = ui.window().scale_factor().max(1.0);
    let physical = ui.window().size();
    let available = ui.get_browse_width().max(1.0);
    let view = ui.shell().get_view();
    let browsing = !ui.player().get_full_player() && !ui.shell().get_window_hidden();
    let collection_draft = if ui.dialogs().get_collection_dialog() {
        serde_json::from_str::<serde_json::Value>(&ui.dialogs().get_collection_request())
            .unwrap_or_default()
    } else {
        serde_json::Value::Null
    };
    let artwork_changes = cache.borrow_mut().drain();
    if !artwork_changes.is_empty() {
        if let Some(playlists) = ui
            .library()
            .get_playlists()
            .as_any()
            .downcast_ref::<models::PlaylistMenu>()
        {
            playlists.artwork_changed(&artwork_changes);
        }
        if browsing && ui.preferences().get_browse_artwork() {
            let indices = tracks.artwork_indices(&artwork_changes);
            if !ui.library().get_detail_key().is_empty()
                || ui.shell().get_view() == "songs" && !ui.preferences().get_grid()
            {
                let visible = ui.library().get_song_first().max(0) as usize
                    ..ui.library().get_song_end().max(0) as usize;
                for &index in indices.iter().filter(|index| visible.contains(index)) {
                    list.notify.row_changed(index);
                }
            }
            if ui.shell().get_view() == "songs"
                && ui.preferences().get_grid()
                && ui.library().get_detail_key().is_empty()
            {
                grid.artwork_changed(&indices);
            } else if (ui.library().get_detail_key().is_empty()
                || ui.shell().get_view() == "folders")
                && ui.shell().get_view() != "settings"
            {
                groups.artwork_changed(&artwork_changes);
            }
            if !ui.library().get_detail_key().is_empty() {
                related.artwork_changed(&artwork_changes);
            }
        }
        let current = now.borrow();
        let draft = ui.dialogs().get_draft();
        let detail = detail_art.borrow();
        update_images |= artwork_changes.iter().any(|key| {
            (ui.dialogs().get_collection_dialog()
                && collection_draft
                    .get("cover")
                    .or_else(|| collection_draft.get("preview"))
                    .and_then(|value| value.as_str())
                    == Some(key.path.as_str()))
                || key.path == current.artwork
                || key.path == current.artwork_thumb
                || key.path == current.artwork_original
                || (ui.dialogs().get_metadata_open()
                    && (key.path == draft.artwork.as_str()
                        || key.path == draft.cover_to_embed.as_str()))
                || (!ui.library().get_detail_key().is_empty()
                    && (key.path == detail.0 || key.path == detail.1))
                || queue_tracks.borrow().iter().any(|track| {
                    key.path == track.artwork_thumb
                        || key.path == track.artwork
                        || key.path == track.artwork_original
                })
        });
    }
    if update_images && !ui.shell().get_window_hidden() {
        if ui.dialogs().get_collection_dialog() {
            if let Some(path) = collection_draft
                .get("cover")
                .or_else(|| collection_draft.get("preview"))
                .and_then(|value| value.as_str())
            {
                ui.dialogs()
                    .set_collection_cover_missing(path.is_empty() || cache.borrow().failed(path));
                ui.dialogs()
                    .set_collection_cover(cache.borrow_mut().player_cover(
                        path,
                        (128.0 * dpr) as u32,
                        0,
                    ));
            }
        }
        let track = now.borrow();
        ui.player()
            .set_now(if ui.preferences().get_compact_artwork() {
                models::song(
                    &track,
                    0,
                    models::physical_artwork_edge(52, dpr * ui.preferences().get_scale()),
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
        if let Some(style) =
            BackdropStyle::from_setting(ui.preferences().get_full_player_background().as_str())
                .filter(|_| ui.player().get_full_player())
        {
            let full = cache.borrow_mut().panel_backdrop(
                background_source,
                physical.width,
                physical.height,
                0,
                style,
            );
            if full.size().width > 0
                || background_source.is_empty()
                || cache.borrow().failed(background_source)
            {
                ui.player().set_full_backdrop(full);
            }
        } else {
            ui.player().set_full_backdrop(slint::Image::default());
        }
        let original = if track.artwork_original.is_empty() {
            &track.artwork
        } else {
            &track.artwork_original
        };
        ui.player()
            .set_original_cover_missing(original.is_empty() || cache.borrow().failed(original));
        let full_cover = cache.borrow_mut().player_cover(
            if ui.preferences().get_full_artwork() && ui.player().get_full_player() {
                original
            } else {
                ""
            },
            (ui.player().get_player_cover_size() * dpr).round().max(8.0) as u32,
            (6.0 * dpr).round() as u32,
        );
        ui.player()
            .set_original_cover(if full_cover.size().width > 0 {
                full_cover
            } else if ui.preferences().get_full_artwork() && ui.player().get_full_player() {
                cache
                    .borrow_mut()
                    .cached_cover(&[original, &track.artwork, &track.artwork_thumb])
            } else {
                slint::Image::default()
            });
        let backdrop = cache.borrow_mut().library_background(
            if ui.preferences().get_blurred_background() && !ui.player().get_full_player() {
                background_source
            } else {
                ""
            },
            ui.preferences().get_light_theme(),
        );
        if backdrop.size().width > 0
            || background_source.is_empty()
            || cache.borrow().failed(background_source)
            || !ui.preferences().get_blurred_background()
        {
            ui.player().set_backdrop(backdrop);
        }
        if ui.dialogs().get_metadata_open() {
            let draft = ui.dialogs().get_draft();
            let path = if draft.cover_to_embed.is_empty() {
                &draft.artwork
            } else {
                &draft.cover_to_embed
            };
            ui.dialogs()
                .set_editor_cover_missing(path.is_empty() || cache.borrow().failed(path));
            ui.dialogs()
                .set_editor_cover(cache.borrow_mut().player_cover(
                    path,
                    (200.0 * ui.window().scale_factor()) as u32,
                    0,
                ));
        }
        if !ui.library().get_detail_key().is_empty() {
            let art = detail_art.borrow();
            let edge = ((if available < 600.0 { 96.0 } else { 148.0 }) * dpr) as u32;
            ui.library()
                .set_detail_cover_missing(art.0.is_empty() || cache.borrow().failed(&art.0));
            let rgb = cache.borrow().accent(&art.0).unwrap_or([245; 3]);
            ui.library().set_detail_accent(slint::Color::from_rgb_u8(
                rgb[0] / 2 + 127,
                rgb[1] / 2 + 127,
                rgb[2] / 2 + 127,
            ));
            ui.library().set_detail_cover(cache.borrow_mut().rounded(
                if ui.preferences().get_browse_artwork() {
                    &art.0
                } else {
                    ""
                },
                edge,
                if view == "artists" {
                    edge / 2
                } else {
                    (6.0 * dpr) as u32
                },
            ));
            ui.library().set_detail_backdrop(
                cache.borrow_mut().panel_backdrop(
                    if !ui.preferences().get_browse_artwork()
                        || ui.library().get_detail_cover_missing()
                    {
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
                    BackdropStyle::Artwork,
                ),
            );
        }
        let rows = queue_tracks.borrow();
        for (index, track) in rows.iter().enumerate() {
            let row = if ui.preferences().get_browse_artwork() && ui.player().get_queue_open() {
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
    if (ui.preferences().get_dynamic_accent()
        || (ui.player().get_full_player()
            && ui.preferences().get_full_player_background().as_str() == "color"))
        && !ui.shell().get_window_hidden()
    {
        cache.borrow_mut().get(&track.artwork_thumb, 48, false);
    }
    let cover_rgb = {
        let cache = cache.borrow();
        cache
            .accent(&track.artwork_thumb)
            .or_else(|| cache.accent(&track.artwork))
    };
    let background_rgb = player_background_tone(cover_rgb.unwrap_or([96, 96, 96]));
    ui.player().set_background_accent(slint::Color::from_rgb_u8(
        background_rgb[0],
        background_rgb[1],
        background_rgb[2],
    ));
    let rgb = if ui.preferences().get_dynamic_accent() {
        cover_rgb
            .map(|rgb| rgb.map(|value| value / 2 + 127))
            .unwrap_or([245; 3])
    } else {
        [245, 245, 245]
    };
    ui.player()
        .set_player_accent(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
    let rgb = theme_accent(rgb, ui.preferences().get_light_theme());
    ui.preferences()
        .set_accent(slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]));
}
