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
            start.elapsed() < Duration::from_secs(45),
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
        }))
        .unwrap();
        let selected = wait(&state, "selected shared device", |s| {
            s.output.revision >= 4 && !s.output.switching
        });
        assert_eq!(selected.output.config.device_id, default.id);
        assert!(selected.output.error.is_empty());
        assert!(!selected.is_playing);
    });
    tx.send(AudioCommand::Shutdown).unwrap();
    let shutdown = Instant::now();
    worker.join().unwrap();
    assert!(shutdown.elapsed() < Duration::from_secs(2));
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
