//! Schedule visible and nearby artwork; defer browse decoding until scrolling settles.
use super::RuntimeModels;
use crate::media::artwork::BackdropStyle;
use crate::OrcaWindow;
use orca_services::operation_types::{ArtworkDemand, CollectionIdentity, CollectionKind};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};
type RangeSignature = (u64, u64, usize, usize, u32);
type SongSignature = (u64, u64, usize, usize, u32, bool);
type NeighborSignature = (u64, u32, u32, u32, bool, Option<BackdropStyle>);
#[derive(Default)]
pub(super) struct ArtworkScheduler {
    scroll: RefCell<ScrollDebounce>,
    online_tick: Cell<Option<Instant>>,
    online_candidate: RefCell<Option<(ArtworkDemand, Instant)>>,
    online_sent: RefCell<Option<ArtworkDemand>>,
    song_warm: Cell<Option<SongSignature>>,
    catalog_warm: Cell<Option<RangeSignature>>,
    related_warm: Cell<Option<RangeSignature>>,
    pub neighbor_warm: Cell<Option<NeighborSignature>>,
}
// Cached artwork stays immediate; new browse decodes wait for scrolling to settle.
#[derive(Default)]
struct ScrollDebounce {
    context: String,
    sample: Option<(f32, Instant)>,
    until: Option<Instant>,
}
impl ScrollDebounce {
    fn update(&mut self, context: &str, position: f32, now: Instant, browsing: bool) -> bool {
        if !browsing || self.context != context || !position.is_finite() {
            self.context = context.to_owned();
            self.sample = Some((position, now));
            self.until = None;
            return false;
        }
        if let Some((previous, _)) = self.sample.replace((position, now)) {
            if position != previous {
                self.until = Some(now + Duration::from_millis(90));
            }
        }
        self.until.is_some_and(|until| now < until)
    }
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
pub(super) fn schedule_artwork(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    schedule: &ArtworkScheduler,
    viewport: Viewport,
) {
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
        player,
        catalog,
        ..
    } = models;
    let crate::playback::state::PlayerModels {
        now,
        queue_tracks,
        neighbors,
        ..
    } = player.as_ref();
    let crate::models::CatalogModels {
        grid,
        tracks,
        groups,
        related,
        detail_art,
        ..
    } = catalog.as_ref();

