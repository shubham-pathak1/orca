//! Bounded PCM handoff: decoding runs outside the endpoint event loop.
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Default)]
struct State<T> {
    samples: VecDeque<T>,
    active: bool,
    draining: bool,
    shutdown: bool,
    epoch: u64,
    submitted_until: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct PcmQueue<T = f32> {
    state: Mutex<State<T>>,
    changed: Condvar,
}
impl<T> PcmQueue<T> {
    pub fn stop_worker(&self) {
        self.state.lock().unwrap().shutdown = true;
        self.changed.notify_all();
    }
}
impl<T: Copy + Default> PcmQueue<T> {
    #[cfg(test)]
    pub(crate) fn queued_samples(&self) -> usize {
        self.state.lock().unwrap().samples.len()
    }
    pub fn has_pending(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.samples.is_empty()
            || state
                .submitted_until
                .is_some_and(|end| Instant::now() < end)
    }
    pub fn submitted(&self, duration: Duration) {
        self.state.lock().unwrap().submitted_until = Some(Instant::now() + duration);
    }
    pub fn set_active(&self, active: bool) {
        let mut state = self.state.lock().unwrap();
        state.active = active && !state.draining;
        self.changed.notify_all();
    }
    pub fn set_draining(&self, draining: bool) {
        let mut state = self.state.lock().unwrap();
        state.draining = draining;
        if draining {
            state.active = false;
        }
        self.changed.notify_all();
    }
    pub fn discard(&self) {
        let mut state = self.state.lock().unwrap();
        state.active = false;
        state.draining = false;
        state.samples.clear();
        state.submitted_until = None;
        state.epoch = state.epoch.wrapping_add(1);
        self.changed.notify_all();
    }
    pub fn stop(&self) {
        self.state.lock().unwrap().shutdown = true;
        self.changed.notify_all();
    }
    pub fn read(&self, mut write: impl FnMut(T), length: usize) -> usize {
        let mut state = self.state.lock().unwrap();
        let consumed = length.min(state.samples.len());
        for _ in 0..length {
            write(state.samples.pop_front().unwrap_or_default());
        }
        self.changed.notify_one();
        consumed
    }
}

pub(crate) fn produce<T: Copy + Default + Send + 'static>(
    mut source: impl Iterator<Item = T> + Send + 'static,
    queue: Arc<PcmQueue<T>>,
    capacity: usize,
    channels: usize,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("orca-pcm".into())
        .spawn(move || {
            let mut chunk = vec![T::default(); 256 * channels];
            loop {
                let (epoch, length) = {
                    let mut state = queue.state.lock().unwrap();
                    while !state.shutdown && (!state.active || state.samples.len() >= capacity) {
                        state = queue.changed.wait(state).unwrap();
                    }
                    if state.shutdown {
                        break;
                    }
                    (state.epoch, chunk.len().min(capacity - state.samples.len()))
                };
                // Never hold the queue lock while decoding or accessing source controls.
                for sample in &mut chunk[..length] {
                    *sample = source.next().unwrap_or_default();
                }
                let mut state = queue.state.lock().unwrap();
                if state.epoch == epoch && !state.shutdown {
                    state.samples.extend(&chunk[..length]);
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn integer_handoff_preserves_low_bits_and_full_scale_without_float_conversion() {
        let expected = [i32::MIN, i32::MAX, 1, -1, 0x12345679, -0x12345679].repeat(1000);
        let queue = Arc::new(PcmQueue::<i32>::default());
        let worker = produce(expected.clone().into_iter(), queue.clone(), 512, 2).unwrap();
        queue.set_active(true);
        let mut actual = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while actual.len() < expected.len() {
            assert!(Instant::now() < deadline);
            let count = queue
                .queued_samples()
                .min(256)
                .min(expected.len() - actual.len());
            if count == 0 {
                thread::yield_now();
                continue;
            }
            queue.read(
                |sample| {
                    let mut bytes = [0; 4];
                    crate::integer_audio::encode_integer(sample, 32, 32, &mut bytes).unwrap();
                    actual.push(i32::from_le_bytes(bytes));
                },
                count,
            );
        }
        queue.stop();
        worker.join().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn queue_is_bounded_parks_when_full_and_wakes_for_shutdown() {
        let queue = Arc::new(PcmQueue::default());
        let worker = produce(std::iter::repeat(0.5), queue.clone(), 512, 2).unwrap();
        queue.set_active(true);
        let deadline = Instant::now() + Duration::from_secs(2);
        while queue.state.lock().unwrap().samples.len() < 512 {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(queue.state.lock().unwrap().samples.len(), 512);
        queue.set_active(false);
        let mut samples = Vec::new();
        queue.read(|sample| samples.push(sample), 600);
        assert!(samples[..512].iter().all(|v| *v == 0.5));
        assert!(samples[512..].iter().all(|v| *v == 0.0));
        queue.stop();
        worker.join().unwrap();
    }

    #[test]
    fn discard_invalidates_in_flight_pcm_and_parks_producer() {
        let queue = PcmQueue::default();
        {
            let mut s = queue.state.lock().unwrap();
            s.active = true;
            s.samples.extend([0.2, 0.3]);
        }
        queue.discard();
        let state = queue.state.lock().unwrap();
        assert!(!state.active);
        assert!(state.samples.is_empty());
        assert_eq!(state.epoch, 1);
    }
    #[test]
    fn cancellation_discards_a_chunk_that_was_decoding_outside_the_queue_lock() {
        let queue = Arc::new(PcmQueue::default());
        let (entered, started) = std::sync::mpsc::channel();
        let (release, ready) = std::sync::mpsc::channel();
        let mut block = Some(ready);
        let source = std::iter::repeat_with(move || {
            if let Some(ready) = block.take() {
                entered.send(()).unwrap();
                ready.recv().unwrap();
            }
            0.5
        });
        let worker = produce(source, queue.clone(), 512, 2).unwrap();
        queue.set_active(true);
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        queue.discard();
        release.send(()).unwrap();
        queue.stop();
        worker.join().unwrap();
        assert!(!queue.has_pending());
    }
}
