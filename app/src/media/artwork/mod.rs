mod decode;
use decode::decode;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Key {
    pub path: String,
    pub tiles: Vec<String>,
    pub edge: u32,
    pub height: u32,
    pub radius: u32,
    pub backdrop: bool,
    pub revision: u64,
}
pub struct Decoded {
    pub key: Key,
    pub cancelled: bool,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub accent: [u8; 3],
}
struct Entry {
    image: Image,
    bytes: usize,
    used: u64,
    accent: [u8; 3],
}

// The model holds paths, not decoded images. Only visible delegates request
// artwork, and the cache owns a bounded number of decoded buffers.
pub struct ArtworkCache {
    entries: HashMap<Key, Entry>,
    failures: HashMap<String, usize>,
    pending: HashSet<Key>,
    deferred: VecDeque<Key>,
    prefetch: VecDeque<Key>,
    protected: HashSet<String>,
    tx: mpsc::SyncSender<Key>,
    rx: mpsc::Receiver<Decoded>,
    clock: u64,
    bytes: usize,
    limit: usize,
    revision: u64,
    wanted: Arc<Mutex<HashSet<Key>>>,
    priority: Arc<Mutex<Option<Key>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl ArtworkCache {
    pub fn new(limit: usize) -> Self {
        let (tx, jobs) = mpsc::sync_channel::<Key>(8);
        let (results, rx) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let wanted = Arc::new(Mutex::new(HashSet::<Key>::new()));
        let worker_wanted = wanted.clone();
        let priority = Arc::new(Mutex::new(None::<Key>));
        let worker_priority = priority.clone();
        let worker = thread::spawn(move || {
            while !cancelled.load(Ordering::Relaxed) {
                let urgent = worker_priority.lock().unwrap().take();
                let key = if let Some(key) = urgent {
                    key
                } else {
                    let Ok(key) = jobs.recv() else {
                        return;
                    };
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    key
                };
                let decoded = if worker_wanted.lock().unwrap().contains(&key) {
                    decode(key)
                } else {
                    Decoded {
                        key,
                        cancelled: true,
                        width: 0,
                        height: 0,
                        bytes: vec![],
                        accent: [245; 3],
                    }
                };
                // Avoid blocking shutdown on an undrained GUI result queue.
                let mut result = decoded;
                loop {
                    match results.try_send(result) {
                        Ok(()) => break,
                        Err(mpsc::TrySendError::Disconnected(_)) => return,
                        Err(mpsc::TrySendError::Full(value)) => result = value,
                    }
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        });
        Self {
            entries: HashMap::new(),
            failures: HashMap::new(),
            pending: HashSet::new(),
            deferred: VecDeque::new(),
            prefetch: VecDeque::new(),
            protected: HashSet::new(),
            tx,
            rx,
            clock: 0,
            bytes: 0,
            limit,
            revision: 0,
            wanted,
            priority,
            stop,
            worker: Some(worker),
        }
    }
    #[cfg(test)]
    pub fn requested_count(&self) -> usize {
        self.pending.len() + self.deferred.len() + self.entries.len()
    }
    pub fn failed(&self, path: &str) -> bool {
        self.failures.contains_key(path)
    }
    fn remove_entry(&mut self, key: &Key) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes -= entry.bytes;
            if entry.bytes == 0 && !key.backdrop {
                if let Some(count) = self.failures.get_mut(&key.path) {
                    *count -= 1;
                    if *count == 0 {
                        self.failures.remove(&key.path);
                    }
                }
            }
        }
    }
    pub fn get(&mut self, path: &str, edge: u32, backdrop: bool) -> Image {
        self.request(path, &[], edge, backdrop)
    }
    pub fn player_cover(&mut self, path: &str, edge: u32, radius: u32) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        // Height distinguishes the high-quality full-player variant from grid covers.
        self.request_key_inner(
            Key {
                path: path.into(),
                tiles: vec![],
                edge: edge.clamp(8, 1024),
                height: edge.clamp(8, 1024),
                radius,
                backdrop: false,
                revision: self.revision,
            },
            true,
        )
    }
    #[cfg(test)]
    pub fn exact(&mut self, path: &str, edge: u32) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        self.request_key(Key {
            path: path.into(),
            tiles: vec![],
            edge: edge.clamp(8, 1024),
            height: 0,
            radius: 0,
            backdrop: false,
            revision: self.revision,
        })
    }
    /// Cancel queued work for delegates that left the viewport. A running decode
    /// may finish, but its obsolete result will not allocate a GUI image.
    pub fn retain_requests(&mut self, paths: &HashSet<String>) {
        let relevant = |key: &Key| {
            paths.contains(&key.path) || key.tiles.iter().any(|path| paths.contains(path))
        };
        self.deferred.retain(relevant);
        self.prefetch.retain(relevant);
        self.wanted.lock().unwrap().retain(relevant);
        if &self.protected != paths {
            self.protected.clone_from(paths);
        }
    }
    fn grid_key(&self, path: &str, tiles: &[String], edge: u32, landscape: bool) -> Key {
        let edge = edge.clamp(8, 1024);
        Key {
            path: if tiles.len() > 1 {
                "genre-collage".into()
            } else {
                path.into()
            },
            tiles: if tiles.len() > 1 {
                tiles[..tiles.len().min(4)].to_vec()
            } else {
                vec![]
            },
            edge,
            height: if landscape { edge * 3 / 4 } else { 0 },
            radius: (edge / 64).clamp(1, 8),
            backdrop: false,
            revision: self.revision,
        }
    }
    pub fn grid_cover(
        &mut self,
        path: &str,
        tiles: &[String],
        edge: u32,
        landscape: bool,
    ) -> Image {
        if path.is_empty() && tiles.len() <= 1 {
            return Image::default();
        }
        self.request_key(self.grid_key(path, tiles, edge, landscape))
    }
    pub fn prefetch_grid_cover(&mut self, path: &str, edge: u32, landscape: bool) {
        if !path.is_empty() {
            self.prefetch_key(self.grid_key(path, &[], edge, landscape));
        }
    }
    fn request(&mut self, path: &str, tiles: &[String], edge: u32, backdrop: bool) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        let key = Key {
            path: path.into(),
            tiles: tiles.to_vec(),
            edge: edge.clamp(8, 1024).div_ceil(8) * 8,
            height: 0,
            radius: if backdrop {
                0
            } else {
                (edge / 32).clamp(1, 12)
            },
            backdrop,
            revision: self.revision,
        };
        self.request_key(key)
    }
    pub fn rounded(&mut self, path: &str, edge: u32, radius: u32) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        self.request_key(Key {
            path: path.into(),
            tiles: vec![],
            edge: edge.clamp(8, 1024),
            height: 0,
            radius,
            backdrop: false,
            revision: self.revision,
        })
    }
    pub fn panel_backdrop(&mut self, path: &str, width: u32, height: u32, radius: u32) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        let factor = (2048.0 / width.max(height).max(1) as f32).min(1.0);
        self.request_key(Key {
            path: path.into(),
            tiles: vec![],
            edge: (width as f32 * factor).max(8.0) as u32,
            height: (height as f32 * factor).max(8.0) as u32,
            radius: (radius as f32 * factor) as u32,
            backdrop: true,
            revision: self.revision,
        })
    }
    /// Reuse an existing image while the original is decoded, without loading
    /// another file or briefly displaying the previous song's cover.
    pub fn cached_cover(&mut self, paths: &[&str]) -> Image {
        let key = self
            .entries
            .iter()
            .filter(|(key, entry)| {
                !key.backdrop && entry.bytes > 0 && paths.contains(&key.path.as_str())
            })
            .max_by_key(|(key, _)| key.edge)
            .map(|(key, _)| key.clone());
        if let Some(key) = key {
            self.clock += 1;
            let entry = self.entries.get_mut(&key).unwrap();
            entry.used = self.clock;
            entry.image.clone()
        } else {
            Image::default()
        }
    }
    pub fn prefetch_cover(&mut self, path: &str, edge: u32, radius: u32, player: bool) {
        self.prefetch_key(Key {
            path: path.into(),
            tiles: vec![],
            edge: edge.clamp(8, 1024),
            height: if player { edge.clamp(8, 1024) } else { 0 },
            radius,
            backdrop: false,
            revision: self.revision,
        });
    }
    pub fn prefetch_backdrop(&mut self, path: &str, width: u32, height: u32, radius: u32) {
        let factor = (2048.0 / width.max(height).max(1) as f32).min(1.0);
        let edge = (width as f32 * factor).max(8.0) as u32;
        let height = (height as f32 * factor).max(8.0) as u32;
        // Do not speculate on exceptionally large square backgrounds that could
        // consume most of the shared budget before the user switches tracks.
        if edge as usize * height as usize * 4 > self.limit / 3 {
            return;
        }
        self.prefetch_key(Key {
            path: path.into(),
            tiles: vec![],
            edge,
            height,
            radius: (radius as f32 * factor) as u32,
            backdrop: true,
            revision: self.revision,
        });
    }
    fn prefetch_key(&mut self, key: Key) {
        if key.path.is_empty()
            || self.entries.contains_key(&key)
            || self.pending.contains(&key)
            || self.deferred.contains(&key)
            || self.prefetch.contains(&key)
        {
            return;
        }
        if self.prefetch.len() >= 64 {
            if let Some(old) = self.prefetch.pop_front() {
                self.wanted.lock().unwrap().remove(&old);
            }
        }
        self.wanted.lock().unwrap().insert(key.clone());
        self.prefetch.push_back(key);
    }
    fn request_key(&mut self, key: Key) -> Image {
        self.request_key_inner(key, false)
    }
    fn request_key_inner(&mut self, key: Key, priority: bool) -> Image {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return entry.image.clone();
        }
        if let Some(at) = self.prefetch.iter().position(|item| item == &key) {
            self.prefetch.remove(at);
        }
        self.wanted.lock().unwrap().insert(key.clone());
        if priority && !self.pending.contains(&key) {
            if let Some(index) = self.deferred.iter().position(|item| item == &key) {
                self.deferred.remove(index);
            }
            if let Some(old) = self.priority.lock().unwrap().replace(key.clone()) {
                self.pending.remove(&old);
                self.wanted.lock().unwrap().remove(&old);
            }
            self.pending.insert(key);
            // Wake an idle decoder immediately; a full queue already wakes it.
            let _ = self.tx.try_send(Self::wake_key());
            return Image::default();
        }
        if !self.pending.contains(&key) {
            if let Some(index) = self.deferred.iter().position(|item| item == &key) {
                self.deferred.remove(index);
            }
            // Keep recently requested delegates when scrolling outruns decoding.
            // Deferred keys hold only paths, never decoded pixel buffers.
            if self.deferred.len() >= 128 {
                if let Some(old) = self.deferred.pop_front() {
                    self.wanted.lock().unwrap().remove(&old);
                }
            }
            self.deferred.push_back(key);
            self.schedule();
        }
        Image::default()
    }
    fn schedule(&mut self) {
        // Speculative work starts only after all visible/priority work completes.
        if self.pending.is_empty() && self.deferred.is_empty() {
            if let Some(key) = self.prefetch.pop_front() {
                self.deferred.push_back(key);
            }
        }
        while self.pending.len() < 12 {
            let Some(key) = self.deferred.pop_back() else {
                break;
            };
            match self.tx.try_send(key.clone()) {
                Ok(()) => {
                    self.pending.insert(key);
                }
                Err(mpsc::TrySendError::Full(_)) => {
                    self.deferred.push_back(key);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.deferred.clear();
                    break;
                }
            }
        }
    }
    pub fn drain(&mut self) -> Vec<Key> {
        let mut changed = Vec::new();
        while let Ok(mut decoded) = self.rx.try_recv() {
            self.pending.remove(&decoded.key);
            let wanted = self.wanted.lock().unwrap().remove(&decoded.key);
            if decoded.key.revision != self.revision {
                continue;
            }
            if decoded.cancelled || !wanted {
                // Notify a delegate that returned while the cancellation was
                // in flight, so it can request this cover again.
                changed.push(decoded.key);
                continue;
            }
            if decoded.bytes.len() > self.limit {
                decoded.bytes.clear();
            }
            let bytes = decoded.bytes.len();
            while self.bytes + bytes > self.limit || self.entries.len() >= 512 {
                let Some(oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(key, entry)| {
                        (
                            u8::from(
                                self.protected.contains(&key.path)
                                    || key.tiles.iter().any(|p| self.protected.contains(p)),
                            ),
                            u8::from(!key.backdrop && key.edge <= 96),
                            entry.used,
                        )
                    })
                    .map(|(key, _)| key.clone())
                else {
                    break;
                };
                self.remove_entry(&oldest);
            }
            let image = if bytes == 0 {
                Image::default()
            } else {
                Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                    &decoded.bytes,
                    decoded.width,
                    decoded.height,
                ))
            };
            self.clock += 1;
            changed.push(decoded.key.clone());
            self.remove_entry(&decoded.key);
            if bytes == 0 && !decoded.key.backdrop {
                *self.failures.entry(decoded.key.path.clone()).or_default() += 1;
            }
            self.entries.insert(
                decoded.key,
                Entry {
                    image,
                    bytes,
                    used: self.clock,
                    accent: decoded.accent,
                },
            );
            self.bytes += bytes;
        }
        self.schedule();
        changed
    }
    fn wake_key() -> Key {
        Key {
            path: String::new(),
            tiles: vec![],
            edge: 0,
            height: 0,
            radius: 0,
            backdrop: false,
            revision: u64::MAX,
        }
    }
    fn stop_worker(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.tx.try_send(Self::wake_key());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    pub fn clear(&mut self) {
        self.priority.lock().unwrap().take();
        self.wanted.lock().unwrap().clear();
        self.entries.clear();
        self.failures.clear();
        self.pending.clear();
        self.deferred.clear();
        self.prefetch.clear();
        self.protected.clear();
        self.bytes = 0;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn accent(&self, path: &str) -> Option<[u8; 3]> {
        self.entries
            .iter()
            .find(|(key, entry)| key.path == path && !key.backdrop && entry.bytes > 0)
            .map(|(_, entry)| entry.accent)
    }
}
impl Drop for ArtworkCache {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

#[cfg(test)]
mod tests;
