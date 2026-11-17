//! Apply catalog pages and collection rows without resetting unrelated visible models.
use super::RuntimeModels;
use crate::OrcaWindow;
use orca_services::types::{Group, Track};
use slint::ComponentHandle;
use std::{cell::Cell, time::Duration};
pub(super) fn apply_page(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    artwork_epoch: &Cell<u64>,
    generation: u64,
    offset: u32,
    count: u32,
    rows: Vec<Track>,
) {
    let catalog = &models.catalog;
    let crate::models::CatalogModels {
        tracks, list, grid, ..
    } = catalog.as_ref();
    let columns = grid.columns.get();
    ui.library().set_catalog_loading(false);
    let reset = tracks.count.get() != count as usize;
    let length = rows.len();
    tracks.apply(generation, offset as usize, count as usize, rows);
    artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
    if reset {
        list.notify.reset();
        grid.reset();
    } else {
        // Notify only the active viewport. Offscreen delegates read
        // the latest cached page when Slint creates them.
        if !ui.library().get_detail_key().is_empty()
            || ui.shell().get_view() == "songs" && !ui.preferences().get_grid()
        {
            let start = (offset as usize).max(ui.library().get_song_first().max(0) as usize);
            let end = (offset as usize + length).min(ui.library().get_song_end().max(0) as usize);
            for index in start..end {
                list.notify.row_changed(index);
            }
        } else if ui.shell().get_view() == "songs" && ui.preferences().get_grid() {
            let rows: Vec<_> = {
                let visible = grid.visible_rows.borrow();
                (offset as usize / columns..(offset as usize + length).div_ceil(columns))
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
        }
    }
}
pub(super) fn apply_groups(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    artwork_epoch: &Cell<u64>,
    rows: Vec<Group>,
) {
    let groups = &models.catalog.groups;
    if ui.library().get_detail_key().is_empty() {
        ui.library().set_catalog_loading(false);
    }
    let rows: Vec<_> =
        if ui.shell().get_view() == "playlists" && !ui.preferences().get_show_liked_songs() {
            rows.into_iter()
                .filter(|group| group.key != "liked")
                .collect()
        } else {
            rows
        };
    ui.library().set_catalog_summary(
        format!(
            "{} {}",
            rows.len(),
            if rows.len() == 1 {
                ui.shell().get_view().trim_end_matches('s').to_string()
            } else {
                ui.shell().get_view().to_string()
            }
        )
        .into(),
    );
    if *groups.groups.borrow() != rows {
        *groups.groups.borrow_mut() = rows;
        groups.reset();
        artwork_epoch.set(artwork_epoch.get().wrapping_add(1));
    }
    if ui.library().get_catalog_scroll_restore_pending() && ui.library().get_detail_key().is_empty()
    {
        ui.library().set_catalog_scroll_restore_pending(false);
        let weak = ui.as_weak();
        let view = ui.shell().get_view();
        slint::Timer::single_shot(Duration::from_millis(16), move || {
            if let Some(ui) = weak.upgrade() {
                if ui.library().get_detail_key().is_empty() && ui.shell().get_view() == view {
                    ui.library().set_catalog_scroll_restore_revision(
                        ui.library()
                            .get_catalog_scroll_restore_revision()
                            .wrapping_add(1),
                    );
                }
            }
        });
    }
}