    let mut wanted_art = std::collections::HashSet::new();
    if !ui.shell().get_window_hidden() {
        let current = now.borrow();
        wanted_art.extend([
            current.artwork.clone(),
            current.artwork_thumb.clone(),
            current.artwork_original.clone(),
        ]);
        if !ui.player().get_full_player() && ui.preferences().get_browse_artwork() {
            if ui.shell().get_view() == "songs"
                && ui.preferences().get_grid()
                && ui.library().get_detail_key().is_empty()
            {
                let rows = grid.visible_rows.borrow();
                let columns = grid.columns.get().max(1);
                tracks.artwork_paths(
                    rows.iter()
                        .filter(|(_, row)| row.strong_count() > 0)
                        .flat_map(|(&row, _)| row * columns..(row + 1) * columns),
                    &mut wanted_art,
                );
            } else if ui.shell().get_view() == "songs" || !ui.library().get_detail_key().is_empty()
            {
                tracks.artwork_paths(
                    ui.library().get_song_first().max(0) as usize
                        ..ui.library().get_song_end().max(0) as usize,
                    &mut wanted_art,
                );
            }
            groups.visible_paths(&mut wanted_art);
            related.visible_paths(&mut wanted_art);
            if !ui.library().get_detail_key().is_empty() {
                let art = detail_art.borrow();
                wanted_art.extend([art.0.clone(), art.1.clone()]);
            }
        }
        if ui.player().get_queue_open() && ui.preferences().get_browse_artwork() {
            for track in queue_tracks.borrow().iter() {
                wanted_art.extend([
                    track.artwork.clone(),
                    track.artwork_thumb.clone(),
                    track.artwork_original.clone(),
                ]);
            }
        }
        if ui.dialogs().get_metadata_open() {
            let draft = ui.dialogs().get_draft();
            wanted_art.extend([draft.artwork.to_string(), draft.cover_to_embed.to_string()]);
        }
    }
    let collection_draft = if ui.dialogs().get_collection_dialog() {
        serde_json::from_str::<serde_json::Value>(&ui.dialogs().get_collection_request())
            .unwrap_or_default()
    } else {
        serde_json::Value::Null
    };
    if ui.dialogs().get_collection_dialog() {
        if let Some(path) = collection_draft
            .get("cover")
            .or_else(|| collection_draft.get("preview"))
            .and_then(|value| value.as_str())
        {
            wanted_art.insert(path.to_string());
        }
    }
    if browsing && ui.preferences().get_browse_artwork() {
        let is_grid = ui.shell().get_view() == "songs"
            && ui.preferences().get_grid()
            && ui.library().get_detail_key().is_empty();
        let range = if is_grid {
            grid.nearby_range()
        } else if ui.shell().get_view() == "songs" || !ui.library().get_detail_key().is_empty() {
            Some((
                (ui.library().get_song_first().max(0) as usize).saturating_sub(20),
                (ui.library().get_song_end().max(0) as usize + 20).min(tracks.count.get()),
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
    if !ui.shell().get_window_hidden() && ui.player().get_full_player() {
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
            ui.preferences().get_full_artwork(),
            BackdropStyle::from_setting(ui.preferences().get_full_player_background().as_str()),
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
                if ui.preferences().get_full_artwork() {
                    cache.prefetch_cover(
                        original,
                        cover_size.max(8),
                        (6.0 * dpr).round() as u32,
                        true,
                    );
                }
                if let Some(style) = BackdropStyle::from_setting(
                    ui.preferences().get_full_player_background().as_str(),
                ) {
                    cache.prefetch_backdrop(small, physical.width, physical.height, 0, style);
                }
            }
        }
    } else {
        schedule.neighbor_warm.set(None);
    }
    cache.borrow_mut().retain_requests(&wanted_art);
    let deferred = schedule.scroll.borrow_mut().update(
        &format!(
            "{}|{}|{}",
            ui.shell().get_view(),
            ui.library().get_detail_key(),
            ui.preferences().get_grid()
        ),
        ui.library().get_artwork_scroll_position(),
        Instant::now(),
        browsing && !ui.dialogs().get_metadata_open() && !ui.dialogs().get_collection_dialog(),
    );
    let playing = now.borrow();
    cache.borrow_mut().defer_scrolling(
        deferred,
        &[
            &playing.artwork,
            &playing.artwork_thumb,
            &playing.artwork_original,
        ],
    );
    drop(playing);
    online_artwork(ui, models, schedule);
}

fn online_artwork(ui: &OrcaWindow, models: &RuntimeModels, schedule: &ArtworkScheduler) {
    let playing = (ui.preferences().get_auto_artwork() && models.player.now.borrow().id > 0)
        .then_some(models.player.now.borrow().id);
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
    if ui.preferences().get_auto_artwork() {
        demand.playing = playing;
        if !ui.shell().get_window_hidden()
            && !ui.player().get_full_player()
            && ui.shell().get_view() != "settings"
            && !ui.library().get_catalog_loading()
        {
            if ui.shell().get_view() == "songs"
                && ui.preferences().get_grid()
                && ui.library().get_detail_key().is_empty()
            {
                let columns = models.catalog.grid.columns.get().max(1);
                demand.tracks = models.catalog.tracks.visible_ids(
                    models
                        .catalog
                        .grid
                        .visible_rows
                        .borrow()
                        .iter()
                        .filter(|(_, row)| row.strong_count() > 0)
                        .flat_map(|(&row, _)| row * columns..(row + 1) * columns),
                );
            } else if ui.shell().get_view() == "songs" || !ui.library().get_detail_key().is_empty()
            {
                demand.tracks = models.catalog.tracks.visible_ids(
                    ui.library().get_song_first().max(0) as usize
                        ..ui.library().get_song_end().max(0) as usize,
                );
            }
            if let Ok(kind @ (CollectionKind::Artists | CollectionKind::Albums)) =
                CollectionKind::try_from(ui.shell().get_view().as_str())
            {
                if ui.library().get_detail_key().is_empty() {
                    demand.collections = models.catalog.groups.visible_collections(kind);
                } else {
                    demand.collections.push(CollectionIdentity {
                        kind,
                        key: ui.library().get_detail_key().into(),
                        secondary: ui.library().get_detail_secondary().into(),
                    });
                }
            }
            demand.collections.extend(
                models
                    .catalog
                    .related
                    .visible_collections(CollectionKind::Albums),
            );
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
        || !ui.preferences().get_auto_artwork()
        || candidate.as_ref().unwrap().1.elapsed() >= Duration::from_millis(400)
    {
        models.worker.artwork_demand.submit(demand.clone());
        *schedule.online_sent.borrow_mut() = Some(demand);
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::*;
    #[test]
    fn browse_decoding_waits_until_both_slow_and_fast_scrolling_settle() {
        let mut scroll = ScrollDebounce::default();
        let start = Instant::now();
        assert!(!scroll.update("songs", 0.0, start, true));
        assert!(scroll.update("songs", -48.0, start + Duration::from_millis(33), true));
        assert!(scroll.update("songs", -248.0, start + Duration::from_millis(66), true));
        assert!(scroll.update("songs", -248.0, start + Duration::from_millis(100), true));
        assert!(scroll.update("songs", -248.0, start + Duration::from_millis(155), true));
        assert!(!scroll.update("songs", -248.0, start + Duration::from_millis(156), true));
        assert!(scroll.update("songs", -448.0, start + Duration::from_millis(200), true));
        assert!(!scroll.update("albums", 0.0, start + Duration::from_millis(201), true));
        assert!(!scroll.update("albums", -500.0, start + Duration::from_millis(202), false));
    }
}
