use super::output_source::{OutputFormat, PlaybackSink as Sink};
use super::sources::{create_track_source, load_track_into_sink, track_sample_rate};
use super::{report_playback_error, AudioCommand, PlaybackState, VisualizerData};
use crate::audio_output::{OutputConfig, OutputHandle};
use log::error;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
const PLAYBACK_POLL_INTERVAL_MS: u64 = 80;

fn open_output(
    config: &OutputConfig,
    rate: Option<u32>,
) -> Result<(OutputHandle, crate::audio_output::OutputStatus), String> {
    let (handle, mut status) = match OutputHandle::open_for_rate(config, rate) {
        Ok(result) => Ok(result),
        Err(error) => OutputHandle::open(&OutputConfig::default())
            .map(|(handle, mut status)| {
                status.error = format!(
                    "Requested output unavailable: {error}. Using system default shared output."
                );
                (handle, status)
            })
            .map_err(|fallback| format!("{error}. Shared fallback also failed: {fallback}")),
    }?;
    status.source_sample_rate = rate.unwrap_or_default();
    Ok((handle, status))
}

fn restart_current_with_eq(
    sink: &Sink,
    state: &Arc<Mutex<PlaybackState>>,
    playing_flag: &mut bool,
    position_base_ms: &mut u64,
    eq: (bool, [f32; 5]),
    replaygain: &Arc<AtomicBool>,
    visualizer: &Arc<Mutex<std::collections::VecDeque<f32>>>,
) {
    let (eq_enabled, eq_gains) = eq;
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
        replaygain,
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
) -> (
    mpsc::Sender<AudioCommand>,
    Arc<Mutex<PlaybackState>>,
    VisualizerData,
)
where
    F: for<'a> Fn(&'a str, u64) + Send + 'static,
{
    let (tx, state, visualizer, _handle) = spawn_audio_thread_managed(event_callback);
    (tx, state, visualizer)
}

