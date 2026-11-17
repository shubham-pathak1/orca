//! Unprocessed playback owns a separate integer source and endpoint. The
//! processed Rodio mixer is stopped before entering this loop.
use super::{report_playback_error, AudioCommand, PlaybackState};
use crate::{
    audio_output::{IntegerOutput, OutputConfig, OutputStatus},
    integer_audio::{
        stream::{StreamControl, TrackStart},
        IntegerDecoder, IntegerFormat,
    },
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

struct Player {
    config: OutputConfig,
    output: Option<IntegerOutput>,
    source: Option<Arc<StreamControl>>,
    events: Option<mpsc::Receiver<TrackStart>>,
    format: Option<IntegerFormat>,
    path: Option<String>,
    next: Option<String>,
    position_ms: u64,
    started: Option<Instant>,
    playing: bool,
}
impl Player {
    fn position(&self) -> u64 {
        self.position_ms.saturating_add(
            self.started
                .map(|time| time.elapsed().as_millis() as u64)
                .unwrap_or_default(),
        )
    }
    fn start(
        &mut self,
        path: String,
        position_ms: u64,
        playing: bool,
        state: &Arc<Mutex<PlaybackState>>,
    ) -> Result<(), String> {
        let mut decoder = IntegerDecoder::open(Path::new(&path))?;
        let duration_ms = decoder.duration_ms();
        let position_ms = if duration_ms > 0 {
            position_ms.min(duration_ms)
        } else {
            position_ms
        };
        let frame =
            (position_ms as u128 * decoder.format.rate as u128 / 1000).min(u64::MAX as u128) as u64;
        if frame > 0 {
            decoder.seek_frame(frame)?;
        }
        let format = decoder.format;
        if let Some(output) = self.output.as_ref().filter(|_| self.format == Some(format)) {
            output.discard_pending();
            self.source.as_ref().unwrap().replace(decoder)?;
            output.set_draining(false);
        } else {
            state.lock().unwrap().output.switching = true;
            self.output = None;
            self.source = None;
            self.events = None;
            self.playing = false;
            self.started = None;
            let (source, stream, events) = StreamControl::new(decoder);
            let result = IntegerOutput::open_integer(&self.config, format, stream);
            let (output, mut status) = match result {
                Ok(result) => result,
                Err(error) => {
                    let mut state = state.lock().unwrap();
                    state.is_playing = false;
                    state.output = OutputStatus {
                        config: self.config.clone(),
                        error: error.clone(),
                        revision: state.output.revision,
                        ..Default::default()
                    };
                    return Err(error);
                }
            };
            status.notice = "Use your device's volume control.".into();
            status.revision = state.lock().unwrap().output.revision;
            state.lock().unwrap().output = status;
            self.output = Some(output);
            self.source = Some(source);
            self.events = Some(events);
            self.format = Some(format);
        }
        self.next = None;
        self.path = Some(path.clone());
        self.position_ms = position_ms;
        self.playing = playing;
        self.started = playing.then(Instant::now);
        self.output.as_ref().unwrap().set_active(playing);
        let mut state = state.lock().unwrap();
        state.current_path = Some(path);
        state.position_ms = position_ms;
        state.duration_ms = duration_ms;
        state.is_playing = playing;
        state.volume = 1.0;
        state.error.clear();
        state.output.error.clear();
        Ok(())
    }
    fn queue(&mut self, path: String) -> Result<(), String> {
        let decoder = IntegerDecoder::open(Path::new(&path))?;
        if self.format == Some(decoder.format) {
            self.source
                .as_ref()
                .ok_or("No integer source")?
                .queue(path.clone(), decoder)?;
        }
        self.next = Some(path);
        Ok(())
    }
}

/// Returns a routing command for the normal worker, or None for shutdown.
#[allow(clippy::too_many_arguments)]
pub(super) fn run<F: Fn(&str, u64)>(
    receiver: &mpsc::Receiver<AudioCommand>,
    state: &Arc<Mutex<PlaybackState>>,
    callback: Option<&F>,
    config: OutputConfig,
    first: Option<AudioCommand>,
    replaygain: &Arc<AtomicBool>,
    eq_enabled: &mut bool,
    eq_gains: &mut [f32; 5],
    processed_volume: &mut f32,
) -> Option<AudioCommand> {
    let previous = state.lock().unwrap().clone();
    {
        let mut state = state.lock().unwrap();
        state.output = OutputStatus { config: config.clone(), revision: state.output.revision.wrapping_add(1),
            notice: "Unprocessed output requires a lossless integer WAV, AIFF or FLAC source and a matching exclusive device format.".into(), ..Default::default() };
        state.volume = 1.0;
        state.is_playing = false;
    }
    let mut player = Player {
        config,
        output: None,
        source: None,
        events: None,
        format: None,
        path: previous.current_path.clone(),
        next: None,
        position_ms: previous.position_ms,
        started: None,
        playing: false,
    };
    if let Some(path) = previous.current_path {
        if let Err(error) = player.start(path, previous.position_ms, previous.is_playing, state) {
            state.lock().unwrap().output.error = error.clone();
            report_playback_error(state, "output", &error, true);
        }
    }
    let mut pending = first;
    loop {
        let command = pending.take().or_else(|| {
            if player.playing {
                receiver.recv_timeout(Duration::from_millis(80)).ok()
            } else {
                receiver.recv().ok()
            }
        });
        if let Some(command) = command {
            let result = match command {
                AudioCommand::Shutdown => return None,
                AudioCommand::SetOutput(config) if !config.unprocessed => {
                    return Some(AudioCommand::SetOutput(config))
                }
                AudioCommand::SetOutput(config) => {
                    let position = player.position();
                    let playing = player.playing;
                    player.config = config;
                    player.output = None;
                    {
                        let mut state = state.lock().unwrap();
                        state.output.revision = state.output.revision.wrapping_add(1);
                    }
                    if let Some(path) = player.path.clone() {
                        player.start(path, position, playing, state)
                    } else {
                        Ok(())
                    }
                }
                AudioCommand::DefaultOutputChanged if player.config.device_id.is_empty() => {
                    pending = Some(AudioCommand::SetOutput(player.config.clone()));
                    Ok(())
                }
                AudioCommand::Play(path) | AudioCommand::PlayCrossfade(path, _) => {
                    player.start(path, 0, true, state)
                }
                AudioCommand::LoadPaused(path, position) => {
                    player.start(path, position.as_millis() as u64, false, state)
                }
                AudioCommand::Pause => {
                    player.position_ms = player.position();
                    player.started = None;
                    player.playing = false;
                    if let Some(output) = &player.output {
                        output.set_active(false);
                    }
                    let mut state = state.lock().unwrap();
                    state.position_ms = player.position_ms;
                    state.is_playing = false;
                    Ok(())
                }
                AudioCommand::Resume | AudioCommand::Seek(_) | AudioCommand::ClearQueued => {
                    let position = match command {
                        AudioCommand::Seek(value) => value.as_millis() as u64,
                        _ => player.position(),
                    };
                    let playing = matches!(command, AudioCommand::Resume) || player.playing;
                    let next = if matches!(command, AudioCommand::Resume) {
                        player.next.clone()
                    } else {
                        None
                    };
                    let result = if let Some(path) = player.path.clone() {
                        player.start(path, position, playing, state)
                    } else {
                        Ok(())
                    };
                    if result.is_ok() {
                        if let Some(path) = next {
                            if let Err(error) = player.queue(path) {
                                report_playback_error(state, "queue", &error, false);
                            }
                        }
                    }
                    result
                }
                AudioCommand::QueueNext(path) => player.queue(path),
                AudioCommand::Stop => {
                    player.output = None;
                    player.source = None;
                    player.events = None;
                    player.path = None;
                    player.next = None;
                    player.playing = false;
                    player.started = None;
                    player.position_ms = 0;
                    let mut state = state.lock().unwrap();
                    state.current_path = None;
                    state.position_ms = 0;
                    state.duration_ms = 0;
                    state.is_playing = false;
                    state.output.sample_rate = 0;
                    state.output.notice =
                        "Unprocessed output stopped; the device has been released.".into();
                    Ok(())
                }
                AudioCommand::SetReplayGain(enabled) => {
                    replaygain.store(enabled, Ordering::Relaxed);
                    Ok(())
                }
                AudioCommand::SetEqEnabled(enabled) => {
                    *eq_enabled = enabled;
                    Ok(())
                }
                AudioCommand::SetEqGains(gains) => {
                    *eq_gains = gains;
                    Ok(())
                }
                // Volume preferences remain owned by the processed path. This
                // path never silently scales or clips integer PCM.
                AudioCommand::SetVolume(value) => {
                    if value.is_finite() {
                        *processed_volume = value.clamp(0.0, 1.0);
                    }
                    state.lock().unwrap().volume = 1.0;
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                state.lock().unwrap().output.error = error.clone();
                report_playback_error(state, "unprocessed", &error, !player.playing);
            }
        }
        if let Some(events) = &player.events {
            while let Ok(start) = events.try_recv() {
                if player
                    .source
                    .as_ref()
                    .is_none_or(|source| source.generation() != start.generation)
                {
                    continue;
                }
                player.path = Some(start.path.clone());
                player.next = None;
                player.position_ms = 0;
                player.started = player.playing.then(Instant::now);
                let mut state = state.lock().unwrap();
                state.current_path = Some(start.path);
                state.duration_ms = start.duration_ms;
                state.position_ms = 0;
                drop(state);
                if let Some(callback) = callback {
                    callback("track-transitioned", start.duration_ms);
                }
            }
        }
        if let Some(error) = player
            .source
            .as_ref()
            .and_then(|source| source.take_error())
            .or_else(|| player.output.as_ref().and_then(IntegerOutput::take_error))
        {
            player.playing = false;
            player.started = None;
            player.output = None;
            {
                let mut state = state.lock().unwrap();
                state.output.error = error.clone();
                state.output.sample_rate = 0;
                state.output.channels = 0;
                state.output.sample_format.clear();
            }
            report_playback_error(state, "unprocessed", &error, true);
        }
        if player.playing {
            let ended = player.source.as_ref().is_some_and(|source| source.ended());
            if let Some(output) = &player.output {
                output.set_draining(ended);
            }
            state.lock().unwrap().position_ms = player.position();
            if let Some(callback) = callback {
                callback("playback-progress", player.position());
            }
            if ended
                && !player
                    .output
                    .as_ref()
                    .is_some_and(IntegerOutput::has_pending)
            {
                if let Some(path) = player.next.take() {
                    if let Err(error) = player.start(path, 0, true, state) {
                        player.playing = false;
                        report_playback_error(state, "unprocessed", &error, true);
                    } else if let Some(callback) = callback {
                        callback("track-transitioned", 0);
                    }
                } else {
                    player.position_ms = player.position();
                    player.started = None;
                    player.playing = false;
                    if let Some(output) = &player.output {
                        output.set_active(false);
                    }
                    state.lock().unwrap().is_playing = false;
                    if let Some(callback) = callback {
                        callback("playback-ended", 0);
                    }
                }
            }
        }
    }
}
