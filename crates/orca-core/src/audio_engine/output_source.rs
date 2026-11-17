//! Normalize each track before it enters Rodio's persistent queue. Its empty
//! queue advertises mono/44.1k; exposing that to a mixer can corrupt transitions.
use rodio::{mixer::Mixer, source::UniformSourceIterator, Sink, Source};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::{ops::Deref, time::Duration};

#[derive(Clone, Copy)]
pub(super) struct OutputFormat {
    pub channels: u16,
    pub rate: u32,
}

pub(super) struct PlaybackSink {
    inner: Sink,
    format: OutputFormat,
}
impl PlaybackSink {
    pub fn connect_new(mixer: &Mixer, format: OutputFormat) -> Self {
        let (inner, source) = Sink::new();
        mixer.add(FixedFormat {
            inner: source,
            format,
        });
        Self { inner, format }
    }
    pub fn append(&self, source: Box<dyn Source<Item = f32> + Send>) -> Result<(), String> {
        let source = normalize(source, self.format)?;
        self.inner.append(source);
        Ok(())
    }
}
impl Deref for PlaybackSink {
    type Target = Sink;
    fn deref(&self) -> &Sink {
        &self.inner
    }
}

struct FixedFormat<S: Source> {
    inner: S,
    format: OutputFormat,
}
impl<S: Source> Iterator for FixedFormat<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next()
    }
}
impl<S: Source> Source for FixedFormat<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.format.channels
    }
    fn sample_rate(&self) -> u32 {
        self.format.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)
    }
}

pub(super) fn normalize(
    source: Box<dyn Source<Item = f32> + Send>,
    format: OutputFormat,
) -> Result<Box<dyn Source<Item = f32> + Send>, String> {
    if !(1..=32).contains(&source.channels())
        || !(8000..=768000).contains(&source.sample_rate())
        || !(1..=32).contains(&format.channels)
        || !(8000..=768000).contains(&format.rate)
    {
        return Err("Unsupported audio channel count or sample rate".into());
    }
    let source: Box<dyn Source<Item = f32> + Send> = if source.sample_rate() == format.rate {
        source
    } else {
        Box::new(SincSource::new(source, format.rate)?)
    };
    // Rates already match here. Rodio only needs to perform channel mapping,
    // with a stable per-track format rather than its empty queue's defaults.
    Ok(Box::new(UniformSourceIterator::new(
        source,
        format.channels,
        format.rate,
    )))
}

struct SincSource {
    inner: Box<dyn Source<Item = f32> + Send>,
    resampler: SincFixedIn<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    channels: usize,
    rate_in: u32,
    rate_out: u32,
    input_frames: u64,
    emitted_frames: u64,
    target_frames: Option<u64>,
    skip_frames: usize,
    cursor: usize,
    available: usize,
    eof: bool,
}
impl SincSource {
    fn new(inner: Box<dyn Source<Item = f32> + Send>, rate_out: u32) -> Result<Self, String> {
        let channels = inner.channels() as usize;
        let rate_in = inner.sample_rate();
        // Keep the transition band narrow in output-frequency terms when
        // downsampling high-rate files. Bound the table size for extreme ratios.
        let filter_scale = rate_in.div_ceil(rate_out).clamp(1, 8) as usize;
        let resampler = SincFixedIn::new(
            rate_out as f64 / rate_in as f64,
            1.0,
            SincInterpolationParameters {
                sinc_len: 256 * filter_scale,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            },
            1024,
            channels,
        )
        .map_err(|e| format!("Cannot prepare sample-rate conversion: {e}"))?;
        let input = resampler.input_buffer_allocate(true);
        let output = resampler.output_buffer_allocate(true);
        let skip_frames = resampler.output_delay();
        Ok(Self {
            inner,
            resampler,
            input,
            output,
            channels,
            rate_in,
            rate_out,
            input_frames: 0,
            emitted_frames: 0,
            target_frames: None,
            skip_frames,
            cursor: 0,
            available: 0,
            eof: false,
        })
    }
    fn refill(&mut self) -> bool {
        while self.cursor >= self.available {
            if self.target_frames.is_some_and(|n| self.emitted_frames >= n) {
                return false;
            }
            let needed = self.resampler.input_frames_next();
            for channel in &mut self.input {
                channel[..needed].fill(0.0);
            }
            let mut read = 0;
            if !self.eof {
                for frame in 0..needed {
                    let Some(first) = self.inner.next() else {
                        self.eof = true;
                        break;
                    };
                    self.input[0][frame] = first;
                    for channel in 1..self.channels {
                        self.input[channel][frame] = self.inner.next().unwrap_or(0.0);
                    }
                    read += 1;
                }
                self.input_frames += read;
                if self.eof {
                    self.target_frames = Some(
                        (self.input_frames * self.rate_out as u64 + self.rate_in as u64 / 2)
                            / self.rate_in as u64,
                    );
                }
                if self.target_frames == Some(0) {
                    return false;
                }
            }
            let Ok((_, frames)) =
                self.resampler
                    .process_into_buffer(&self.input, &mut self.output, None)
            else {
                log::error!("Audio resampler rejected its prepared buffers");
                self.target_frames = Some(self.emitted_frames);
                return false;
            };
            let skip = self.skip_frames.min(frames);
            self.skip_frames -= skip;
            self.cursor = skip * self.channels;
            self.available = frames * self.channels;
        }
        true
    }
}
impl Iterator for SincSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.target_frames.is_some_and(|n| self.emitted_frames >= n) || !self.refill() {
            return None;
        }
        let value = self.output[self.cursor % self.channels][self.cursor / self.channels];
        self.cursor += 1;
        if self.cursor.is_multiple_of(self.channels) {
            self.emitted_frames += 1;
        }
        Some(value)
    }
}
impl Source for SincSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels as u16
    }
    fn sample_rate(&self) -> u32 {
        self.rate_out
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(position)?;
        self.resampler.reset();
        self.input_frames = 0;
        self.emitted_frames = 0;
        self.target_frames = None;
        self.skip_frames = self.resampler.output_delay();
        self.cursor = 0;
        self.available = 0;
        self.eof = false;
        Ok(())
    }
}
