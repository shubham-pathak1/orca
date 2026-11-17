mod decode;
mod gradient;
use decode::decode_cancellable;

const BACKDROP_EDGE: u32 = 512;
const BACKDROP_SOURCE_EDGE: u32 = 86;

use slint::{Image, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackdropStyle {
    Artwork,
    Gradient,
}
impl BackdropStyle {
    pub fn from_setting(value: &str) -> Option<Self> {
        match value {
            "artwork" => Some(Self::Artwork),
            "gradient" => Some(Self::Gradient),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Key {
    pub path: String,
    pub tiles: Vec<String>,
    pub edge: u32,
    pub height: u32,
    pub radius: u32,
    pub backdrop: Option<BackdropStyle>,
    pub blend: Option<BackgroundBlend>,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BackgroundBlend {
    pub base: [u8; 3],
    pub image_alpha: u8,
    pub veil_alpha: u8,
}
impl BackgroundBlend {
    pub fn library(light: bool) -> Self {
        Self {
            base: if light { [252, 251, 247] } else { [9, 10, 12] },
            image_alpha: if light { 20 } else { 61 },
            veil_alpha: 204,
        }
    }
    pub fn apply(self, color: [u8; 3]) -> [u8; 3] {
        std::array::from_fn(|channel| {
            let base = self.base[channel] as u16;
            let alpha = self.image_alpha as u16;
            let under = color[channel] as u16 * alpha / 255 + base * (255 - alpha) / 255;
            let veil = self.veil_alpha as u16;
            (base * veil / 255 + under * (255 - veil) / 255) as u8
        })
    }
}
pub struct Decoded {
    pub key: Key,
    pub cancelled: bool,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub accent: [u8; 3],
}
enum Pixels {
    Rgb(SharedPixelBuffer<Rgb8Pixel>),
    Rgba(SharedPixelBuffer<Rgba8Pixel>),
}
impl Pixels {
    fn into_image(self) -> Image {
        match self {
            Self::Rgb(pixels) => Image::from_rgb8(pixels),
            Self::Rgba(pixels) => Image::from_rgba8(pixels),
        }
    }
}
struct Ready {
    key: Key,
    cancelled: bool,
    pixels: Option<Pixels>,
    bytes: usize,
    accent: [u8; 3],
}
impl Ready {
    fn from_decoded(decoded: Decoded, limit: usize) -> Self {
        let opaque = !decoded.cancelled
            && !decoded.bytes.is_empty()
            && decoded.bytes.len() / 4 * 3 <= limit
            && decoded.bytes.chunks_exact(4).all(|pixel| pixel[3] == 255);
        let bytes = if opaque {
            decoded.bytes.len() / 4 * 3
        } else {
            decoded.bytes.len()
        };
        let pixels = (!decoded.cancelled && bytes > 0 && bytes <= limit).then(|| {
            if opaque {
                let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(decoded.width, decoded.height);
                for (source, target) in decoded
                    .bytes
                    .chunks_exact(4)
                    .zip(pixels.make_mut_bytes().chunks_exact_mut(3))
                {
                    target.copy_from_slice(&source[..3]);
                }
                Pixels::Rgb(pixels)
            } else {
                Pixels::Rgba(SharedPixelBuffer::clone_from_slice(
                    &decoded.bytes,
                    decoded.width,
                    decoded.height,
                ))
            }
        });
        Self {
            key: decoded.key,
            cancelled: decoded.cancelled,
            bytes: if pixels.is_some() { bytes } else { 0 },
            pixels,
            accent: decoded.accent,
        }
    }
}
struct Entry {
    image: Image,
    bytes: usize,
    used: u64,
}
#[derive(Default)]
struct ReadyWake {
    pending: AtomicBool,
    handler: Mutex<Option<Box<dyn Fn() + Send>>>,
}
impl ReadyWake {
    fn notify(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            if let Some(handler) = self.handler.lock().unwrap().as_ref() {
                handler();
            }
        }
    }
}

// Track metadata retains artwork paths. Visible delegates hold image handles;
// the cache separately bounds retained buffers and queues nearby prefetch work.
pub struct ArtworkCache {
    entries: HashMap<Key, Entry>,
    library_key: Option<Key>,
    failures: HashMap<String, usize>,
    palettes: HashMap<String, ([u8; 3], usize)>,
    pending: HashSet<Key>,
    deferred: VecDeque<Key>,
    prefetch: VecDeque<Key>,
    protected: HashSet<String>,
    scroll_deferred: bool,
    immediate_paths: HashSet<String>,
    tx: mpsc::SyncSender<Key>,
    rx: mpsc::Receiver<Ready>,
    clock: u64,
    bytes: usize,
    limit: usize,
    revision: u64,
    wanted: Arc<Mutex<HashSet<Key>>>,
    priority: Arc<Mutex<Option<Key>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    ready_wake: Arc<ReadyWake>,
}
impl ArtworkCache {
    pub fn new(limit: usize) -> Self {
        let (tx, jobs) = mpsc::sync_channel::<Key>(8);
        let (results, rx) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let wanted = Arc::new(Mutex::new(HashSet::<Key>::new()));
        let priority = Arc::new(Mutex::new(None::<Key>));
        let ready_wake = Arc::new(ReadyWake::default());
        let worker_wake = ready_wake.clone();
        let cancelled = stop.clone();
        let worker_wanted = wanted.clone();
        let worker_priority = priority.clone();
        let worker = thread::spawn(move || {
            while !cancelled.load(Ordering::Relaxed) {
                let urgent = worker_priority.lock().unwrap().take();
                let key = if let Some(key) = urgent {
                    key
                } else {
                    match jobs.recv() {
                        Ok(key) => key,
                        Err(_) => return,
                    }
                };
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let mut decoded = decode_cancellable(key, |key| {
                    !cancelled.load(Ordering::Relaxed)
                        && worker_wanted.lock().unwrap().contains(key)
                });
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                if !worker_wanted.lock().unwrap().contains(&decoded.key) {
                    decoded.cancelled = true;
                    decoded.bytes.clear();
                }
                // Allocate/copy the renderer buffer here, never on the UI thread.
                let mut result = Ready::from_decoded(decoded, limit);
                loop {
                    match results.try_send(result) {
                        Ok(()) => {
                            worker_wake.notify();
                            break;
                        }
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
            library_key: None,
            failures: HashMap::new(),
            palettes: HashMap::new(),
            pending: HashSet::new(),
            deferred: VecDeque::new(),
            prefetch: VecDeque::new(),
            protected: HashSet::new(),
            scroll_deferred: false,
            immediate_paths: HashSet::new(),
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
            ready_wake,
        }
    }
    pub fn on_ready(&mut self, handler: impl Fn() + Send + 'static) {
        *self.ready_wake.handler.lock().unwrap() = Some(Box::new(handler));
        self.ready_wake.pending.store(false, Ordering::Release);
    }
    pub fn clear_ready_handler(&mut self) {
        *self.ready_wake.handler.lock().unwrap() = None;
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
            if entry.bytes > 0 && key.backdrop.is_none() {
                if let Some((_, count)) = self.palettes.get_mut(&key.path) {
                    *count -= 1;
                    if *count == 0 {
                        self.palettes.remove(&key.path);
                    }
                }
            }
            if entry.bytes == 0 && key.backdrop.is_none() {
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
    pub fn library_background(&mut self, path: &str, light: bool) -> Image {
        if path.is_empty() {
            self.release_library_background();
            return Image::default();
        }
        let key = Key {
            path: path.into(),
            tiles: vec![],
            edge: BACKDROP_EDGE,
            height: 0,
            radius: 0,
            backdrop: Some(BackdropStyle::Artwork),
            blend: Some(BackgroundBlend::library(light)),
            revision: self.revision,
        };
        if self.library_key.as_ref() != Some(&key) {
            self.release_library_background();
            self.library_key = Some(key.clone());
        }
        self.request_key(key)
    }
    fn release_library_background(&mut self) {
        if let Some(old) = self.library_key.take() {
            self.wanted.lock().unwrap().remove(&old);
            self.deferred.retain(|key| key != &old);
            self.prefetch.retain(|key| key != &old);
            self.remove_entry(&old);
        }
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
                backdrop: None,
                blend: None,
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
            backdrop: None,
            blend: None,
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
        // Match the physical cover size so Slint does not resize a bucketed bitmap again.
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
            backdrop: None,
            blend: None,
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
            backdrop: backdrop.then_some(BackdropStyle::Artwork),
            blend: None,
            revision: self.revision,
        };
        self.request_key(key)
    }
    pub fn rounded(&mut self, path: &str, edge: u32, radius: u32) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        self.request_key(self.cover_key(path, edge, radius, false))
    }
    fn cover_key(&self, path: &str, edge: u32, radius: u32, player: bool) -> Key {
        let circle = !player && radius > 0 && radius >= edge / 2 && edge > 0;
        let edge = edge.clamp(8, 1024);
        Key {
            path: path.into(),
            tiles: vec![],
            edge,
            height: if player { edge } else { 0 },
            radius: if circle { edge / 2 } else { radius },
            backdrop: None,
            blend: None,
            revision: self.revision,
        }
    }
    pub fn panel_backdrop(
        &mut self,
        path: &str,
        width: u32,
        height: u32,
        radius: u32,
        style: BackdropStyle,
    ) -> Image {
        if path.is_empty() {
            return Image::default();
        }
        let factor = (BACKDROP_EDGE as f32 / width.max(height).max(1) as f32).min(1.0);
        self.request_key(Key {
            path: path.into(),
            tiles: vec![],
            edge: (width as f32 * factor).max(8.0) as u32,
            height: (height as f32 * factor).max(8.0) as u32,
            radius: (radius as f32 * factor) as u32,
            backdrop: Some(style),
            blend: None,
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
                key.backdrop.is_none() && entry.bytes > 0 && paths.contains(&key.path.as_str())
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
        self.prefetch_key(self.cover_key(path, edge, radius, player));
    }
    pub fn prefetch_backdrop(
        &mut self,
        path: &str,
        width: u32,
        height: u32,
        radius: u32,
        style: BackdropStyle,
    ) {
        let factor = (BACKDROP_EDGE as f32 / width.max(height).max(1) as f32).min(1.0);
        let edge = (width as f32 * factor).max(8.0) as u32;
        let height = (height as f32 * factor).max(8.0) as u32;
        if edge as usize * height as usize * 4 > self.limit / 3 {
            return;
        }
        self.prefetch_key(Key {
            path: path.into(),
            tiles: vec![],
            edge,
            height,
            radius: (radius as f32 * factor) as u32,
            backdrop: Some(style),
            blend: None,
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
    pub fn defer_scrolling(&mut self, deferred: bool, immediate: &[&str]) {
        self.scroll_deferred = deferred;
        if deferred {
            self.immediate_paths
                .retain(|path| immediate.contains(&path.as_str()));
            for path in immediate.iter().filter(|path| !path.is_empty()) {
                if !self.immediate_paths.contains(*path) {
                    self.immediate_paths.insert((*path).to_owned());
                }
            }
        } else {
            self.immediate_paths.clear();
        }
        self.schedule();
    }
    fn schedule(&mut self) {
        // Speculative work starts only after all visible/priority work completes.
        if !self.scroll_deferred && self.pending.is_empty() && self.deferred.is_empty() {
            if let Some(key) = self.prefetch.pop_front() {
                self.deferred.push_back(key);
            }
        }
        while self.pending.len() < 12 {
            let key = if self.scroll_deferred {
                let immediate = self
                    .deferred
                    .iter()
                    .rposition(|key| self.immediate_paths.contains(&key.path));
                let Some(index) = immediate else {
                    break;
                };
                self.deferred.remove(index).unwrap()
            } else {
                let Some(key) = self.deferred.pop_back() else {
                    break;
                };
                key
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
        self.ready_wake.pending.store(false, Ordering::Release);
        let mut changed = Vec::new();
        while let Ok(decoded) = self.rx.try_recv() {
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
            let bytes = decoded.bytes;
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
                            u8::from(key.backdrop.is_none() && key.edge <= 96),
                            entry.used,
                        )
                    })
                    .map(|(key, _)| key.clone())
                else {
                    break;
                };
                self.remove_entry(&oldest);
            }
            let image = decoded.pixels.map(Pixels::into_image).unwrap_or_default();
            self.clock += 1;
            changed.push(decoded.key.clone());
            self.remove_entry(&decoded.key);
            if bytes == 0 && decoded.key.backdrop.is_none() {
                *self.failures.entry(decoded.key.path.clone()).or_default() += 1;
            }
            if bytes > 0 && decoded.key.backdrop.is_none() {
                self.palettes
                    .entry(decoded.key.path.clone())
                    .and_modify(|(_, count)| *count += 1)
                    .or_insert((decoded.accent, 1));
            }
            self.entries.insert(
                decoded.key,
                Entry {
                    image,
                    bytes,
                    used: self.clock,
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
            backdrop: None,
            blend: None,
            revision: u64::MAX,
        }
    }
    fn stop_worker(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Close the queue to wake the decoder without idle polling.
        let (disconnected, _) = mpsc::sync_channel(0);
        drop(std::mem::replace(&mut self.tx, disconnected));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    pub fn clear(&mut self) {
        self.priority.lock().unwrap().take();
        self.wanted.lock().unwrap().clear();
        self.entries.clear();
        self.failures.clear();
        self.palettes.clear();
        self.pending.clear();
        self.deferred.clear();
        self.prefetch.clear();
        self.protected.clear();
        self.bytes = 0;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn accent(&self, path: &str) -> Option<[u8; 3]> {
        self.palettes.get(path).map(|(accent, _)| *accent)
    }
}
impl Drop for ArtworkCache {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

#[cfg(test)]
mod tests;
