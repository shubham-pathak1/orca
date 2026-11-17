//! Explicit hardware test; regular contributor tests never reserve an audio device.
#![cfg(windows)]
use orca_core::{
    audio_engine::{spawn_audio_thread_managed, AudioCommand, PlaybackState},
    audio_output::{devices, OutputConfig},
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

fn wait(
    state: &Arc<Mutex<PlaybackState>>,
    description: &str,
    predicate: impl Fn(&PlaybackState) -> bool,
) -> PlaybackState {
    let start = Instant::now();
    loop {
        let snapshot = state.lock().unwrap().clone();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "Timed out waiting for {description}: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "requires Windows audio hardware and ORCA_TEST_AUDIO pointing to a disposable WAV fixture"]
fn shared_exclusive_switching_seeking_and_unavailable_device_recovery() {
    let fixture =
        std::env::var("ORCA_TEST_AUDIO").expect("Set ORCA_TEST_AUDIO to a disposable WAV");
    let outputs = devices().unwrap();
    let default = outputs
        .iter()
        .find(|device| device.is_default)
        .expect("No default output device");
    let (tx, state, _, worker) = spawn_audio_thread_managed(None::<fn(&str, u64)>);
    // Always shut down/join the output workers, including when an assertion fails.
    let result = std::panic::catch_unwind(|| {
        wait(&state, "shared startup", |s| s.output.sample_rate > 0);
        tx.send(AudioCommand::SetVolume(0.0)).unwrap();
        tx.send(AudioCommand::Play(fixture.clone())).unwrap();
        wait(&state, "shared playback", |s| {
            s.is_playing && s.position_ms > 200
        });
        tx.send(AudioCommand::SetOutput(OutputConfig {
            device_id: default.id.clone(),
            exclusive: true,
            unprocessed: false,
        }))
        .unwrap();
        let exclusive = wait(&state, "exclusive output", |s| {
            s.output.revision >= 1 && !s.output.switching
        });
        assert!(
            exclusive.output.config.exclusive,
            "Exclusive hardware check failed: {}",
            exclusive.output.error
        );
        assert!(exclusive.is_playing);
        assert_eq!(exclusive.volume, 0.0);
        println!(
            "Exclusive: {} Hz, {} channels, {}",
            exclusive.output.sample_rate, exclusive.output.channels, exclusive.output.sample_format
        );
        tx.send(AudioCommand::Pause).unwrap();
        wait(&state, "pause", |s| !s.is_playing);
        tx.send(AudioCommand::Seek(Duration::from_millis(1000)))
            .unwrap();
        let paused = wait(&state, "paused seek", |s| s.position_ms == 1000);
        assert!(!paused.is_playing);
        tx.send(AudioCommand::Resume).unwrap();
        wait(&state, "exclusive resume", |s| {
            s.is_playing && s.position_ms > 1100
        });
        tx.send(AudioCommand::SetOutput(OutputConfig::default()))
            .unwrap();
        let shared = wait(&state, "shared switch", |s| {
            s.output.revision >= 2 && !s.output.switching
        });
        assert!(!shared.output.config.exclusive);
        assert!(shared.is_playing);
        assert!(shared.position_ms >= 1100);
        tx.send(AudioCommand::SetOutput(OutputConfig {
            device_id: "missing-device-for-orca-test".into(),
            exclusive: true,
            unprocessed: false,
        }))
        .unwrap();
        let fallback = wait(&state, "unavailable device fallback", |s| {
            s.output.revision >= 3 && !s.output.switching
        });
        assert_eq!(fallback.output.config, OutputConfig::default());
        assert!(!fallback.output.error.is_empty());
        assert!(fallback.is_playing);
        tx.send(AudioCommand::Pause).unwrap();
        wait(&state, "pause before switch", |s| !s.is_playing);
        tx.send(AudioCommand::SetOutput(OutputConfig {
            device_id: default.id.clone(),
            exclusive: false,
            unprocessed: false,
        }))
        .unwrap();
        let selected = wait(&state, "selected shared device", |s| {
            s.output.revision >= 4 && !s.output.switching
        });
        assert_eq!(selected.output.config.device_id, default.id);
        assert!(selected.output.error.is_empty());
        assert!(!selected.is_playing);
        tx.send(AudioCommand::DefaultOutputChanged).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(
            state.lock().unwrap().output.revision,
            selected.output.revision,
            "a fixed endpoint must ignore system-default changes"
        );
        tx.send(AudioCommand::Resume).unwrap();
        wait(&state, "selected-device resume", |s| s.is_playing);
        let duration = state.lock().unwrap().duration_ms;
        tx.send(AudioCommand::Seek(Duration::from_millis(
            duration.saturating_sub(250),
        )))
        .unwrap();
        wait(&state, "natural end after draining buffered PCM", |s| {
            !s.is_playing && s.position_ms >= duration.saturating_sub(250)
        });
    });
    tx.send(AudioCommand::Shutdown).unwrap();
    let shutdown = Instant::now();
    worker.join().unwrap();
    assert!(shutdown.elapsed() < Duration::from_secs(2));
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// Silent PCM fixtures exercise rate changes without touching a user's library.
struct RateFixtures(std::path::PathBuf);
impl RateFixtures {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "orca-output-rates-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn wav(&self, rate: u32, name: &str) -> String {
        self.wav_bits(rate, 16, name)
    }
    fn wav_bits(&self, rate: u32, bits: u16, name: &str) -> String {
        let frame_bytes = bits / 8 * 2;
        let samples = rate * frame_bytes as u32;
        let mut bytes = Vec::with_capacity(samples as usize + 44);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x02\0");
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * frame_bytes as u32).to_le_bytes());
        bytes.extend_from_slice(&frame_bytes.to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&samples.to_le_bytes());
        bytes.resize(samples as usize + 44, 0);
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
}
impl Drop for RateFixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "reserves Windows audio hardware; generated fixtures are silent"]
fn unprocessed_mode_preserves_controls_queue_and_explicit_format_failure() {
    let fixtures = RateFixtures::new();
    let first = fixtures.wav(44100, "integer-first.wav");
    let next = fixtures.wav(44100, "integer-next.wav");
    let high = fixtures.wav_bits(48000, 24, "integer-high.wav");
    let wide = fixtures.wav_bits(44100, 32, "integer-wide.wav");
    let (tx, state, _, worker) = spawn_audio_thread_managed(None::<fn(&str, u64)>);
    let result = std::panic::catch_unwind(|| {
        wait(&state, "initial shared device", |s| {
            s.output.sample_rate > 0
        });
        tx.send(AudioCommand::SetVolume(0.0)).unwrap();
        tx.send(AudioCommand::SetOutput(OutputConfig {
            device_id: String::new(),
            exclusive: true,
            unprocessed: true,
        }))
        .unwrap();
        tx.send(AudioCommand::Play(first.clone())).unwrap();
        let started = wait(&state, "unprocessed integer playback", |s| {
            s.is_playing && s.output.config.unprocessed && s.position_ms > 30
        });
        assert_eq!(started.volume, 1.0);
        assert_eq!(started.output.sample_rate, 44100);
        let revision = started.output.revision;
        tx.send(AudioCommand::SetReplayGain(true)).unwrap();
        tx.send(AudioCommand::SetVolume(0.2)).unwrap();
        tx.send(AudioCommand::QueueNext(next.clone())).unwrap();
        wait(&state, "integer same-format gapless transition", |s| {
            s.current_path.as_deref() == Some(&next)
        });
        assert_eq!(state.lock().unwrap().volume, 1.0);
        tx.send(AudioCommand::QueueNext(high.clone())).unwrap();
        let high_state = wait(&state, "integer format change after draining", |s| {
            s.current_path.as_deref() == Some(&high) && s.is_playing
        });
        assert_eq!(high_state.output.sample_rate, 48000);
        assert_eq!(high_state.output.revision, revision);
        tx.send(AudioCommand::Pause).unwrap();
        wait(&state, "integer pause", |s| !s.is_playing);
        tx.send(AudioCommand::Seek(Duration::from_millis(200)))
            .unwrap();
        wait(&state, "integer paused seek", |s| {
            s.position_ms == 200 && !s.is_playing
        });
        tx.send(AudioCommand::Resume).unwrap();
        wait(&state, "integer resume", |s| {
            s.is_playing && s.position_ms > 250
        });
        let error_revision = state.lock().unwrap().error_revision;
        tx.send(AudioCommand::Play(wide.clone())).unwrap();
        let wide_state = wait(&state, "32-bit exact format or explicit refusal", |s| {
            (s.current_path.as_deref() == Some(&wide) && s.is_playing)
                || s.error_revision > error_revision
        });
        if wide_state.is_playing {
            assert_eq!(wide_state.output.sample_format, "32-bit Int");
            assert_eq!(wide_state.output.sample_rate, 44100);
            println!("Device accepted full 32-valid-bit integer output");
        } else {
            assert!(wide_state.output.error.contains("cannot preserve"));
            assert_eq!(wide_state.output.sample_rate, 0);
            assert!(wide_state.output.config.unprocessed);
            println!(
                "Device refused 32 valid bits explicitly: {}",
                wide_state.output.error
            );
        }
        tx.send(AudioCommand::Play(first.clone())).unwrap();
        wait(&state, "integer recovery", |s| {
            s.current_path.as_deref() == Some(&first) && s.is_playing
        });
        tx.send(AudioCommand::QueueNext(high.clone())).unwrap();
        tx.send(AudioCommand::ClearQueued).unwrap();
        tx.send(AudioCommand::Stop).unwrap();
        wait(&state, "integer stop releases endpoint", |s| {
            s.current_path.is_none() && s.output.sample_rate == 0
        });
        tx.send(AudioCommand::SetOutput(OutputConfig::default()))
            .unwrap();
        wait(&state, "return to shared output", |s| {
            !s.output.config.unprocessed && s.output.sample_rate > 0
        });
        tx.send(AudioCommand::Play(first.clone())).unwrap();
        let shared = wait(&state, "processed playback after unprocessed mode", |s| {
            s.current_path.as_deref() == Some(&first) && s.is_playing
        });
        assert_eq!(shared.volume, 0.2);
    });
    let _ = tx.send(AudioCommand::Shutdown);
    worker.join().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

#[test]
#[ignore = "reserves the default Windows endpoint; run only when other players are closed"]
fn exclusive_tracks_follow_rates_and_cancel_deferred_transitions() {
    let fixtures = RateFixtures::new();
    let low = fixtures.wav(44100, "low.wav");
    let same = fixtures.wav(44100, "same.wav");
    let high = fixtures.wav(192000, "high.wav");
    let transitions = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = transitions.clone();
    let (tx, state, _, worker) = spawn_audio_thread_managed(Some(move |event: &str, _| {
        if event == "track-transitioned" {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }));
    let result = std::panic::catch_unwind(|| {
        wait(&state, "startup", |s| s.output.sample_rate > 0);
        tx.send(AudioCommand::SetVolume(0.0)).unwrap();
        tx.send(AudioCommand::SetOutput(OutputConfig {
            device_id: String::new(),
            exclusive: true,
            unprocessed: false,
        }))
        .unwrap();
        tx.send(AudioCommand::Play(low.clone())).unwrap();
        let first = wait(&state, "44.1 kHz source", |s| {
            s.is_playing
                && s.current_path.as_deref() == Some(&low)
                && s.output.source_sample_rate == 44100
        });
        assert!(first.output.config.exclusive, "{}", first.output.error);
        let revision = first.output.revision;
        tx.send(AudioCommand::QueueNext(same.clone())).unwrap();
        let second = wait(&state, "same-rate queue transition", |s| {
            s.current_path.as_deref() == Some(&same)
        });
        assert_eq!(second.output.revision, revision);
        assert_eq!(second.output.sample_rate, first.output.sample_rate);
        tx.send(AudioCommand::QueueNext(high.clone())).unwrap();
        let third = wait(&state, "mixed-rate queue transition", |s| {
            s.current_path.as_deref() == Some(&high)
                && s.output.source_sample_rate == 192000
                && s.is_playing
        });
        assert_eq!(third.output.revision, revision);
        assert!(third.output.config.exclusive, "{}", third.output.error);
        assert_eq!(
            third.output.notice.is_empty(),
            third.output.sample_rate == 192000
        );
        assert_eq!(transitions.load(std::sync::atomic::Ordering::SeqCst), 2);
        println!(
            "Rate negotiation: 44100 -> {}, 192000 -> {}; {}",
            first.output.sample_rate, third.output.sample_rate, third.output.notice
        );
        tx.send(AudioCommand::QueueNext(low.clone())).unwrap();
        tx.send(AudioCommand::ClearQueued).unwrap();
        wait(&state, "cancelled queue finishes current track", |s| {
            !s.is_playing
        });
        assert_eq!(
            state.lock().unwrap().current_path.as_deref(),
            Some(high.as_str())
        );
        assert_eq!(transitions.load(std::sync::atomic::Ordering::SeqCst), 2);
        tx.send(AudioCommand::Play(low.clone())).unwrap();
        wait(&state, "new track after cancellation", |s| {
            s.is_playing && s.current_path.as_deref() == Some(&low)
        });
        tx.send(AudioCommand::QueueNext(high.clone())).unwrap();
        tx.send(AudioCommand::Stop).unwrap();
        wait(&state, "stop cancels deferred track", |s| {
            s.current_path.is_none()
        });
        thread::sleep(Duration::from_millis(250));
        assert!(state.lock().unwrap().current_path.is_none());
    });
    let _ = tx.send(AudioCommand::Shutdown);
    worker.join().unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
