use std::fs::File;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use biquad::{Biquad, Coefficients, DirectForm1, ToHertz, Type};
use log::error;
use rodio::{Decoder, OutputStreamBuilder, Sink, Source};
use serde::{Deserialize, Serialize};

const EQ_BAND_FREQUENCIES_HZ: [f32; 5] = [60.0, 230.0, 910.0, 3_600.0, 14_000.0];
const EQ_Q: f32 = 0.707;
const PLAYBACK_POLL_INTERVAL_MS: u64 = 80;
const VISUALIZER_SAMPLES_PER_PEAK: usize = 1024;
const VISUALIZER_BUFFER_SIZE: usize = 120;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaybackState {
    pub current_path: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing: bool,
    pub volume: f32,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            current_path: None,
            position_ms: 0,
            duration_ms: 0,
            is_playing: false,
            volume: 1.0,
        }
    }
}

pub struct VisualizerData {
    pub peaks: Arc<Mutex<std::collections::VecDeque<f32>>>,
}

impl Default for VisualizerData {
    fn default() -> Self {
        Self {
            peaks: Arc::new(Mutex::new(std::collections::VecDeque::from(vec![0.0; VISUALIZER_BUFFER_SIZE]))),
        }
    }
}

pub enum AudioCommand {
    Play(String), // Sudden play (clears both)
    PlayCrossfade(String, Duration), // Smooth transition
    Pause,
    Resume,
    Seek(Duration),
    Stop,
    SetVolume(f32),
    SetEqEnabled(bool),
    SetEqGains([f32; 5]),
    QueueNext(String),
    UpdateMetadata(String, u64),
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
    fn current_span_len(&self) -> Option<usize> { self.inner.current_span_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}

#[allow(dead_code)]
struct ActiveSink {
    sink: Sink,
    path: Option<String>,
    duration_ms: u64,
    start_time: Instant,
    position_base_ms: u64,
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
    fn current_span_len(&self) -> Option<usize> { self.inner.current_span_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
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
}

fn create_track_source(
    path: &str,
    start_pos: Duration,
    eq_enabled: bool,
    eq_gains: [f32; 5],
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
    on_start: Option<Box<dyn FnOnce() + Send>>,
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
        Ok((total_duration_ms, Box::new(TransitionSource::new(eq_source, cb))))
    } else {
        Ok((total_duration_ms, eq_source))
    }
}

fn load_track_into_sink(
    sink: &Sink,
    path: &str,
    start_pos: Duration,
    eq_enabled: bool,
    eq_gains: [f32; 5],
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
) -> Result<u64, String> {
    let (duration, source) = create_track_source(path, start_pos, eq_enabled, eq_gains, visualizer, None)?;
    sink.clear();
    sink.append(source);
    Ok(duration)
}

fn restart_current_with_eq(
    sink: &Sink,
    state: &Arc<Mutex<PlaybackState>>,
    playing_flag: &mut bool,
    position_base_ms: &mut u64,
    eq_enabled: bool,
    eq_gains: [f32; 5],
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
) {
    let snapshot = state
        .lock()
        .ok()
        .map(|s| (s.current_path.clone(), s.position_ms, s.is_playing));

    let Some((Some(path), position_ms, was_playing)) = snapshot else {
        return;
    };

    match load_track_into_sink(
        sink,
        &path,
        Duration::from_millis(position_ms),
        eq_enabled,
        eq_gains,
        visualizer,
    ) {
        Ok(duration_ms) => {
            *position_base_ms = position_ms;
            if was_playing {
                sink.play();
                *playing_flag = true;
            } else {
                sink.pause();
                *playing_flag = false;
            }

            if let Ok(mut s) = state.lock() {
                s.current_path = Some(path);
                s.position_ms = position_ms;
                s.duration_ms = duration_ms;
                s.is_playing = was_playing;
            }
        }
        Err(err) => error!("Audio Engine: Failed to reload track with updated EQ: {err}"),
    }
}

pub fn spawn_audio_thread<F>(
    event_callback: Option<F>,
) -> (mpsc::Sender<AudioCommand>, Arc<Mutex<PlaybackState>>, VisualizerData)
where
    F: Fn(&str, u64) + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<AudioCommand>();
    let state = Arc::new(Mutex::new(PlaybackState::default()));
    let visualizer = VisualizerData::default();
    let thread_vis = Arc::clone(&visualizer.peaks);
    let thread_state = Arc::clone(&state);
    let thread_tx = tx.clone();

