//! Decoder controls are separate from the endpoint's PCM queue. The producer
//! locks once per block; the WASAPI thread never locks or reads a decoder.
use super::{IntegerDecoder, IntegerFormat};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};

pub struct TrackStart {
    pub path: String,
    pub duration_ms: u64,
    pub generation: u64,
}

struct Source {
    reader: IntegerDecoder,
    next: Option<(String, IntegerDecoder)>,
}

pub struct StreamControl {
    source: Mutex<Source>,
    generation: AtomicU64,
    ended: AtomicBool,
    error: Mutex<Option<String>>,
    events: mpsc::Sender<TrackStart>,
    pub format: IntegerFormat,
}

pub struct IntegerStream {
    control: Arc<StreamControl>,
    samples: Vec<i32>,
    cursor: usize,
    length: usize,
    generation: u64,
}

impl StreamControl {
    pub fn new(reader: IntegerDecoder) -> (Arc<Self>, IntegerStream, mpsc::Receiver<TrackStart>) {
        let format = reader.format;
        let (events, receiver) = mpsc::channel();
        let control = Arc::new(Self {
            source: Mutex::new(Source { reader, next: None }),
            generation: AtomicU64::new(0),
            ended: AtomicBool::new(false),
            error: Mutex::new(None),
            events,
            format,
        });
        let stream = IntegerStream {
            control: control.clone(),
            samples: vec![0; 256 * format.channels as usize],
            cursor: 0,
            length: 0,
            generation: 0,
        };
        (control, stream, receiver)
    }

    pub fn replace(&self, reader: IntegerDecoder) -> Result<(), String> {
        if reader.format != self.format {
            return Err("Integer stream format mismatch".into());
        }
        let mut source = self.source.lock().unwrap();
        source.reader = reader;
        source.next = None;
        self.generation.fetch_add(1, Ordering::Release);
        self.ended.store(false, Ordering::Release);
        *self.error.lock().unwrap() = None;
        Ok(())
    }

    pub fn queue(&self, path: String, reader: IntegerDecoder) -> Result<(), String> {
        if reader.format != self.format {
            return Err("Different integer format requires a drained stream change".into());
        }
        self.source.lock().unwrap().next = Some((path, reader));
        self.ended.store(false, Ordering::Release);
        Ok(())
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

impl Iterator for IntegerStream {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        let generation = self.control.generation();
        if generation != self.generation {
            self.generation = generation;
            self.cursor = 0;
            self.length = 0;
        }
        if self.cursor == self.length {
            let mut source = self.control.source.lock().unwrap();
            // Replacement may have happened between the atomic read and lock.
            self.generation = self.control.generation();
            loop {
                let count = match source.reader.read_frames(&mut self.samples) {
                    Ok(count) => count,
                    Err(error) => {
                        *self.control.error.lock().unwrap() = Some(error);
                        self.control.ended.store(true, Ordering::Release);
                        return None;
                    }
                };
                if count > 0 {
                    self.cursor = 0;
                    self.length = count * self.control.format.channels as usize;
                    break;
                }
                if let Some((path, reader)) = source.next.take() {
                    let duration_ms = reader.duration_ms();
                    source.reader = reader;
                    let _ = self.control.events.send(TrackStart {
                        path,
                        duration_ms,
                        generation: self.generation,
                    });
                } else {
                    self.control.ended.store(true, Ordering::Release);
                    return None;
                }
            }
        }
        let sample = self.samples[self.cursor];
        self.cursor += 1;
        Some(sample)
    }
}
