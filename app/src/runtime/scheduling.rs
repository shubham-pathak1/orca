//! Visible artwork ownership and speculative prefetch policy, separate from rendering.
use super::RuntimeModels;
use crate::{AppState, OrcaWindow};
use orca_services::operation_types::{ArtworkDemand, CollectionIdentity, CollectionKind};
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};
type RangeSignature = (u64, u64, usize, usize, u32);
type SongSignature = (u64, u64, usize, usize, u32, bool);
type NeighborSignature = (u64, u32, u32, u32, bool, bool);
#[derive(Default)]
pub(super) struct ArtworkSchedule {
    online_tick: Cell<Option<Instant>>,
    online_candidate: RefCell<Option<(ArtworkDemand, Instant)>>,
    online_sent: RefCell<Option<ArtworkDemand>>,
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
            s.get_full_player_background().as_str() == "artwork",
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
                if s.get_full_player_background().as_str() == "artwork" {
                    cache.prefetch_backdrop(small, physical.width, physical.height, 0);
                }
            }
        }
    } else {
        schedule.neighbor_warm.set(None);
    }
    cache.borrow_mut().retain_requests(&wanted_art);
    online_artwork(ui, models, schedule);
    collection_draft
}

fn online_artwork(ui: &OrcaWindow, models: &RuntimeModels, schedule: &ArtworkSchedule) {
    let state = ui.global::<AppState>();
    let playing =
        (state.get_auto_artwork() && models.now.borrow().id > 0).then_some(models.now.borrow().id);
    let track_changed = schedule
        .online_sent
        .borrow()
        .as_ref()
        .is_none_or(|sent| sent.playing != playing);
    if !track_changed
        && schedule
            .online_tick
            .get()
            .is_some_and(|at| at.elapsed() < Duration::from_millis(200))
    {
        return;
    }
    schedule.online_tick.set(Some(Instant::now()));
    let mut demand = ArtworkDemand::default();
    if state.get_auto_artwork() {
        demand.playing = playing;
        if !state.get_window_hidden()
            && !state.get_full_player()
            && state.get_view() != "settings"
            && !state.get_catalog_loading()
        {
            if state.get_view() == "songs" && state.get_grid() && state.get_detail_key().is_empty()
            {
                let columns = models.grid.columns.get().max(1);
                demand.tracks = models.tracks.visible_ids(
                    models
                        .grid
                        .visible_rows
                        .borrow()
                        .iter()
                        .filter(|(_, row)| row.strong_count() > 0)
                        .flat_map(|(&row, _)| row * columns..(row + 1) * columns),
                );
            } else if state.get_view() == "songs" || !state.get_detail_key().is_empty() {
                demand.tracks = models.tracks.visible_ids(
                    state.get_song_first().max(0) as usize..state.get_song_end().max(0) as usize,
                );
            }
            if let Ok(kind @ (CollectionKind::Artists | CollectionKind::Albums)) =
                CollectionKind::try_from(state.get_view().as_str())
            {
                if state.get_detail_key().is_empty() {
                    demand.collections = models.groups.visible_collections(kind);
                } else {
                    demand.collections.push(CollectionIdentity {
                        kind,
                        key: state.get_detail_key().into(),
                        secondary: state.get_detail_secondary().into(),
                    });
                }
            }
            demand
                .collections
                .extend(models.related.visible_collections(CollectionKind::Albums));
        }
    }
    demand.tracks.sort_unstable();
    demand.tracks.dedup();
    demand.collections.sort_by(|a, b| {
        (a.kind.as_str(), &a.key, &a.secondary).cmp(&(b.kind.as_str(), &b.key, &b.secondary))
    });
    demand.collections.dedup();
    demand.collections.truncate(64);
    if schedule.online_sent.borrow().as_ref() == Some(&demand) {
        *schedule.online_candidate.borrow_mut() = None;
        return;
    }
    let mut candidate = schedule.online_candidate.borrow_mut();
    if candidate
        .as_ref()
        .is_none_or(|(current, _)| current != &demand)
    {
        *candidate = Some((demand.clone(), Instant::now()));
    }
    if track_changed
        || !state.get_auto_artwork()
        || candidate.as_ref().unwrap().1.elapsed() >= Duration::from_millis(400)
    {
        models.service.artwork_demand.submit(demand.clone());
        *schedule.online_sent.borrow_mut() = Some(demand);
    }
}