    thread::spawn(move || {
        let stream = match OutputStreamBuilder::open_default_stream() {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to open default audio stream: {}", e);
                return;
            }
        };

        let mixer = stream.mixer();
        let sink_a = Sink::connect_new(&mixer);
        let sink_b = Sink::connect_new(&mixer);
        
        // We'll use these to track which sink is doing what
        let mut primary = sink_a;
        let mut secondary = sink_b;
        
        let mut current_path: Option<String> = None;
        let mut position_base_ms: u64 = 0;
        
        let mut fading_out_sink: Option<Sink> = None;
        let mut fade_start: Option<Instant> = None;
        let mut fade_duration: Duration = Duration::from_secs(0);

        let mut playing = false;
        let mut eq_enabled = false;
        let mut eq_gains = [0.0f32; 5];
        let mut global_volume = 1.0f32;

        loop {
            let msg = if playing {
                rx.recv_timeout(Duration::from_millis(PLAYBACK_POLL_INTERVAL_MS)).ok()
            } else {
                rx.recv().ok()
            };

            if let Some(cmd) = msg {
                match cmd {
                    AudioCommand::Play(path) => {
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;
                        
                        match load_track_into_sink(&primary, &path, Duration::from_millis(0), eq_enabled, eq_gains, &thread_vis) {
                            Ok(d) => {
                                primary.set_volume(global_volume);
                                primary.play();
                                playing = true;
                                position_base_ms = 0;
                                current_path = Some(path.clone());
                                
                                if let Ok(mut s) = thread_state.lock() {
                                    s.current_path = Some(path);
                                    s.is_playing = true;
                                    s.position_ms = 0;
                                    s.duration_ms = d;
                                }
                            }
                            Err(e) => error!("Audio Engine: Play error: {e}"),
                        }
                    }
                    AudioCommand::PlayCrossfade(path, cross_dur) => {
                        // If nothing is playing, just play normally
                        if !playing || primary.empty() {
                            let _ = thread_tx.send(AudioCommand::Play(path));
                        } else {
                            // Start secondary sink
                            match load_track_into_sink(&secondary, &path, Duration::from_millis(0), eq_enabled, eq_gains, &thread_vis) {
                                Ok(d) => {
                                    fading_out_sink = Some(primary);
                                    primary = secondary;
                                    secondary = Sink::connect_new(&mixer); // New back pocket sink
                                    
                                    primary.set_volume(0.0);
                                    primary.play();
                                    
                                    fade_start = Some(Instant::now());
                                    fade_duration = cross_dur;
                                    
                                    playing = true;
                                    position_base_ms = 0;
                                    current_path = Some(path.clone());

                                    if let Ok(mut s) = thread_state.lock() {
                                        s.current_path = Some(path);
                                        s.is_playing = true;
                                        s.position_ms = 0;
                                        s.duration_ms = d;
                                    }
                                }
                                Err(e) => error!("Audio Engine: Crossfade load error: {e}"),
                            }
                        }
                    }
                    AudioCommand::Pause => {
                        primary.pause();
                        playing = false;
                        if let Ok(mut s) = thread_state.lock() { s.is_playing = false; }
                    }
                    AudioCommand::Resume => {
                        primary.play();
                        playing = true;
                        if let Ok(mut s) = thread_state.lock() { s.is_playing = true; }
                    }
                    AudioCommand::Seek(pos) => {
                        match load_track_into_sink(&primary, current_path.as_ref().unwrap_or(&"".to_string()), pos, eq_enabled, eq_gains, &thread_vis) {
                            Ok(d) => {
                                position_base_ms = pos.as_millis() as u64;
                                primary.play();
                                playing = true;
                                if let Ok(mut s) = thread_state.lock() {
                                    s.position_ms = position_base_ms;
                                    s.duration_ms = d;
                                    s.is_playing = true;
                                }
                            }
                            Err(e) => error!("Audio Engine: Seek error: {e}"),
                        }
                    }
                    AudioCommand::SetVolume(vol) => {
                        global_volume = vol;
                        primary.set_volume(vol);
                        if let Ok(mut s) = thread_state.lock() { s.volume = vol; }
                    }
                    AudioCommand::SetEqEnabled(enabled) => {
                        eq_enabled = enabled;
                        restart_current_with_eq(&primary, &thread_state, &mut playing, &mut position_base_ms, eq_enabled, eq_gains, &thread_vis);
                    }
                    AudioCommand::SetEqGains(gains) => {
                        eq_gains = gains;
                        if eq_enabled {
                            restart_current_with_eq(&primary, &thread_state, &mut playing, &mut position_base_ms, eq_enabled, eq_gains, &thread_vis);
                        }
                    }
                    AudioCommand::Stop => {
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        playing = false;
                        if let Ok(mut s) = thread_state.lock() {
                            s.current_path = None;
                            s.is_playing = false;
                        }
                    }
                    AudioCommand::QueueNext(path) => {
                        let thread_tx_inner = thread_tx.clone();
                        let path_inner = path.clone();
                        
                        // We need the duration before creating the source
                        match create_track_source(&path, Duration::from_millis(0), eq_enabled, eq_gains, &thread_vis, None) {
                            Ok((duration, _)) => {
                                let on_start = Box::new(move || {
                                    let _ = thread_tx_inner.send(AudioCommand::UpdateMetadata(path_inner, duration));
                                });
                                // Re-create with callback
                                if let Ok((_, source)) = create_track_source(&path, Duration::from_millis(0), eq_enabled, eq_gains, &thread_vis, Some(on_start)) {
                                    primary.append(source);
                                }
                            }
                            Err(e) => error!("Audio Engine: QueueNext error: {e}"),
                        }
                    }
                    AudioCommand::UpdateMetadata(path, d) => {
                        current_path = Some(path.clone());
                        position_base_ms = 0; // Reset position base for the new track in the sink
                        if let Ok(mut s) = thread_state.lock() {
                            s.current_path = Some(path.clone());
                            s.duration_ms = d;
                            s.position_ms = 0;
                            s.is_playing = true;
                        }
                        if let Some(ref cb) = event_callback {
                            cb("track-transitioned", 0);
                        }
                    }
                }
            }

            // Handle active fades
            if let (Some(start), Some(old_sink)) = (fade_start, fading_out_sink.as_ref()) {
                let elapsed: Duration = start.elapsed();
                if elapsed >= fade_duration {
                    old_sink.stop();
                    fading_out_sink = None;
                    fade_start = None;
                    primary.set_volume(global_volume);
                } else {
                    let progress = elapsed.as_secs_f32() / fade_duration.as_secs_f32();
                    old_sink.set_volume(global_volume * (1.0 - progress));
                    primary.set_volume(global_volume * progress);
                }
            }

            if playing && !primary.empty() {
                let pos_ms = position_base_ms.saturating_add(primary.get_pos().as_millis() as u64);
                if let Ok(mut s) = thread_state.lock() {
                    s.position_ms = pos_ms;
                }
                if let Some(ref cb) = event_callback {
                    cb("playback-progress", pos_ms);
                }
            }

            if playing && primary.empty() && fading_out_sink.is_none() {
                playing = false;
                if let Ok(mut s) = thread_state.lock() { s.is_playing = false; }
                if let Some(ref cb) = event_callback {
                    cb("playback-ended", 0);
                }
            }
        }
    });

    (tx, state, visualizer)
}
