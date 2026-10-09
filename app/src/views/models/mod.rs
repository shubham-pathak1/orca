mod groups;
mod songs;
pub use groups::GroupGrid;
pub use songs::{SongGrid, SongList};

use crate::{
    artwork::{ArtworkCache, Key},
    protocol::Request,
    CatalogGroup, Song,
};
use orca_services::types::{Group, Track};
use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
    sync::mpsc::Sender,
};

pub fn duration(ms: u64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}
pub fn text_song(track: &Track, index: usize) -> Song {
    Song {
        id: track.id as i32,
        path: track.path.clone().into(),
        title: track.title.clone().into(),
        artist: track.artist.clone().into(),
        album: track.album.clone().into(),
        album_artist: track.album_artist.clone().into(),
        duration: duration(track.duration_ms).into(),
        quality: track.quality.to_uppercase().into(),
        index: index as i32,
        ..Default::default()
    }
}
pub fn song(track: &Track, index: usize, edge: u32, cache: &Rc<RefCell<ArtworkCache>>) -> Song {
    build_song(track, index, edge, true, cache)
}
#[cfg(test)]
pub fn grid_song(
    track: &Track,
    index: usize,
    edge: u32,
    cache: &Rc<RefCell<ArtworkCache>>,
) -> Song {
    build_song(track, index, edge, false, cache)
}
fn artwork_path(track: &Track, thumbnail: bool) -> &str {
    if thumbnail && !track.artwork_thumb.is_empty() {
        &track.artwork_thumb
    } else if !track.artwork.is_empty() {
        &track.artwork
    } else {
        &track.artwork_thumb
    }
}
fn build_song(
    track: &Track,
    index: usize,
    edge: u32,
    thumbnail: bool,
    cache: &Rc<RefCell<ArtworkCache>>,
) -> Song {
    let path = artwork_path(track, thumbnail);
    let cover_missing = path.is_empty() || cache.borrow().failed(path);
    Song {
        id: track.id as i32,
        path: track.path.clone().into(),
        title: track.title.clone().into(),
        artist: track.artist.clone().into(),
        album: track.album.clone().into(),
        album_artist: track.album_artist.clone().into(),
        duration: duration(track.duration_ms).into(),
        quality: track.quality.to_uppercase().into(),
        cover_missing,
        cover: if thumbnail {
            // Exact physical size avoids another resize in the software renderer.
            cache
                .borrow_mut()
                .rounded(path, edge, (edge / 32).clamp(1, 12))
        } else {
            cache.borrow_mut().grid_cover(path, &[], edge, false)
        },
        index: index as i32,
    }
}
pub fn physical_artwork_edge(logical: u32, dpr: f32) -> u32 {
    (logical as f32 * dpr.max(1.0)).round().max(1.0) as u32
}

pub struct Tracks {
    pub artwork: Cell<bool>,
    pub list_edge: Cell<u32>,
    pub generation: Cell<u64>,
    pub count: Cell<usize>,
    rows: RefCell<HashMap<usize, Track>>,
    text_rows: RefCell<HashMap<usize, Song>>,
    pages: RefCell<Vec<usize>>,
    pending: RefCell<HashSet<usize>>,
    pub cache: Rc<RefCell<ArtworkCache>>,
    tx: Sender<Request>,
}
impl Tracks {
    pub fn new(tx: Sender<Request>, cache: Rc<RefCell<ArtworkCache>>) -> Self {
        Self {
            artwork: Cell::new(true),
            list_edge: Cell::new(28),
            generation: Cell::new(0),
            count: Cell::new(0),
            rows: RefCell::new(HashMap::new()),
            text_rows: RefCell::new(HashMap::new()),
            pages: RefCell::new(vec![]),
            pending: RefCell::new(HashSet::new()),
            cache,
            tx,
        }
    }
    pub fn apply(&self, generation: u64, offset: usize, count: usize, rows: Vec<Track>) {
        if self.generation.get() != generation {
            self.rows.borrow_mut().clear();
            self.text_rows.borrow_mut().clear();
            self.pages.borrow_mut().clear();
            self.pending.borrow_mut().clear();
            self.generation.set(generation);
        }
        self.count.set(count);
        self.pending.borrow_mut().remove(&offset);
        let mut pages = self.pages.borrow_mut();
        pages.retain(|at| *at != offset);
        pages.push(offset);
        while pages.len() > 8 {
            let old = pages.remove(0);
            self.rows
                .borrow_mut()
                .retain(|at, _| *at < old || *at >= old + 128);
        }
        self.text_rows
            .borrow_mut()
            .retain(|index, _| self.rows.borrow().contains_key(index));
        for (index, track) in rows.into_iter().enumerate() {
            self.text_rows
                .borrow_mut()
                .insert(offset + index, text_song(&track, offset + index));
            self.rows.borrow_mut().insert(offset + index, track);
        }
    }
    pub fn row(&self, index: usize, edge: u32, grid: bool) -> Song {
        if let Some(track) = self.rows.borrow().get(&index) {
            let offset = index / 128 * 128;
            let mut pages = self.pages.borrow_mut();
            if pages.last() != Some(&offset) {
                pages.retain(|at| *at != offset);
                pages.push(offset);
            }
            let mut song = self
                .text_rows
                .borrow()
                .get(&index)
                .cloned()
                .unwrap_or_default();
            if self.artwork.get() {
                let path = artwork_path(track, !grid);
                let mut cache = self.cache.borrow_mut();
                song.cover_missing = path.is_empty() || cache.failed(path);
                song.cover = if grid {
                    cache.grid_cover(path, &[], edge, false)
                } else {
                    cache.rounded(path, edge, (edge / 32).clamp(1, 12))
                };
            }
            return song;
        }
        let offset = index / 128 * 128;
        if self.pending.borrow_mut().insert(offset) {
            let _ = self
                .tx
                .send(Request::Page(self.generation.get(), offset as u32));
        }
        Song {
            index: index as i32,
            ..Default::default()
        }
    }
    pub fn artwork_paths(&self, indices: impl Iterator<Item = usize>, paths: &mut HashSet<String>) {
        let rows = self.rows.borrow();
        for index in indices {
            if let Some(track) = rows.get(&index) {
                paths.extend([track.artwork.clone(), track.artwork_thumb.clone()]);
            }
        }
    }
    pub fn prefetch_range(&self, start: usize, end: usize, edge: u32, grid: bool) {
        if !self.artwork.get() {
            return;
        }
        let rows = self.rows.borrow();
        let mut cache = self.cache.borrow_mut();
        for index in start..end.min(self.count.get()) {
            if let Some(track) = rows.get(&index) {
                cache.prefetch_cover(
                    artwork_path(track, !grid),
                    edge,
                    if grid {
                        (edge / 64).clamp(1, 8)
                    } else {
                        (edge / 32).clamp(1, 12)
                    },
                    false,
                );
            } else {
                let offset = index / 128 * 128;
                if self.pending.borrow_mut().insert(offset) {
                    let _ = self
                        .tx
                        .send(Request::Page(self.generation.get(), offset as u32));
                }
            }
        }
    }
    pub fn artwork_indices(&self, keys: &[Key]) -> Vec<usize> {
        self.rows
            .borrow()
            .iter()
            .filter(|(_, track)| {
                keys.iter().any(|key| {
                    !key.backdrop && (key.path == track.artwork || key.path == track.artwork_thumb)
                })
            })
            .map(|(&index, _)| index)
            .collect()
    }
}

#[cfg(test)]
mod tests;
