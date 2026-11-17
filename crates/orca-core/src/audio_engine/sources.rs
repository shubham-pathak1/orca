use super::output_source::PlaybackSink as Sink;
use super::VISUALIZER_BUFFER_SIZE;
use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type};
use lofty::{config::ParseOptions, file::TaggedFileExt, probe::Probe, tag::ItemKey};
use rodio::{Decoder, Source};
use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
const EQ_BAND_FREQUENCIES_HZ: [f32; 5] = [60.0, 230.0, 910.0, 3600.0, 14000.0];
const EQ_Q: f32 = 0.707;
const VISUALIZER_SAMPLES_PER_PEAK: usize = 1024;

pub(super) fn track_sample_rate(path: &str) -> Result<u32, String> {
    let decoder = Decoder::try_from(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(decoder.sample_rate())
}

fn parse_gain_db(value: &str) -> Option<f32> {
    let value = value
        .trim()
        .trim_end_matches(['B', 'b'])
        .trim_end_matches(['D', 'd'])
        .trim();
    value
        .parse::<f32>()
        .ok()
        .filter(|gain| gain.is_finite() && (-40.0..=40.0).contains(gain))
}

fn replaygain_multiplier(gain: Option<&str>, peak: Option<&str>) -> f32 {
    let Some(db) = gain.and_then(parse_gain_db) else {
        return 1.0;
    };
    let mut multiplier = 10.0_f32.powf(db / 20.0);
    if let Some(peak) = peak
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|peak| peak.is_finite() && *peak > 0.0)
    {
        multiplier = multiplier.min(1.0 / peak);
    } else {
        // A positive gain without a measured peak could clip. Apply attenuation,
        // but do not boost an unmeasured track.
        multiplier = multiplier.min(1.0);
    }
    multiplier
}

fn track_replaygain(path: &str) -> f32 {
    let Ok(file) = Probe::open(path).and_then(|probe| {
        probe
            .options(
                ParseOptions::new()
                    .read_properties(false)
                    .read_cover_art(false),
            )
            .read()
    }) else {
        return 1.0;
    };
    for tag in file.tags() {
        let track = tag.get_string(&ItemKey::ReplayGainTrackGain);
        if track.is_some() {
            return replaygain_multiplier(track, tag.get_string(&ItemKey::ReplayGainTrackPeak));
        }
    }
    1.0
}

struct ReplayGainSource<S: Source<Item = f32>> {
    inner: S,
    multiplier: f32,
    enabled: Arc<AtomicBool>,
}

impl<S: Source<Item = f32>> Iterator for ReplayGainSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next().map(|sample| {
            if self.enabled.load(Ordering::Relaxed) && self.multiplier != 1.0 {
                (sample * self.multiplier).clamp(-1.0, 1.0)
            } else {
                sample
            }
        })
    }
}

impl<S: Source<Item = f32>> Source for ReplayGainSource<S> {
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
    replaygain: &Arc<AtomicBool>,
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
    on_start: Option<Box<dyn FnOnce(u64) + Send>>,
) -> Result<(u64, Box<dyn Source<Item = f32> + Send>), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open '{path}': {e}"))?;
    let mut decoder =
        Decoder::try_from(file).map_err(|e| format!("Decode error for '{path}': {e}"))?;
    let total_duration_ms = decoder
        .total_duration()
        .unwrap_or_else(|| Duration::from_secs(0))
        .as_millis() as u64;

    if !start_pos.is_zero() && decoder.try_seek(start_pos).is_err() {
        // A failed seek may have moved the decoder. Restart before falling back
        // to a frame-aligned skip; sample-wise duration rounding can swap stereo
        // channels when the requested time falls between frames.
        decoder = Decoder::try_from(File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let frames = start_pos.as_nanos() * decoder.sample_rate() as u128 / 1_000_000_000;
        let samples = (frames * decoder.channels() as u128).min(usize::MAX as u128) as usize;
        if samples > 0 {
            let _ = decoder.nth(samples - 1);
        }
    }
    let source = decoder;
    let gain_source = ReplayGainSource {
        inner: source,
        multiplier: track_replaygain(path),
        enabled: Arc::clone(replaygain),
    };
    let vis_source = VisualizerSource::new(gain_source, Arc::clone(visualizer));

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
    replaygain: &Arc<AtomicBool>,
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
) -> Result<u64, String> {
    let (duration, source) = create_track_source(
        path, start_pos, eq_enabled, eq_gains, replaygain, visualizer, None,
    )?;
    sink.clear();
    sink.append(source)?;
    Ok(duration)
}

#[cfg(test)]
mod replaygain_tests {
    use super::*;
    use lofty::{
        config::WriteOptions,
        prelude::*,
        tag::{Tag, TagType},
    };
    use rodio::buffer::SamplesBuffer;

    #[test]
    fn tag_gain_is_bounded_by_peak_and_invalid_values_are_ignored() {
        assert!((replaygain_multiplier(Some("-6 dB"), None) - 0.5011872).abs() < 0.0001);
        assert!((replaygain_multiplier(Some("+6.0 dB"), Some("0.8")) - 1.25).abs() < 0.0001);
        for value in ["NaN", "infinity", "400 dB", "junk"] {
            assert_eq!(replaygain_multiplier(Some(value), None), 1.0);
        }
        assert_eq!(replaygain_multiplier(None, Some("0.5")), 1.0);
        assert_eq!(replaygain_multiplier(Some("+6 dB"), None), 1.0);
    }

    #[test]
    fn toggle_changes_output_without_replacing_the_playing_source() {
        let enabled = Arc::new(AtomicBool::new(false));
        let source = SamplesBuffer::new(1, 44_100, vec![0.5, 0.5, 0.5]);
        let mut gain = ReplayGainSource {
            inner: source,
            multiplier: 0.5,
            enabled: Arc::clone(&enabled),
        };
        assert_eq!(gain.next(), Some(0.5));
        enabled.store(true, Ordering::Relaxed);
        assert_eq!(gain.next(), Some(0.25));
        enabled.store(false, Ordering::Relaxed);
        assert_eq!(gain.next(), Some(0.5));
    }

    #[test]
    fn disposable_tagged_audio_changes_decoded_playback_and_untagged_audio_does_not() {
        let file = std::env::temp_dir().join(format!(
            "orca-replaygain-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let samples = [16_384_i16; 800];
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + samples.len() as u32 * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples.len() as u32 * 2).to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&file, wav).unwrap();

        let enabled = Arc::new(AtomicBool::new(true));
        let visualizer = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let path = file.to_str().unwrap();
        let first_sample = || {
            let (_, mut source) = create_track_source(
                path,
                Duration::ZERO,
                false,
                [0.0; 5],
                &enabled,
                &visualizer,
                None,
            )
            .unwrap();
            source.next().unwrap()
        };
        let untagged = first_sample();
        assert!((untagged - 0.5).abs() < 0.001);

        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::ReplayGainTrackGain, "-6.00 dB".into());
        tag.insert_text(ItemKey::ReplayGainTrackPeak, "0.5".into());
        tag.save_to_path(&file, WriteOptions::default()).unwrap();
        let tagged = first_sample();
        assert!((tagged - untagged * 0.5011872).abs() < 0.001);
        std::fs::remove_file(file).unwrap();
    }
}
