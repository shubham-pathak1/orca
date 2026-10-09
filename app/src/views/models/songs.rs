use super::*;

pub struct SongList {
    pub store: Rc<Tracks>,
    pub notify: ModelNotify,
}
impl Model for SongList {
    type Data = Song;
    fn row_count(&self) -> usize {
        self.store.count.get()
    }
    fn row_data(&self, index: usize) -> Option<Song> {
        (index < self.row_count()).then(|| self.store.row(index, self.store.list_edge.get(), false))
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
pub struct SongGrid {
    pub visible_rows: RefCell<HashMap<usize, Weak<VecModel<Song>>>>,
    pub store: Rc<Tracks>,
    pub notify: ModelNotify,
    pub columns: Cell<usize>,
    pub edge: Cell<u32>,
}
impl SongGrid {
    pub fn nearby_range(&self) -> Option<(usize, usize)> {
        let rows = self.visible_rows.borrow();
        let mut live = rows
            .iter()
            .filter(|(_, model)| model.strong_count() > 0)
            .map(|(&row, _)| row);
        let first = live.next()?;
        let (start, end) = live.fold((first, first), |(start, end), row| {
            (start.min(row), end.max(row))
        });
        let columns = self.columns.get().max(1);
        Some((
            (start * columns).saturating_sub(12),
            ((end + 1) * columns + 12).min(self.store.count.get()),
        ))
    }
    pub fn artwork_changed(&self, indices: &[usize]) {
        let columns = self.columns.get().max(1);
        let mut visible = self.visible_rows.borrow_mut();
        visible.retain(|_, row| row.strong_count() > 0);
        for &index in indices {
            if let Some(row) = visible.get(&(index / columns)).and_then(Weak::upgrade) {
                let cell = index % columns;
                let mut song = self.store.row(index, self.edge.get(), true);
                if let Some(old) = row.row_data(cell) {
                    if old.path == song.path
                        && old.cover.size().width > 0
                        && song.cover.size().width == 0
                        && !song.cover_missing
                    {
                        song.cover = old.cover;
                    }
                }
                if row.row_data(cell).as_ref() != Some(&song) {
                    row.set_row_data(cell, song);
                }
            }
        }
    }
}
impl Model for SongGrid {
    type Data = ModelRc<Song>;
    fn row_count(&self) -> usize {
        self.store.count.get().div_ceil(self.columns.get().max(1))
    }
    fn row_data(&self, index: usize) -> Option<Self::Data> {
        if index >= self.row_count() {
            return None;
        }
        let columns = self.columns.get().max(1);
        let mut row: Vec<_> = (index * columns
            ..((index + 1) * columns).min(self.store.count.get()))
            .map(|at| self.store.row(at, self.edge.get(), true))
            .collect();
        // Empty cells keep the last row the same width as every preceding row.
        row.resize(columns, Song::default());
        let mut visible = self.visible_rows.borrow_mut();
        visible.retain(|_, row| row.strong_count() > 0);
        let model = if let Some(model) = visible
            .get(&index)
            .and_then(Weak::upgrade)
            .filter(|model| model.row_count() == columns)
        {
            for (cell, mut song) in row.into_iter().enumerate() {
                if let Some(old) = model.row_data(cell) {
                    if old.path == song.path
                        && old.cover.size().width > 0
                        && song.cover.size().width == 0
                        && !song.cover_missing
                    {
                        song.cover = old.cover;
                    }
                }
                if model.row_data(cell).as_ref() != Some(&song) {
                    model.set_row_data(cell, song);
                }
            }
            model
        } else {
            Rc::new(VecModel::from(row))
        };
        visible.insert(index, Rc::downgrade(&model));
        Some(model.into())
    }
    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