/// Native hosts retain the handle and send `Shutdown` before joining it.
pub fn spawn_audio_thread_managed<F>(
    event_callback: Option<F>,
) -> (
    mpsc::Sender<AudioCommand>,
    Arc<Mutex<PlaybackState>>,
    VisualizerData,
    thread::JoinHandle<()>,
)
where
    F: for<'a> Fn(&'a str, u64) + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<AudioCommand>();
    let state = Arc::new(Mutex::new(PlaybackState::default()));
    let visualizer = VisualizerData::default();
    let thread_vis = Arc::clone(&visualizer.peaks);
    let replaygain = Arc::new(AtomicBool::new(false));
    let thread_state = Arc::clone(&state);
    let thread_tx = tx.clone();

    let handle = thread::spawn(move || {
        #[cfg(target_os = "windows")]
        let _default_notifications = {
            let notify_tx = thread_tx.clone();
            crate::audio_output::DefaultOutputNotifications::start(move || {
                let _ = notify_tx.send(AudioCommand::DefaultOutputChanged);
            })
            .map_err(|error| error!("Default output notifications unavailable: {error}"))
            .ok()
        };
        let initial = OutputHandle::open(&OutputConfig::default());
        let mut output = match initial {
            Ok((handle, status)) => {
                thread_state.lock().unwrap().output = status;
                Some(handle)
            }
            Err(error) => {
                thread_state.lock().unwrap().output.error = error;
                None
            }
        };
        let mut mixer = output
            .as_ref()
            .map(|handle| handle.mixer().clone())
            .unwrap_or_else(|| rodio::mixer::mixer(2, 48000).0);
        let mut output_format = {
            let state = thread_state.lock().unwrap();
            OutputFormat {
                channels: state.output.channels.max(1),
                rate: if state.output.sample_rate == 0 {
                    48000
                } else {
                    state.output.sample_rate
                },
            }
        };
        let mut primary = Sink::connect_new(&mixer, output_format);
        let mut secondary = Sink::connect_new(&mixer, output_format);

        let mut current_path: Option<String> = None;
        let mut source_generation = 0u64;
        let mut position_base_ms: u64 = 0;
        let mut track_started_at: Option<Instant> = None;

        let mut fading_out_sink: Option<Sink> = None;
        let mut fade_start: Option<Instant> = None;
        let mut fade_duration: Duration = Duration::from_secs(0);

        let mut playing = false;
        let mut eq_enabled = false;
        let mut eq_gains = [0.0f32; 5];
        let mut global_volume = 1.0f32;
        let mut requested_output = OutputConfig::default();
        let mut pending_command = None;
        let mut recovery_notice: Option<String> = None;
        let mut negotiated_rate = None;
        let mut deferred_next: Option<String> = None;

        loop {
            let msg = if pending_command.is_some() {
                pending_command.take()
            } else if playing {
                rx.recv_timeout(Duration::from_millis(PLAYBACK_POLL_INTERVAL_MS))
                    .ok()
            } else {
                rx.recv().ok()
            };

            if let Some(mut cmd) = msg {
                let queued_transition = matches!(&cmd, AudioCommand::PlayQueued(_, _));
                if let AudioCommand::PlayQueued(path, generation) = cmd {
                    if generation != source_generation {
                        continue;
                    }
                    cmd = AudioCommand::Play(path);
                }
                // Collapse only consecutive output requests. Playback/seek/shutdown
                // retain their order and are never discarded during rapid changes.
                if matches!(cmd, AudioCommand::SetOutput(_)) {
                    while let Ok(next) = rx.try_recv() {
                        if matches!(next, AudioCommand::SetOutput(_)) {
                            cmd = next;
                        } else {
                            pending_command = Some(next);
                            break;
                        }
                    }
                }
                let force_default = matches!(&cmd, AudioCommand::DefaultOutputChanged);
                if force_default && !requested_output.device_id.is_empty() {
                    continue;
                }
                let cmd = if force_default {
                    AudioCommand::SetOutput(requested_output.clone())
                } else {
                    cmd
                };
                #[cfg(windows)]
                if let AudioCommand::SetOutput(config) = &cmd {
                    if config.unprocessed {
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;
                        deferred_next = None;
                        source_generation = source_generation.wrapping_add(1);
                        drop(output.take());
                        let next = super::integer_playback::run(
                            &rx,
                            &thread_state,
                            event_callback.as_ref(),
                            config.clone(),
                            pending_command.take(),
                            &replaygain,
                            &mut eq_enabled,
                            &mut eq_gains,
                            &mut global_volume,
                        );
                        let Some(next) = next else {
                            break;
                        };
                        let mut state = thread_state.lock().unwrap();
                        current_path = state.current_path.clone();
                        position_base_ms = state.position_ms;
                        playing = state.is_playing;
                        state.volume = global_volume;
                        track_started_at = playing.then(Instant::now);
                        pending_command = Some(next);
                        continue;
                    }
                }
                let incoming_path = match &cmd {
                    AudioCommand::Play(path)
                    | AudioCommand::LoadPaused(path, _)
                    | AudioCommand::PlayCrossfade(path, _) => Some(path),
                    _ => None,
                };
                if let Some(path) = incoming_path {
                    let rate = match track_sample_rate(path) {
                        Ok(rate) => rate,
                        Err(error) => {
                            report_playback_error(&thread_state, "play", &error, queued_transition);
                            continue;
                        }
                    };
                    if requested_output.exclusive
                        && (output.is_none() || negotiated_rate != Some(rate))
                    {
                        thread_state.lock().unwrap().output.switching = true;
                        source_generation = source_generation.wrapping_add(1);
                        deferred_next = None;
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;
                        playing = false;
                        track_started_at = None;
                        drop(output.take());
                        match open_output(&requested_output, Some(rate)) {
                            Ok((handle, mut status)) => {
                                let mut state = thread_state.lock().unwrap();
                                // A track-driven format change preserves the service's
                                // planned queue transition; only routing changes cancel it.
                                status.revision = state.output.revision;
                                mixer = handle.mixer().clone();
                                output_format = OutputFormat {
                                    channels: status.channels,
                                    rate: status.sample_rate,
                                };
                                primary = Sink::connect_new(&mixer, output_format);
                                secondary = Sink::connect_new(&mixer, output_format);
                                output = Some(handle);
                                state.output = status;
                                negotiated_rate = Some(rate);
                            }
                            Err(error) => {
                                let mut state = thread_state.lock().unwrap();
                                state.output.error = error;
                                state.output.switching = false;
                                state.output.sample_rate = 0;
                                state.output.channels = 0;
                                state.output.sample_format.clear();
                                state.is_playing = false;
                                continue;
                            }
                        }
                    }
                    thread_state.lock().unwrap().output.source_sample_rate = rate;
                }
                let buffered = output.as_ref().is_some_and(OutputHandle::buffered);
                if output.is_none()
                    && matches!(
                        &cmd,
                        AudioCommand::Seek(_)
                            | AudioCommand::LoadPaused(_, _)
                            | AudioCommand::ClearQueued
                            | AudioCommand::SetEqEnabled(_)
                            | AudioCommand::SetEqGains(_)
                    )
                {
                    report_playback_error(
                        &thread_state,
                        "output",
                        "No audio output available",
                        false,
                    );
                    continue;
                }
                let cmd = match cmd {
                    AudioCommand::PlayCrossfade(path, _)
                        if buffered || requested_output.exclusive =>
                    {
                        AudioCommand::Play(path)
                    }
                    cmd => cmd,
                };
                // Buffered output must never wait for a hardware callback to
                // acknowledge source controls. Replace its stopped sink instead.
                if matches!(
                    &cmd,
                    AudioCommand::Play(_)
                        | AudioCommand::LoadPaused(_, _)
                        | AudioCommand::PlayCrossfade(_, _)
                        | AudioCommand::Seek(_)
                        | AudioCommand::ClearQueued
                        | AudioCommand::SetEqEnabled(_)
                        | AudioCommand::SetEqGains(_)
                ) {
                    deferred_next = None;
                    if let Some(handle) = output.as_ref() {
                        if buffered {
                            source_generation = source_generation.wrapping_add(1);
                            handle.discard_pending();
                            primary.stop();
                            primary = Sink::connect_new(&mixer, output_format);
                            primary.set_volume(global_volume);
                        } else {
                            handle.set_active(true);
                        }
                    }
                }
                match cmd {
                    AudioCommand::PlayQueued(_, _) => {
                        unreachable!("queued starts are normalized above")
                    }
                    AudioCommand::DefaultOutputChanged => {} // Normalized above.
                    AudioCommand::Shutdown => break,
                    AudioCommand::SetOutput(config) => {
                        requested_output = config.clone();
                        let revision = {
                            let mut state = thread_state.lock().unwrap();
                            state.output.revision = state.output.revision.wrapping_add(1);
                            state.output.revision
                        };
                        if !force_default
                            && output.is_some()
                            && thread_state.lock().unwrap().output.config == config
                        {
                            thread_state.lock().unwrap().output.error.clear();
                            continue;
                        }
                        deferred_next = None;
                        let was_playing = playing;
                        let resume_position = position_base_ms.saturating_add(
                            track_started_at
                                .map(|start| start.elapsed().as_millis() as u64)
                                .unwrap_or(0),
                        );
                        thread_state.lock().unwrap().output.switching = true;
                        source_generation = source_generation.wrapping_add(1);
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;

                        drop(output.take());

                        let preferred = current_path
                            .as_deref()
                            .and_then(|path| track_sample_rate(path).ok());
                        let opened = open_output(&config, preferred);
                        match opened {
                            Ok((handle, mut status)) => {
                                if let Some(error) = recovery_notice.take() {
                                    if status.error.is_empty() {
                                        status.error = format!("Previous output failed: {error}. Using system default shared output.");
                                    }
                                }
                                status.revision = revision;

                                mixer = handle.mixer().clone();
                                output_format = OutputFormat {
                                    channels: status.channels,
                                    rate: status.sample_rate,
                                };
                                primary = Sink::connect_new(&mixer, output_format);
                                secondary = Sink::connect_new(&mixer, output_format);
                                output = Some(handle);
                                negotiated_rate = preferred;
                                thread_state.lock().unwrap().output = status;
                                playing = false;
                                if let Some(path) = current_path.as_deref() {
                                    match load_track_into_sink(
                                        &primary,
                                        path,
                                        Duration::from_millis(resume_position),
                                        eq_enabled,
                                        eq_gains,
                                        &replaygain,
                                        &thread_vis,
                                    ) {
                                        Ok(duration) => {
                                            primary.set_volume(global_volume);
                                            playing = was_playing;
                                            if playing {
                                                primary.play();
                                            } else {
                                                primary.pause();
                                            }
                                            position_base_ms = resume_position;
                                            let mut state = thread_state.lock().unwrap();
                                            state.duration_ms = duration;
                                            state.position_ms = resume_position;
                                        }
                                        Err(error) => {
                                            thread_state.lock().unwrap().output.error = error
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                playing = false;
                                let mut state = thread_state.lock().unwrap();
                                state.output.error = error;
                                state.output.switching = false;
                                state.output.sample_rate = 0;
                                state.output.channels = 0;
                                state.output.sample_format.clear();
                            }
                        }

                        track_started_at = playing.then(Instant::now);
                        thread_state.lock().unwrap().is_playing = playing;
                    }

                    AudioCommand::Play(path) => {
                        thread_state.lock().unwrap().error.clear();
                        if output.is_none() {
                            continue;
                        }
                        source_generation = source_generation.wrapping_add(1);
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;

                        match load_track_into_sink(
                            &primary,
                            &path,
                            Duration::from_millis(0),
                            eq_enabled,
                            eq_gains,
                            &replaygain,
                            &thread_vis,
                        ) {
                            Ok(d) => {
                                primary.set_volume(global_volume);
                                primary.play();
                                playing = true;
                                position_base_ms = 0;
                                track_started_at = Some(Instant::now());
                                current_path = Some(path.clone());

                                if let Ok(mut s) = thread_state.lock() {
                                    s.current_path = Some(path);
                                    s.is_playing = true;
                                    s.position_ms = 0;
                                    s.duration_ms = d;
                                }
                                if queued_transition {
                                    if let Some(ref cb) = event_callback {
                                        cb("track-transitioned", d);
                                    }
                                }
                            }
                            Err(e) => {
                                playing = false;
                                track_started_at = None;
                                report_playback_error(&thread_state, "play", &e.to_string(), true);
                            }
                        }
                    }
                    AudioCommand::LoadPaused(path, start_position) => {
                        thread_state.lock().unwrap().error.clear();
                        if output.is_none() {
                            continue;
                        }
                        source_generation = source_generation.wrapping_add(1);
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        fade_start = None;

                        match load_track_into_sink(
                            &primary,
                            &path,
                            start_position,
                            eq_enabled,
                            eq_gains,
                            &replaygain,
                            &thread_vis,
                        ) {
                            Ok(d) => {
                                primary.set_volume(global_volume);
                                primary.pause();
                                playing = false;
                                position_base_ms = start_position.as_millis() as u64;
                                track_started_at = None;
                                current_path = Some(path.clone());

                                if let Ok(mut s) = thread_state.lock() {
                                    s.current_path = Some(path);
                                    s.is_playing = false;
                                    s.position_ms = position_base_ms;
                                    s.duration_ms = d;
                                }
                            }
                            Err(e) => {
                                playing = false;
                                track_started_at = None;
                                report_playback_error(
                                    &thread_state,
                                    "restore",
                                    &e.to_string(),
                                    true,
                                );
                            }
                        }
                    }
                    AudioCommand::PlayCrossfade(path, cross_dur) => {
                        if output.is_none() {
                            continue;
                        }
                        source_generation = source_generation.wrapping_add(1);
                        if !playing || primary.empty() {
                            let _ = thread_tx.send(AudioCommand::Play(path));
                        } else {
                            match load_track_into_sink(
                                &secondary,
                                &path,
                                Duration::from_millis(0),
                                eq_enabled,
                                eq_gains,
                                &replaygain,
                                &thread_vis,
                            ) {
                                Ok(d) => {
                                    fading_out_sink = Some(primary);
                                    primary = secondary;
                                    secondary = Sink::connect_new(&mixer, output_format);

                                    primary.set_volume(0.0);
                                    primary.play();

                                    fade_start = Some(Instant::now());
                                    fade_duration = cross_dur;

                                    playing = true;
                                    position_base_ms = 0;
                                    track_started_at = Some(Instant::now());
                                    current_path = Some(path.clone());

                                    if let Ok(mut s) = thread_state.lock() {
                                        s.current_path = Some(path);
                                        s.is_playing = true;
                                        s.position_ms = 0;
                                        s.duration_ms = d;
                                    }
                                }
                                Err(e) => {
                                    playing = false;
                                    track_started_at = None;
                                    report_playback_error(
                                        &thread_state,
                                        "play",
                                        &e.to_string(),
                                        true,
                                    );
                                }
                            }
                        }
                    }
                    AudioCommand::Pause => {
                        if playing {
                            if let Some(started_at) = track_started_at.take() {
                                position_base_ms = position_base_ms
                                    .saturating_add(started_at.elapsed().as_millis() as u64);
                            }
                        }
                        primary.pause();
                        playing = false;
                        if let Ok(mut s) = thread_state.lock() {
                            s.position_ms = position_base_ms;
                            s.is_playing = false;
                        }
                    }
                    AudioCommand::Resume => {
                        if buffered && !playing {
                            let Some(path) = current_path.as_deref() else {
                                continue;
                            };
                            output.as_ref().unwrap().discard_pending();
                            source_generation = source_generation.wrapping_add(1);
                            primary.stop();
                            primary = Sink::connect_new(&mixer, output_format);
                            primary.set_volume(global_volume);
                            // Reset discards the device's outstanding buffer on pause.
                            // Resume from the frozen position, not the producer's
                            // decoded-ahead position, so those samples are not skipped.
                            if let Err(error) = load_track_into_sink(
                                &primary,
                                path,
                                Duration::from_millis(position_base_ms),
                                eq_enabled,
                                eq_gains,
                                &replaygain,
                                &thread_vis,
                            ) {
                                report_playback_error(&thread_state, "resume", &error, false);
                                continue;
                            }
                        }
                        primary.play();
                        playing = true;
                        track_started_at = Some(Instant::now());
                        if let Ok(mut s) = thread_state.lock() {
                            s.is_playing = true;
                        }
                    }
                    AudioCommand::Seek(pos) => {
                        thread_state.lock().unwrap().error.clear();
                        let duration = thread_state
                            .lock()
                            .ok()
                            .map(|state| state.duration_ms)
                            .unwrap_or_default();
                        let seek_result = if buffered {
                            current_path
                                .as_deref()
                                .ok_or_else(|| "No current track".to_string())
                                .and_then(|path| {
                                    load_track_into_sink(
                                        &primary,
                                        path,
                                        pos,
                                        eq_enabled,
                                        eq_gains,
                                        &replaygain,
                                        &thread_vis,
                                    )
                                    .map(|_| ())
                                })
                        } else {
                            primary
                                .try_seek(pos)
                                .map_err(|e| e.to_string())
                                .or_else(|seek_error| {
                                    let Some(path) = current_path.as_deref() else {
                                        return Err(seek_error);
                                    };

                                    // Some decoders cannot seek in place. Rebuild only for those.
                                    load_track_into_sink(
                                        &primary,
                                        path,
                                        pos,
                                        eq_enabled,
                                        eq_gains,
                                        &replaygain,
                                        &thread_vis,
                                    )
                                    .map(|_| ())
                                    .map_err(|_| seek_error)
                                })
                        };

                        match seek_result {
                            Ok(()) => {
                                position_base_ms = pos.as_millis() as u64;
                                track_started_at = playing.then(Instant::now);
                                if playing {
                                    primary.play();
                                } else {
                                    primary.pause();
                                }
                                if let Ok(mut s) = thread_state.lock() {
                                    s.position_ms = position_base_ms;
                                    s.duration_ms = duration;
                                    s.is_playing = playing;
                                }
                            }
                            Err(e) => {
                                report_playback_error(&thread_state, "seek", &e.to_string(), false)
                            }
                        }
                    }
                    AudioCommand::SetVolume(vol) => {
                        global_volume = vol;
                        primary.set_volume(vol);
                        if let Ok(mut s) = thread_state.lock() {
                            s.volume = vol;
                        }
                    }
                    AudioCommand::SetReplayGain(enabled) => {
                        replaygain.store(enabled, Ordering::Relaxed);
                    }
                    AudioCommand::SetEqEnabled(enabled) => {
                        eq_enabled = enabled;
                        restart_current_with_eq(
                            &primary,
                            &thread_state,
                            &mut playing,
                            &mut position_base_ms,
                            (eq_enabled, eq_gains),
                            &replaygain,
                            &thread_vis,
                        );
                    }
                    AudioCommand::SetEqGains(gains) => {
                        eq_gains = gains;
                        if eq_enabled {
                            restart_current_with_eq(
                                &primary,
                                &thread_state,
                                &mut playing,
                                &mut position_base_ms,
                                (eq_enabled, eq_gains),
                                &replaygain,
                                &thread_vis,
                            );
                        }
                    }
                    AudioCommand::Stop => {
                        deferred_next = None;
                        current_path = None;
                        position_base_ms = 0;
                        if let Some(handle) = output.as_ref() {
                            handle.discard_pending();
                        }
                        source_generation = source_generation.wrapping_add(1);
                        primary.stop();
                        secondary.stop();
                        fading_out_sink = None;
                        track_started_at = None;
                        playing = false;
                        if let Ok(mut s) = thread_state.lock() {
                            s.current_path = None;
                            s.position_ms = 0;
                            s.duration_ms = 0;
                            s.is_playing = false;
                        }
                    }
                    AudioCommand::ClearQueued => {
                        // Preserve the audio thread's current state, including a Resume
                        // that the frontend has not observed yet.
                        source_generation = source_generation.wrapping_add(1);
                        if let Some(path) = current_path.as_deref() {
                            let position = position_base_ms.saturating_add(
                                track_started_at
                                    .map(|start| start.elapsed().as_millis() as u64)
                                    .unwrap_or(0),
                            );
                            match load_track_into_sink(
                                &primary,
                                path,
                                Duration::from_millis(position),
                                eq_enabled,
                                eq_gains,
                                &replaygain,
                                &thread_vis,
                            ) {
                                Ok(duration) => {
                                    primary.set_volume(global_volume);
                                    if playing {
                                        primary.play();
                                    } else {
                                        primary.pause();
                                    }
                                    position_base_ms = position;
                                    track_started_at = playing.then(Instant::now);
                                    if let Ok(mut state) = thread_state.lock() {
                                        state.position_ms = position;
                                        state.duration_ms = duration;
                                        state.is_playing = playing;
                                    }
                                }
                                Err(error) => report_playback_error(
                                    &thread_state,
                                    "queue",
                                    &error.to_string(),
                                    false,
                                ),
                            }
                        }
                    }
                    AudioCommand::QueueNext(path) => {
                        deferred_next = None;
                        if requested_output.exclusive {
                            match track_sample_rate(&path) {
                                Ok(rate) if negotiated_rate != Some(rate) => {
                                    deferred_next = Some(path);
                                    continue;
                                }
                                Ok(_) => (),
                                Err(error) => {
                                    report_playback_error(&thread_state, "queue", &error, false);
                                    continue;
                                }
                            }
                        }
                        thread_state.lock().unwrap().error.clear();
                        let thread_tx_inner = thread_tx.clone();
                        let path_inner = path.clone();
                        let generation = source_generation;

                        let on_start = Box::new(move |duration| {
                            let _ = thread_tx_inner.send(AudioCommand::UpdateMetadata(
                                path_inner, duration, generation,
                            ));
                        });

                        match create_track_source(
                            &path,
                            Duration::from_millis(0),
                            eq_enabled,
                            eq_gains,
                            &replaygain,
                            &thread_vis,
                            Some(on_start),
                        ) {
                            Ok((_, source)) => {
                                if let Err(error) = primary.append(source) {
                                    report_playback_error(&thread_state, "queue", &error, false);
                                }
                            }
                            Err(e) => {
                                report_playback_error(&thread_state, "queue", &e.to_string(), false)
                            }
                        }
                    }
                    AudioCommand::UpdateMetadata(path, d, generation) => {
                        if generation != source_generation {
                            continue;
                        }
                        current_path = Some(path.clone());
                        position_base_ms = 0;
                        track_started_at = Some(Instant::now());
                        let rate = track_sample_rate(&path).unwrap_or_default();
                        if let Ok(mut s) = thread_state.lock() {
                            s.output.source_sample_rate = rate;
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

            if let Some(handle) = output.as_ref() {
                handle.set_draining(playing && primary.empty());
                handle.set_active(playing);
                if let Some(error) = handle.take_error() {
                    recovery_notice = Some(error.clone());
                    {
                        let mut state = thread_state.lock().unwrap();
                        state.output.error = error.clone();
                        state.error = format!("output:{error}");
                        state.error_revision = state.error_revision.wrapping_add(1);
                    }
                    let _ = thread_tx.send(AudioCommand::SetOutput(OutputConfig::default()));
                    // Force recovery even when the failed endpoint was already default.
                    drop(output.take());
                }
            }

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
                let pos_ms = track_started_at
                    .map(|started_at| {
                        position_base_ms.saturating_add(started_at.elapsed().as_millis() as u64)
                    })
                    .unwrap_or(position_base_ms);
                if let Ok(mut s) = thread_state.lock() {
                    s.position_ms = pos_ms;
                }
                if let Some(ref cb) = event_callback {
                    cb("playback-progress", pos_ms);
                }
            }

            if playing
                && primary.empty()
                && fading_out_sink.is_none()
                && !output.as_ref().is_some_and(OutputHandle::has_pending)
            {
                if let Some(path) = deferred_next.take() {
                    playing = false;
                    track_started_at = None;
                    let _ = thread_tx.send(AudioCommand::PlayQueued(path, source_generation));
                    continue;
                }
                playing = false;
                track_started_at = None;
                if let Ok(mut s) = thread_state.lock() {
                    s.is_playing = false;
                }
                if let Some(ref cb) = event_callback {
                    cb("playback-ended", 0);
                }
            }
        }
    });

    (tx, state, visualizer, handle)
}
