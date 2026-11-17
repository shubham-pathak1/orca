//! UI runtime: drain service events and update visible models/render assets.
//! Owns the timer's retained state; shutdown stops/drops this timer before AppWorker.
mod artwork_scheduler;
mod artwork_updates;
mod catalog_events;
mod event_dispatch;
mod layout;
mod lyric_events;
mod operation_results;
mod playback_events;
#[cfg(test)]
pub(crate) mod tests;
use crate::{lyric_render, lyrics, models, worker::AppWorker, LyricLine, OrcaTray, OrcaWindow};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub(crate) struct RuntimeModels {
    pub(crate) worker: Rc<AppWorker>,
    pub(crate) cache: Rc<RefCell<crate::artwork::ArtworkCache>>,
    pub(crate) player: Rc<crate::playback::state::PlayerModels>,
    pub(crate) catalog: Rc<models::CatalogModels>,
}
pub(crate) struct UiRuntime {
    update_timer: Timer,
    artwork_timer: Rc<Timer>,
    ui: slint::Weak<OrcaWindow>,
    cache: Rc<RefCell<crate::artwork::ArtworkCache>>,
}
impl UiRuntime {
    pub fn stop(&self) {
        self.update_timer.stop();
        self.artwork_timer.stop();
        self.cache.borrow_mut().clear_ready_handler();
        if let Some(ui) = self.ui.upgrade() {
            ui.on_artwork_ready(|| {});
        }
    }
}
impl Drop for UiRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}
pub(crate) fn start(
    ui: &OrcaWindow,
    poll_tray: Option<slint::Weak<OrcaTray>>,
    models: RuntimeModels,
) -> UiRuntime {
    let artwork_timer = Rc::new(Timer::default());
    let ready_timer = artwork_timer.clone();
    let ready_models = models.clone();
    let weak = ui.as_weak();
    ui.on_artwork_ready(move || {
        if ready_timer.running() {
            return;
        }
        let weak = weak.clone();
        let models = ready_models.clone();
        ready_timer.start(TimerMode::SingleShot, Duration::from_millis(8), move || {
            if let Some(ui) = weak.upgrade() {
                artwork_updates::refresh_artwork(&ui, &models, false);
            }
        });
    });
    let weak = ui.as_weak();
    models.cache.borrow_mut().on_ready(move || {
        let _ = weak.upgrade_in_event_loop(|ui| ui.invoke_artwork_ready());
    });
    let runtime_cache = models.cache.clone();
    let schedule = artwork_scheduler::ArtworkScheduler::default();
    let neighbor_revision = Cell::new(0u64);
    let artwork_epoch = Cell::new(0u64);
    let visual_settings = Cell::new(None);
    let image_geometry = Cell::new((0u32, 0u32, 0u32, false, false));
    let player_cover_edge = Cell::new(0u32);
    let background_theme = Cell::new(false);
    let last_hidden_update = Cell::new(None::<Instant>);

    let mut romanization_source: Option<(
        slint::ModelRc<LyricLine>,
        i32,
        slint::ModelRc<crate::RomanizationLanguage>,
    )> = None;
    let update_timer = Timer::default();
    let weak = ui.as_weak();
    update_timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if ui.shell().get_window_hidden() {
            let now = Instant::now();
            if last_hidden_update
                .get()
                .is_some_and(|last| now.duration_since(last) < Duration::from_millis(200))
            {
                return;
            }
            last_hidden_update.set(Some(now));
        } else {
            last_hidden_update.set(None);
        }

        let RuntimeModels {
            cache,
            player,
            catalog,
            ..
        } = &models;
        let crate::playback::state::PlayerModels {
            lyric_renderer,
            playback_clock,
            lines,
            preview,
            ..
        } = player.as_ref();
        let models::CatalogModels {
            tracks,
            list,
            grid,
            groups,
            related,
            ..
        } = catalog.as_ref();

        let mut update_images = background_theme.replace(ui.preferences().get_light_theme())
            != ui.preferences().get_light_theme();
        let visuals = (
            ui.preferences().get_browse_artwork(),
            ui.preferences().get_compact_artwork(),
            ui.preferences().get_full_artwork(),
            ui.preferences().get_blurred_background(),
            ui.preferences().get_dynamic_accent(),
            match ui.preferences().get_full_player_background().as_str() {
                "artwork" => 1,
                "gradient" => 2,
                "color" => 3,
                _ => 0,
            },
        );
        if visual_settings.replace(Some(visuals)) != Some(visuals) {
            cache.borrow_mut().clear();
            artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
            schedule.neighbor_warm.set(None);
            tracks.artwork.set(visuals.0);
            groups.artwork.set(visuals.0);
            related.artwork.set(visuals.0);
            list.notify.reset();
            grid.reset();
            groups.reset();
            related.reset();
            update_images = true;
        }
        let dpr = ui.window().scale_factor().max(1.0);
        ui.shell().set_display_dpr(dpr);
        let physical = ui.window().size();
        let geometry = (
            physical.width,
            physical.height,
            (dpr * 100.0) as u32,
            ui.player().get_full_player(),
            ui.shell().get_window_hidden(),
        );
        let old_geometry = image_geometry.replace(geometry);
        update_images |= old_geometry != geometry;
        let cover_size = (ui.player().get_player_cover_size() * dpr).round() as u32;
        update_images |= player_cover_edge.replace(cover_size) != cover_size;
        let browsing = !ui.player().get_full_player() && !ui.shell().get_window_hidden();
        if browsing && (old_geometry.3 || old_geometry.4) {
            // Pending covers canceled while covered/hidden need fresh requests
            // once the existing browsing delegates become visible again.
            list.notify.reset();
            grid.reset();
            groups.reset();
            related.reset();
        }
        let width = ui.window().size().width as f32 / dpr;
        let available = ui.get_browse_width().max(1.0);
        if tracks
            .list_edge
            .replace(models::physical_artwork_edge(36, dpr))
            != models::physical_artwork_edge(36, dpr)
        {
            list.notify.reset();
        }
        let layout::GridGeometry { columns, edge } = layout::songs(available, dpr);
        ui.library().set_columns(columns as i32);
        ui.library().set_image_edge(edge as i32);
        let columns_changed = grid.columns.replace(columns) != columns;
        let edge_changed = grid.edge.replace(edge) != edge;
        if columns_changed || edge_changed {
            grid.reset();
        }
        let view = ui.shell().get_view();
        groups.circular.set(view == "artists");
        let shape_changed = groups.landscape.replace(view == "genres") != (view == "genres");
        let layout::GridGeometry {
            columns: related_columns,
            edge: related_edge,
        } = layout::related(
            if !ui.lyrics().get_lyrics_open() && available >= 800.0 {
                336.0
            } else {
                available
            },
            dpr,
            ui.lyrics().get_lyrics_open(),
        );
        ui.library().set_related_columns(related_columns as i32);
        let related_columns_changed = related.columns.replace(related_columns) != related_columns;
        if related.edge.replace(related_edge) != related_edge || related_columns_changed {
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
            ui.preferences().get_folder_grid(),
            !ui.library().get_detail_key().is_empty(),
        );
        ui.library().set_group_columns(group_columns as i32);
        let columns_changed = groups.columns.replace(group_columns) != group_columns;
        let edge_changed = groups.edge.replace(group_edge) != group_edge;
        if columns_changed || edge_changed || shape_changed {
            groups.reset();
        }
        update_images |= event_dispatch::drain_events(
            &ui,
            &models,
            &poll_tray,
            &neighbor_revision,
            &artwork_epoch,
        );
        // Model identity changes only when original lyrics change, not on clock ticks.
        let source = (
            ui.lyrics().get_lyrics(),
            ui.preferences().get_romanization_mode(),
            ui.preferences().get_romanization_languages(),
        );
        if romanization_source.as_ref() != Some(&source) {
            lyric_renderer.borrow_mut().set_romanization(Vec::new());
            let policy = crate::controllers::preferences::romanization_policy(&ui);
            let texts = if policy.enabled() {
                lines
                    .borrow()
                    .iter()
                    .map(|line| line.text.to_string())
                    .collect()
            } else {
                Vec::new()
            };
            models.worker.romanize(texts, policy);
            romanization_source = Some(source);
        }
        let lyrics_visible = ui.lyrics().get_lyrics_open()
            && !ui.shell().get_window_hidden()
            && (ui.player().get_full_player()
                || (!ui.player().get_queue_open() && !ui.player().get_now().path.is_empty()));
        if lyrics_visible && ui.player().get_playing() && preview.borrow().is_none() {
            let position = playback_clock.borrow().position(Instant::now());
            ui.lyrics().set_lyric_position(position);
            ui.lyrics()
                .set_active_line(lyrics::active(&lines.borrow(), position));
        }
        if lyrics_visible && ui.lyrics().get_lyrics_width() > 0.0 {
            lyric_renderer.borrow_mut().configure(
                lyric_render::Config {
                    blur_strength: if ui.preferences().get_blur_unfocused_lyrics() {
                        ui.preferences()
                            .get_lyrics_blur_strength()
                            .round()
                            .clamp(0.0, 100.0) as u8
                    } else {
                        0
                    },
                    left_aligned: ui.preferences().get_lyrics_left_aligned(),
                    width: (ui.lyrics().get_lyrics_width() * dpr)
                        .round()
                        .clamp(32.0, 2048.0) as u32,
                    size: (ui.lyrics().get_lyric_font_size() * dpr)
                        .round()
                        .clamp(16.0, 160.0) as u32,
                    dpr,
                    family: ui.preferences().get_font_family().into(),
                    font_path: ui.preferences().get_font_path().into(),
                },
                &lines.borrow(),
            );
        }
        if lyric_renderer.borrow_mut().drain() {
            ui.lyrics()
                .set_lyric_render_revision(ui.lyrics().get_lyric_render_revision().wrapping_add(1));
        }
        artwork_scheduler::schedule_artwork(
            &ui,
            &models,
            &schedule,
            artwork_scheduler::Viewport {
                browsing,
                dpr,
                cover_size,
                physical,
                update_images,
                artwork_epoch: artwork_epoch.get(),
                neighbor_revision: neighbor_revision.get(),
            },
        );
        ui.lyrics()
            .set_lyric_layout_revision(lyric_renderer.borrow().layout_revision() as i32);
        artwork_updates::refresh_artwork(&ui, &models, update_images);
    });
    UiRuntime {
        update_timer,
        artwork_timer,
        ui: ui.as_weak(),
        cache: runtime_cache,
    }
}
