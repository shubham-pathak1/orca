use super::VISUALIZER_BUFFER_SIZE;
use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type};
use rodio::{Decoder, Sink, Source};
use std::{
    fs::File,
    sync::{Arc, Mutex},
    time::Duration,
};
const EQ_BAND_FREQUENCIES_HZ: [f32; 5] = [60.0, 230.0, 910.0, 3600.0, 14000.0];
const EQ_Q: f32 = 0.707;
const VISUALIZER_SAMPLES_PER_PEAK: usize = 1024;

struct TransitionSource<S: Source<Item = f32>> {
    inner: S,
    callback: Option<Box<dyn FnOnce() + Send>>,
}

impl<S: Source<Item = f32>> TransitionSource<S> {
    fn new(inner: S, callback: impl FnOnce() + Send + 'static) -> Self {
        Self {
            inner,
            callback: Some(Box::new(callback)),
        }
    }
}

impl<S: Source<Item = f32>> Iterator for TransitionSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(cb) = self.callback.take() {
            cb();
        }
        self.inner.next()
    }
}

impl<S: Source<Item = f32>> Source for TransitionSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

struct EqSource<S: Source<Item = f32>> {
    inner: S,
    filters: Vec<Vec<DirectForm1<f32>>>,
    channels: usize,
    channel_cursor: usize,
}

impl<S: Source<Item = f32>> EqSource<S> {
    fn new(inner: S, gains_db: [f32; 5]) -> Self {
        let channels = inner.channels().max(1) as usize;
        let sample_rate_hz = inner.sample_rate() as f32;

        let coefficients = EQ_BAND_FREQUENCIES_HZ
            .iter()
            .zip(gains_db.iter())
            .filter_map(|(freq_hz, gain_db)| {
                Coefficients::<f32>::from_params(
                    Type::PeakingEQ(*gain_db),
                    sample_rate_hz.hz(),
                    freq_hz.hz(),
                    EQ_Q,
                )
                .ok()
            })
            .collect::<Vec<_>>();

        let filters = (0..channels)
            .map(|_| {
                coefficients
                    .iter()
                    .map(|coeff| DirectForm1::<f32>::new(*coeff))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Self {
            inner,
            filters,
            channels,
            channel_cursor: 0,
        }
    }

    fn process_sample(&mut self, sample: f32) -> f32 {
        let channel_index = self.channel_cursor % self.channels;
        let mut out = sample;
        for filter in self.filters[channel_index].iter_mut() {
            out = filter.run(out);
        }
        self.channel_cursor = (self.channel_cursor + 1) % self.channels;
        out
    }
}

impl<S: Source<Item = f32>> Iterator for EqSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|sample| self.process_sample(sample))
    }
}

struct VisualizerSource<S: Source<Item = f32>> {
    inner: S,
    peaks: Arc<Mutex<std::collections::VecDeque<f32>>>,
    sample_counter: usize,
    current_peak: f32,
}

impl<S: Source<Item = f32>> VisualizerSource<S> {
    fn new(inner: S, peaks: Arc<Mutex<std::collections::VecDeque<f32>>>) -> Self {
        Self {
            inner,
            peaks,
            sample_counter: 0,
            current_peak: 0.0,
        }
    }
}

impl<S: Source<Item = f32>> Iterator for VisualizerSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let abs_sample = sample.abs();
        if abs_sample > self.current_peak {
            self.current_peak = abs_sample;
        }

        self.sample_counter += 1;
        if self.sample_counter >= VISUALIZER_SAMPLES_PER_PEAK {
            if let Ok(mut peaks) = self.peaks.lock() {
                peaks.push_back(self.current_peak);
                if peaks.len() > VISUALIZER_BUFFER_SIZE {
                    peaks.pop_front();
                }
            }
            self.sample_counter = 0;
            self.current_peak = 0.0;
        }
        Some(sample)
    }
}

impl<S: Source<Item = f32>> Source for VisualizerSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.sample_counter = 0;
        self.current_peak = 0.0;
        self.inner.try_seek(pos)
    }
}

impl<S: Source<Item = f32>> Source for EqSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        for channel_filters in &mut self.filters {
            for filter in channel_filters {
                filter.reset_state();
            }
        }
        self.channel_cursor = 0;
        self.inner.try_seek(pos)
    }
}

pub(super) fn create_track_source(
    path: &str,
    start_pos: Duration,
    eq_enabled: bool,
    eq_gains: [f32; 5],
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
    on_start: Option<Box<dyn FnOnce(u64) + Send>>,
) -> Result<(u64, Box<dyn Source<Item = f32> + Send>), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open '{path}': {e}"))?;
    let decoder = Decoder::try_from(file).map_err(|e| format!("Decode error for '{path}': {e}"))?;
    let total_duration_ms = decoder
        .total_duration()
        .unwrap_or_else(|| Duration::from_secs(0))
        .as_millis() as u64;

    let source = decoder.skip_duration(start_pos);
    let vis_source = VisualizerSource::new(source, Arc::clone(visualizer));

    let eq_source: Box<dyn Source<Item = f32> + Send> = if eq_enabled {
        Box::new(EqSource::new(vis_source, eq_gains))
    } else {
        Box::new(vis_source)
    };

    if let Some(cb) = on_start {
        let duration_for_callback = total_duration_ms;
        Ok((
            total_duration_ms,
            Box::new(TransitionSource::new(eq_source, move || {
                cb(duration_for_callback)
            })),
        ))
    } else {
        Ok((total_duration_ms, eq_source))
    }
}

pub(super) fn load_track_into_sink(
    sink: &Sink,
    path: &str,
    start_pos: Duration,
    eq_enabled: bool,
    eq_gains: [f32; 5],
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
) -> Result<u64, String> {
    let (duration, source) =
        create_track_source(path, start_pos, eq_enabled, eq_gains, visualizer, None)?;
    sink.clear();
    sink.append(source);
    Ok(duration)
}
