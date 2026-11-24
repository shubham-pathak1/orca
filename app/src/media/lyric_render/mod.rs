//! Cache lyric layouts and rasterize requested rows off the UI thread.
mod layout;
mod projection;
mod raster;
use layout::shape;
#[cfg(test)]
use raster::raster;

use crate::{LyricLine, LyricRender, LyricSpan};
use parley::{FontContext, Layout, LayoutContext};
use slint::{Model, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

const MIN_LYRIC_ROW_HEIGHT: f32 = 68.0;

#[derive(Clone, PartialEq)]
pub struct Config {
    pub blur_strength: u8,
    pub left_aligned: bool,
    pub width: u32,
    pub size: u32,
    pub dpr: f32,
    pub family: String,
    pub font_path: String,
}
#[derive(Clone)]
struct Word {
    start: usize,
    end: usize,
    time: f32,
    end_time: f32,
}
struct Line {
    text: String,
    secondary: String,
    words: Vec<Word>,
}
struct Prepare {
    generation: u64,
    config: Config,
    lines: Vec<Line>,
}
#[derive(Clone)]
struct Span {
    caption: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    time: f32,
    end_time: f32,
    rtl: bool,
}
struct Prepared {
    layout: Layout<()>,
    spans: Vec<Span>,
    height: u32,
}
enum RenderResult {
    Heights {
        generation: u64,
        heights: Vec<f32>,
    },
    Bitmap {
        generation: u64,
        index: usize,
        width: u32,
        height: u32,
        padding: u32,
        bytes: Vec<u8>,
        blurred: Vec<u8>,
        ink_left: u32,
        ink_right: u32,
        ink_top: u32,
        ink_bottom: u32,
        spans: Vec<Span>,
    },
}

enum Job {
    Wake,
    Row(u64, usize),
}

struct Entry {
    render: LyricRender,
    bytes: usize,
    used: u64,
}
pub struct Renderer {
    romanized: Vec<String>,
    prepare: Arc<Mutex<Option<Prepare>>>,
    jobs: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<RenderResult>,
    generation: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    config: Option<Config>,
    entries: HashMap<usize, Entry>,
    pending: HashSet<usize>,
    heights: Vec<f32>,
    offsets: Vec<f32>,
    bytes: usize,
    clock: u64,
    layout_revision: u64,
}
const BUDGET: usize = 8 * 1024 * 1024;
impl Renderer {
    pub fn new() -> Self {
        let prepare = Arc::new(Mutex::new(None::<Prepare>));
        let generation = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (jobs, rx) = mpsc::sync_channel::<Job>(8);
        let (tx, results) = mpsc::sync_channel(12);
        let work = prepare.clone();
        let epoch = generation.clone();
        let cancelled = stop.clone();
        let worker = thread::spawn(move || {
            let Ok(first) = rx.recv() else { return };
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            let mut fonts = FontContext::new();
            fonts.collection.register_fonts(
                parley::fontique::Blob::new(Arc::new(
                    include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf").to_vec(),
                )),
                None,
            );
            let mut context = LayoutContext::<()>::new();
            let mut scaler = swash::scale::ScaleContext::new();
            let mut prepared = vec![];
            let mut registered_fonts = HashSet::new();
            let mut current = 0;
            let mut config = None;
            for job in std::iter::once(first).chain(rx) {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                let next = work.lock().unwrap().take();
                if let Some(next) = next {
                    current = next.generation;
                    if !next.config.font_path.is_empty()
                        && registered_fonts.insert(next.config.font_path.clone())
                    {
                        if let Ok(bytes) = std::fs::read(&next.config.font_path) {
                            fonts
                                .collection
                                .register_fonts(parley::fontique::Blob::new(Arc::new(bytes)), None);
                        }
                    }
                    prepared.clear();
                    for line in next.lines {
                        if epoch.load(Ordering::Relaxed) != current
                            || cancelled.load(Ordering::Relaxed)
                        {
                            break;
                        }
                        prepared.push(shape(&mut fonts, &mut context, line, &next.config));
                    }
                    if epoch.load(Ordering::Relaxed) == current {
                        let heights = prepared
                            .iter()
                            .map(|line| line.height as f32 / next.config.dpr)
                            .collect();
                        send_result_cancellable(
                            &tx,
                            RenderResult::Heights {
                                generation: current,
                                heights,
                            },
                            &cancelled,
                            &epoch,
                            current,
                        );
                    }
                    config = Some(next.config);
                }
                let Job::Row(request_generation, index) = job else {
                    continue;
                };
                if request_generation != current
                    || request_generation != epoch.load(Ordering::Relaxed)
                {
                    continue;
                }
                let (Some(line), Some(config)) = (prepared.get(index), config.as_ref()) else {
                    continue;
                };
                let sigma = config.blur_strength.min(100) as f32 * 0.06 * config.dpr;
                let padding = if config.blur_strength > 0 {
                    (sigma * 3.0 + config.size as f32 * 0.15).ceil() as u32
                } else {
                    0
                };
                let width = config.width + padding * 2;
                let height = line.height + padding * 2;
                if width as usize * height as usize * 4 > BUDGET {
                    send_result_cancellable(
                        &tx,
                        RenderResult::Bitmap {
                            generation: request_generation,
                            index,
                            width: 0,
                            height: 0,
                            padding: 0,
                            bytes: vec![],
                            blurred: vec![],
                            ink_left: 0,
                            ink_right: 0,
                            ink_top: 0,
                            ink_bottom: 0,
                            spans: vec![],
                        },
                        &cancelled,
                        &epoch,
                        request_generation,
                    );
                } else {
                    let bitmap = raster::raster_padded(
                        &line.layout,
                        config.width,
                        line.height,
                        padding,
                        &mut scaler,
                    );
                    let blurred = if config.blur_strength > 0 && bitmap.bytes.len() * 2 <= BUDGET {
                        raster::blur_alpha(&bitmap.bytes, width, height, sigma)
                    } else {
                        Vec::new()
                    };
                    send_result_cancellable(
                        &tx,
                        RenderResult::Bitmap {
                            generation: request_generation,
                            index,
                            width,
                            height,
                            padding,
                            bytes: bitmap.bytes,
                            blurred,
                            ink_left: bitmap.ink_left,
                            ink_right: bitmap.ink_right,
                            ink_top: bitmap.ink_top,
                            ink_bottom: bitmap.ink_bottom,
                            spans: line.spans.clone(),
                        },
                        &cancelled,
                        &epoch,
                        request_generation,
                    );
                }
            }
        });
        Self {
            prepare,
            jobs,
            results,
            generation,
            stop,
            worker: Some(worker),
            config: None,
            entries: HashMap::new(),
            pending: HashSet::new(),
            romanized: vec![],
            heights: vec![],
            offsets: vec![0.0],
            bytes: 0,
            clock: 0,
            layout_revision: 0,
        }
    }
    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }
    pub fn invalidate(&mut self) {
        self.layout_revision = self.layout_revision.wrapping_add(1);
        self.config = None;
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.entries.clear();
        self.pending.clear();
        self.heights.clear();
        self.offsets.clear();
        self.offsets.push(0.0);
        self.bytes = 0;
    }
    pub fn set_romanization(&mut self, lines: Vec<String>) {
        if self.romanized != lines {
            self.romanized = lines;
            self.invalidate();
        }
    }
    pub fn configure(&mut self, config: Config, lines: &[LyricLine]) {
        if self.config.as_ref() == Some(&config) {
            return;
        }
        self.invalidate();
        let request_generation = self.generation.load(Ordering::Relaxed);
        let lines = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let mut offset = 0;
                let words = (0..line.words.row_count())
                    .filter_map(|index| line.words.row_data(index))
                    .map(|word| {
                        let start = offset;
                        offset += word.text.len();
                        Word {
                            start,
                            end: offset,
                            time: word.time,
                            end_time: word.end_time,
                        }
                    })
                    .collect();
                Line {
                    text: line.text.to_string(),
                    secondary: self.romanized.get(index).cloned().unwrap_or_default(),
                    words,
                }
            })
            .collect();
        *self.prepare.lock().unwrap() = Some(Prepare {
            generation: request_generation,
            config: config.clone(),
            lines,
        });
        // A full queue already guarantees the worker will observe the latest prepare.
        let _ = self.jobs.try_send(Job::Wake);
        self.config = Some(config);
    }
    pub fn get_signed(&mut self, index: i32) -> LyricRender {
        if index < 0 {
            LyricRender::default()
        } else {
            self.get(index as usize)
        }
    }
    pub fn get(&mut self, index: usize) -> LyricRender {
        if index >= self.heights.len() {
            return LyricRender {
                height: MIN_LYRIC_ROW_HEIGHT,
                ..Default::default()
            };
        }
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&index) {
            entry.used = self.clock;
            return entry.render.clone();
        }
        if self.pending.len() < 12
            && !self.pending.contains(&index)
            && self
                .jobs
                .try_send(Job::Row(self.generation.load(Ordering::Relaxed), index))
                .is_ok()
        {
            self.pending.insert(index);
        }
        LyricRender {
            height: self
                .heights
                .get(index)
                .copied()
                .unwrap_or(MIN_LYRIC_ROW_HEIGHT),
            ..Default::default()
        }
    }
    pub fn offset(&self, index: usize) -> f32 {
        self.offsets.get(index).copied().unwrap_or_else(|| {
            self.offsets.last().copied().unwrap_or(0.0)
                + index.saturating_sub(self.heights.len()) as f32 * MIN_LYRIC_ROW_HEIGHT
        })
    }
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        let current = self.generation.load(Ordering::Relaxed);
        while let Ok(result) = self.results.try_recv() {
            match result {
                RenderResult::Heights {
                    generation,
                    heights,
                } if generation == current => {
                    self.offsets = Vec::with_capacity(heights.len() + 1);
                    self.offsets.push(0.0);
                    for &height in &heights {
                        self.offsets
                            .push(self.offsets.last().copied().unwrap() + height);
                    }
                    self.heights = heights;
                    self.layout_revision = self.layout_revision.wrapping_add(1);
                    changed = true;
                }
                RenderResult::Bitmap {
                    generation,
                    index,
                    width,
                    height,
                    padding,
                    bytes,
                    blurred,
                    ink_left,
                    ink_right,
                    ink_top,
                    ink_bottom,
                    spans,
                } if generation == current => {
                    self.pending.remove(&index);
                    let cost = bytes.len() + blurred.len();
                    if cost > BUDGET {
                        continue;
                    }
                    while self.bytes + cost > BUDGET || self.entries.len() >= 64 {
                        let Some(oldest) = self
                            .entries
                            .iter()
                            .min_by_key(|(_, entry)| entry.used)
                            .map(|(key, _)| *key)
                        else {
                            break;
                        };
                        self.bytes -= self.entries.remove(&oldest).unwrap().bytes;
                    }
                    let dpr = self.config.as_ref().map_or(1.0, |config| config.dpr);
                    let bitmap = if bytes.is_empty() {
                        slint::Image::default()
                    } else {
                        slint::Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &bytes, width, height,
                        ))
                    };
                    let blurred = if blurred.is_empty() {
                        slint::Image::default()
                    } else {
                        slint::Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &blurred, width, height,
                        ))
                    };
                    let spans = spans
                        .into_iter()
                        .map(|span| LyricSpan {
                            caption: span.caption,
                            x: span.x / dpr,
                            y: span.y / dpr,
                            width: span.width / dpr,
                            height: span.height / dpr,
                            time: span.time,
                            end_time: span.end_time,
                            rtl: span.rtl,
                        })
                        .collect::<Vec<_>>();
                    self.clock += 1;
                    let render = LyricRender {
                        bitmap,
                        blurred,
                        padding: padding as f32 / dpr,
                        height: self
                            .heights
                            .get(index)
                            .copied()
                            .unwrap_or(height as f32 / dpr),
                        ink_top: (ink_top as f32 - padding as f32) / dpr,
                        ink_bottom: (ink_bottom as f32 - padding as f32) / dpr,
                        ink_left: (ink_left as f32 - padding as f32) / dpr,
                        ink_right: (ink_right as f32 - padding as f32) / dpr,
                        spans: Rc::new(VecModel::from(spans)).into(),
                    };
                    if let Some(old) = self.entries.insert(
                        index,
                        Entry {
                            render,
                            bytes: cost,
                            used: self.clock,
                        },
                    ) {
                        self.bytes -= old.bytes;
                    }
                    self.bytes += cost;
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.jobs.try_send(Job::Wake);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn send_result_cancellable(
    tx: &mpsc::SyncSender<RenderResult>,
    mut result: RenderResult,
    stop: &AtomicBool,
    generation: &AtomicU64,
    current: u64,
) {
    loop {
        if stop.load(Ordering::Relaxed) || generation.load(Ordering::Relaxed) != current {
            return;
        }
        match tx.try_send(result) {
            Ok(()) => return,
            Err(mpsc::TrySendError::Disconnected(_)) => return,
            Err(mpsc::TrySendError::Full(value)) => result = value,
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests;
