mod sources;
mod waveform;
mod worker;
pub use waveform::{compute_waveform_peaks, compute_waveform_peaks_cancellable};
pub use worker::{spawn_audio_thread, spawn_audio_thread_managed};

pub const MAX_WAVEFORM_BUCKETS: usize = 1000;

use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use crate::audio_output::devices as output_devices;
use crate::audio_output::{OutputConfig, OutputStatus};
use log::error;
use serde::{Deserialize, Serialize};

const VISUALIZER_BUFFER_SIZE: usize = 120;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaybackState {
    pub current_path: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing: bool,
    pub volume: f32,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub error_revision: u64,
    #[serde(default)]
    pub output: OutputStatus,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            current_path: None,
            position_ms: 0,
            duration_ms: 0,
            is_playing: false,
            volume: 1.0,
            error: String::new(),
            error_revision: 0,
            output: OutputStatus::default(),
        }
    }
}

fn report_playback_error(
    state: &Arc<Mutex<PlaybackState>>,
    action: &str,
    detail: &str,
    stopped: bool,
) {
    error!("Audio Engine: {action} error: {detail}");
    if let Ok(mut state) = state.lock() {
        state.error = format!("playback:{action}:{detail}");
        state.error_revision = state.error_revision.wrapping_add(1);
        if stopped {
            state.is_playing = false;
        }
    }
}

pub struct VisualizerData {
    pub peaks: Arc<Mutex<std::collections::VecDeque<f32>>>,
}

impl Default for VisualizerData {
    fn default() -> Self {
        Self {
            peaks: Arc::new(Mutex::new(std::collections::VecDeque::from(
                vec![0.0; VISUALIZER_BUFFER_SIZE],
            ))),
        }
    }
}

pub enum AudioCommand {
    Play(String), // Clears the active and preloaded sinks.
    LoadPaused(String, Duration),
    PlayCrossfade(String, Duration),
    Pause,
    Resume,
    Seek(Duration),
    Stop,
    Shutdown,
    SetVolume(f32),
    SetEqEnabled(bool),
    SetEqGains([f32; 5]),
    QueueNext(String),
    ClearQueued,
    SetOutput(OutputConfig),
    DefaultOutputChanged,
    UpdateMetadata(String, u64, u64),
}

#[cfg(test)]
mod error_reporting_tests {
    use super::*;
    #[test]
    fn decode_failure_stops_playback_and_reports_repeated_attempts() {
        let state = Arc::new(Mutex::new(PlaybackState {
            is_playing: true,
            current_path: Some("previous.flac".into()),
            ..Default::default()
        }));
        report_playback_error(&state, "play", "invalid header", true);
        {
            let s = state.lock().unwrap();
            assert!(!s.is_playing);
            assert_eq!(s.error_revision, 1);
            assert_eq!(s.error, "playback:play:invalid header");
            assert_eq!(s.current_path.as_deref(), Some("previous.flac"));
        }
        report_playback_error(&state, "play", "invalid header", true);
        assert_eq!(state.lock().unwrap().error_revision, 2);
    }
    #[test]
    fn failed_seek_or_preload_keeps_current_playback_running() {
        let state = Arc::new(Mutex::new(PlaybackState {
            is_playing: true,
            ..Default::default()
        }));
        for action in ["seek", "queue"] {
            report_playback_error(&state, action, "unsupported", false);
            assert!(state.lock().unwrap().is_playing);
        }
    }
}

#[cfg(test)]
mod waveform_cancellation_tests {
    #[test]
    fn cancelled_analysis_does_not_open_audio_file() {
        assert!(
            super::compute_waveform_peaks_cancellable("absent.flac", 720, || true)
                .unwrap_err()
                .contains("cancelled")
        );
    }
}
