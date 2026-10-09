//! Visible artwork ownership and speculative prefetch policy, separate from rendering.
use super::RuntimeModels;
use crate::{AppState, OrcaWindow};
use slint::ComponentHandle;
use std::cell::Cell;
type RangeSignature = (u64, u64, usize, usize, u32);
type SongSignature = (u64, u64, usize, usize, u32, bool);
type NeighborSignature = (u64, u32, u32, u32, bool, bool);
#[derive(Default)]
pub(super) struct ArtworkSchedule {
    song_warm: Cell<Option<SongSignature>>,
    catalog_warm: Cell<Option<RangeSignature>>,
    related_warm: Cell<Option<RangeSignature>>,
    pub neighbor_warm: Cell<Option<NeighborSignature>>,
}
pub(super) struct Viewport {
    pub browsing: bool,
    pub dpr: f32,
    pub cover_size: u32,
    pub physical: slint::PhysicalSize,
    pub update_images: bool,
    pub artwork_epoch: u64,
    pub neighbor_revision: u64,
}
pub(super) fn tick(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    schedule: &ArtworkSchedule,
    viewport: Viewport,
) -> serde_json::Value {
    let Viewport {
        browsing,
        dpr,
        cover_size,
        physical,
        update_images,
        artwork_epoch,
        neighbor_revision,
    } = viewport;
    let RuntimeModels {
        cache,
        now,
        grid,
        tracks,
        groups,
        related,
        detail_art,
        queue_tracks,
        neighbors,
        ..
    } = models;
    let s = ui.global::<AppState>();
    let mut wanted_art = std::collections::HashSet::new();
    if !s.get_window_hidden() {
        let current = now.borrow();
        wanted_art.extend([
            current.artwork.clone(),
            current.artwork_thumb.clone(),
            current.artwork_original.clone(),
        ]);
        if !s.get_full_player() && s.get_browse_artwork() {
            if s.get_view() == "songs" && s.get_grid() && s.get_detail_key().is_empty() {
                let rows = grid.visible_rows.borrow();
                let columns = grid.columns.get().max(1);
                tracks.artwork_paths(
                    rows.iter()
                        .filter(|(_, row)| row.strong_count() > 0)
                        .flat_map(|(&row, _)| row * columns..(row + 1) * columns),
                    &mut wanted_art,
                );
            } else if s.get_view() == "songs" || !s.get_detail_key().is_empty() {
                tracks.artwork_paths(
                    s.get_song_first().max(0) as usize..s.get_song_end().max(0) as usize,
                    &mut wanted_art,
                );
            }
            groups.visible_paths(&mut wanted_art);
            related.visible_paths(&mut wanted_art);
            if !s.get_detail_key().is_empty() {
                let art = detail_art.borrow();
                wanted_art.extend([art.0.clone(), art.1.clone()]);
            }
        }
        if s.get_queue_open() && s.get_browse_artwork() {
            for track in queue_tracks.borrow().iter() {
                wanted_art.extend([track.artwork.clone(), track.artwork_thumb.clone()]);
            }
        }
        if s.get_metadata_open() {
            let draft = s.get_draft();
            wanted_art.extend([draft.artwork.to_string(), draft.cover_to_embed.to_string()]);
        }
    }
    let collection_draft = if s.get_collection_dialog() {
        serde_json::from_str::<serde_json::Value>(&s.get_collection_request()).unwrap_or_default()
    } else {
        serde_json::Value::Null
    };
    if s.get_collection_dialog() {
        if let Some(path) = collection_draft
            .get("cover")
            .or_else(|| collection_draft.get("preview"))
            .and_then(|value| value.as_str())
        {
            wanted_art.insert(path.to_string());
        }
    }
    if browsing && s.get_browse_artwork() {
        let is_grid = s.get_view() == "songs" && s.get_grid() && s.get_detail_key().is_empty();
        let range = if is_grid {
            grid.nearby_range()
        } else if s.get_view() == "songs" || !s.get_detail_key().is_empty() {
            Some((
                (s.get_song_first().max(0) as usize).saturating_sub(20),
                (s.get_song_end().max(0) as usize + 20).min(tracks.count.get()),
            ))
        } else {
            None
        };
        if let Some((start, end)) = range {
            tracks.artwork_paths(start..end, &mut wanted_art);
            let edge = if is_grid {
                grid.edge.get()
            } else {
                tracks.list_edge.get()
            };
            let signature = (
                artwork_epoch,
                tracks.generation.get(),
                start,
                end,
                edge,
                is_grid,
            );
            if schedule.song_warm.replace(Some(signature)) != Some(signature) {
                tracks.prefetch_range(start, end, edge, is_grid);
            }
        }
        for (model, warm) in [
            (&groups, &schedule.catalog_warm),
            (&related, &schedule.related_warm),
        ] {
            if let Some((start, end)) = model.nearby_range() {
                model.nearby_paths(start, end, &mut wanted_art);
                let signature = (
                    artwork_epoch,
                    tracks.generation.get(),
                    start,
                    end,
                    model.edge.get(),
                );
                if warm.replace(Some(signature)) != Some(signature) {
                    model.prefetch_range(start, end);
                }
            }
        }
    }
    if !s.get_window_hidden() && s.get_full_player() {
        for track in neighbors.borrow().iter() {
            wanted_art.extend([
                track.artwork.clone(),
                track.artwork_thumb.clone(),
                track.artwork_original.clone(),
            ]);
        }
        let signature = (
            neighbor_revision,
            cover_size,
            physical.width,
            physical.height,
            s.get_full_artwork(),
            s.get_blurred_background(),
        );
        if !update_images && schedule.neighbor_warm.replace(Some(signature)) != Some(signature) {
            let mut cache = cache.borrow_mut();
            for track in neighbors.borrow().iter() {
                let original = if track.artwork_original.is_empty() {
                    &track.artwork
                } else {
                    &track.artwork_original
                };
                let small = if track.artwork_thumb.is_empty() {
                    &track.artwork
                } else {
                    &track.artwork_thumb
                };
                if s.get_full_artwork() {
                    cache.prefetch_cover(
                        original,
                        cover_size.max(8),
                        (6.0 * dpr).round() as u32,
                        true,
                    );
                }
                if s.get_blurred_background() {
                    cache.prefetch_backdrop(small, physical.width, physical.height, 0);
                }
            }
        }
    } else {
        schedule.neighbor_warm.set(None);
    }
    cache.borrow_mut().retain_requests(&wanted_art);
    collection_draft
}
