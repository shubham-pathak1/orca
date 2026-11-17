//! Maintain the bounded song-row window while preserving rows shared between scroll positions.
use crate::{OrcaWindow, Song, SongGridRow};
use slint::{Model, ModelRc, ModelTracker, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct GridWindow {
    rows: VecModel<SongGridRow>,
    pending: RefCell<Option<Vec<SongGridRow>>>,
    revision: Cell<i32>,
}

impl GridWindow {
    fn prepare(&self, source: ModelRc<ModelRc<Song>>, first: usize, count: usize) -> i32 {
        source.model_tracker().track_row_count_changes();
        let first = first.min(source.row_count());
        let end = first.saturating_add(count).min(source.row_count());
        let desired: Vec<_> = (first..end)
            .map(|index| {
                source.model_tracker().track_row_data_changes(index);
                SongGridRow {
                    index: index as i32,
                    cells: source.row_data(index).unwrap_or_default(),
                }
            })
            .collect();
        let unchanged = self.pending.borrow().as_ref().map_or_else(
            || {
                self.rows.row_count() == desired.len()
                    && desired
                        .iter()
                        .enumerate()
                        .all(|(index, row)| self.rows.row_data(index).as_ref() == Some(row))
            },
            |pending| pending == &desired,
        );
        if !unchanged {
            *self.pending.borrow_mut() = Some(desired);
            self.revision.set(self.revision.get().wrapping_add(1));
        }
        self.revision.get()
    }
    fn apply(&self) {
        let desired = self.pending.borrow_mut().take();
        let Some(desired) = desired else { return };
        let first = desired.first().map_or(0, |row| row.index as usize);
        let end = first + desired.len();
        let existing = self.rows.row_data(0).map(|row| row.index as usize);
        if existing.is_some_and(|start| end <= start || first >= start + self.rows.row_count()) {
            self.rows.set_vec(desired);
            return;
        }
        while self
            .rows
            .row_data(0)
            .is_some_and(|row| (row.index as usize) < first)
        {
            self.rows.remove(0);
        }
        while self.rows.row_count() > 0
            && self
                .rows
                .row_data(self.rows.row_count() - 1)
                .is_some_and(|row| (row.index as usize) >= end)
        {
            self.rows.remove(self.rows.row_count() - 1);
        }
        for (index, row) in desired.into_iter().enumerate() {
            match self.rows.row_data(index) {
                Some(old) if old.index == row.index => {
                    if old != row {
                        self.rows.set_row_data(index, row);
                    }
                }
                _ => self.rows.insert(index, row),
            }
        }
    }
}

impl Model for GridWindow {
    type Data = SongGridRow;
    fn row_count(&self) -> usize {
        self.rows.row_count()
    }
    fn row_data(&self, index: usize) -> Option<SongGridRow> {
        self.rows.row_data(index)
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        self.rows.model_tracker()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) fn install_grid_window(ui: &OrcaWindow) {
    ui.library()
        .on_new_grid_window(|| Rc::new(GridWindow::default()).into());
    ui.library()
        .on_prepare_grid_window(|window, source, first, count| {
            window
                .as_any()
                .downcast_ref::<GridWindow>()
                .map_or(0, |window| {
                    window.prepare(source, first.max(0) as usize, count.max(0) as usize)
                })
        });
    ui.library().on_apply_grid_window(|window, _| {
        if let Some(window) = window.as_any().downcast_ref::<GridWindow>() {
            window.apply();
        }
    });
}
