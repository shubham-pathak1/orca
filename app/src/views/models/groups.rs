use super::*;

pub struct GroupGrid {
    pub landscape: Cell<bool>,
    pub visible_rows: RefCell<HashMap<usize, Weak<VecModel<CatalogGroup>>>>,
    pub artwork: Cell<bool>,
    pub circular: Cell<bool>,
    pub groups: RefCell<Vec<Group>>,
    pub notify: ModelNotify,
    pub columns: Cell<usize>,
    pub cache: Rc<RefCell<ArtworkCache>>,
    pub edge: Cell<u32>,
}
impl GroupGrid {
    pub fn reset(&self) {
        self.visible_rows.borrow_mut().clear();
        self.notify.reset();
    }
    pub fn visible_paths(&self, paths: &mut HashSet<String>) {
        let groups = self.groups.borrow();
        let columns = self.columns.get().max(1);
        for (&row, model) in self.visible_rows.borrow().iter() {
            if model.strong_count() == 0 {
                continue;
            }
            for group in groups.iter().skip(row * columns).take(columns) {
                paths.insert(group.artwork.clone());
                paths.extend(group.artwork_tiles.iter().take(4).cloned());
            }
        }
    }
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
            ((end + 1) * columns + 12).min(self.groups.borrow().len()),
        ))
    }
    pub fn nearby_paths(&self, start: usize, end: usize, paths: &mut HashSet<String>) {
        for group in self.groups.borrow().iter().skip(start).take(end - start) {
            paths.insert(group.artwork.clone());
            paths.extend(group.artwork_tiles.iter().take(4).cloned());
        }
    }
    pub fn prefetch_range(&self, start: usize, end: usize) {
        if !self.artwork.get() {
            return;
        }
        let edge = self.edge.get();
        let mut cache = self.cache.borrow_mut();
        for group in self.groups.borrow().iter().skip(start).take(end - start) {
            // Collages decode only on demand to avoid speculative multi-image work.
            if group.artwork_tiles.len() > 1 {
                continue;
            }
            if self.circular.get() {
                cache.prefetch_cover(&group.artwork, edge, edge / 2, false);
            } else {
                cache.prefetch_grid_cover(&group.artwork, edge, self.landscape.get());
            }
        }
    }
    fn group(&self, group: &Group) -> CatalogGroup {
        CatalogGroup {
            key: group.key.clone().into(),
            secondary: group.secondary.clone().into(),
            title: group.title.clone().into(),
            subtitle: group.subtitle.clone().into(),
            count: group.count as i32,
            cover_missing: group.artwork.is_empty() && group.artwork_tiles.is_empty()
                || self.cache.borrow().failed(&group.artwork),
            cover: if !self.artwork.get() {
                slint::Image::default()
            } else if self.circular.get() {
                self.cache.borrow_mut().rounded(
                    &group.artwork,
                    self.edge.get(),
                    self.edge.get() / 2,
                )
            } else {
                self.cache.borrow_mut().grid_cover(
                    &group.artwork,
                    &group.artwork_tiles,
                    self.edge.get(),
                    self.landscape.get(),
                )
            },
        }
    }
    pub fn artwork_changed(&self, keys: &[Key]) {
        let columns = self.columns.get().max(1);
        let groups = self.groups.borrow();
        let mut visible = self.visible_rows.borrow_mut();
        visible.retain(|_, row| row.strong_count() > 0);
        for (&row, model) in visible.iter() {
            let Some(model) = model.upgrade() else {
                continue;
            };
            for (cell, group) in groups.iter().skip(row * columns).take(columns).enumerate() {
                if keys.iter().any(|key| {
                    !key.backdrop
                        && (key.path == group.artwork
                            || !key.tiles.is_empty()
                                && key.tiles.iter().eq(group.artwork_tiles.iter().take(4)))
                }) {
                    let mut value = self.group(group);
                    if let Some(old) = model.row_data(cell) {
                        if old.key == value.key
                            && old.secondary == value.secondary
                            && old.cover.size().width > 0
                            && value.cover.size().width == 0
                            && !value.cover_missing
                        {
                            value.cover = old.cover;
                        }
                    }
                    if model.row_data(cell).as_ref() != Some(&value) {
                        model.set_row_data(cell, value);
                    }
                }
            }
        }
    }
}
impl Model for GroupGrid {
    type Data = ModelRc<CatalogGroup>;
    fn row_count(&self) -> usize {
        self.groups
            .borrow()
            .len()
            .div_ceil(self.columns.get().max(1))
    }
    fn row_data(&self, index: usize) -> Option<Self::Data> {
        let groups = self.groups.borrow();
        let columns = self.columns.get().max(1);
        if index * columns >= groups.len() {
            return None;
        }
        let mut visible = self.visible_rows.borrow_mut();
        visible.retain(|_, row| row.strong_count() > 0);
        if let Some(model) = visible.get(&index).and_then(Weak::upgrade) {
            return Some(model.into());
        }
        let mut row: Vec<_> = groups
            .iter()
            .skip(index * columns)
            .take(columns)
            .map(|group| self.group(group))
            .collect();
        row.resize(columns, CatalogGroup::default());
        let model = Rc::new(VecModel::from(row));
        visible.insert(index, Rc::downgrade(&model));
        Some(model.into())
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
