//! Playback commands and snapshots. Audio decoding/output remain in orca-core.
use super::{ffi, Backend};
use orca_core::audio_engine::{self, AudioCommand};
use std::{path::Path, sync::atomic::Ordering, time::Duration};
impl Backend {
    pub fn snapshot(&self) -> Result<ffi::Snapshot, String> {
        let state = self.playback.lock().map_err(|e| e.to_string())?;
        Ok(ffi::Snapshot {
            path: state.current_path.clone().unwrap_or_default(),
            position_ms: state.position_ms,
            duration_ms: state.duration_ms,
            playing: state.is_playing,
            volume: state.volume,
            scanning: self.scan.active.load(Ordering::Acquire),
            scanned: self.scan.count.load(Ordering::Relaxed),
            revision: self.scan.revision.load(Ordering::Relaxed),
            ended: self.ended.load(Ordering::Relaxed),
            transitioned: self.transitioned.load(Ordering::Relaxed),
            error: self.scan.error.lock().map_err(|e| e.to_string())?.clone(),
            output_status: serde_json::to_string(&state.output).map_err(|e| e.to_string())?,
            output_revision: state.output.revision,
            playback_error: state.error.clone(),
            playback_error_revision: state.error_revision,
        })
    }

    pub fn output_devices(&self) -> Result<String, String> {
        serde_json::to_string(&audio_engine::output_devices()?).map_err(|e| e.to_string())
    }

    pub fn set_output(&self, device_id: &str, exclusive: bool) -> Result<(), String> {
        if device_id.len() > 8192 || device_id.contains('\0') {
            return Err("Invalid output device ID".into());
        }
        let tx = self
            .audio_tx
            .as_ref()
            .ok_or("Audio is disabled for this session")?;
        tx.send(AudioCommand::SetOutput(
            orca_core::audio_output::OutputConfig {
                device_id: device_id.into(),
                exclusive,
            },
        ))
        .map_err(|_| "Audio device unavailable".into())
    }

    pub fn command(&self, action: &str, path: &str, value: f64) -> Result<(), String> {
        use super::types::PlaybackCommand;
        if !value.is_finite() {
            return Err("Invalid numeric value".into());
        }
        let command = match action {
            "play" => PlaybackCommand::Play(path.into()),
            "queue" => PlaybackCommand::QueueNext(path.into()),
            "load-paused" => PlaybackCommand::LoadPaused {
                path: path.into(),
                position_ms: value.clamp(0.0, 86_400_000.0) as u64,
            },
            "pause" => PlaybackCommand::Pause,
            "resume" => PlaybackCommand::Resume,
            "stop" => PlaybackCommand::Stop,
            "clear-queued" => PlaybackCommand::ClearQueued,
            "seek" => PlaybackCommand::Seek(value.clamp(0.0, 86_400_000.0) as u64),
            "volume" => PlaybackCommand::Volume(value.clamp(0.0, 1.0) as f32),
            _ => return Err("Unknown audio command".into()),
        };
        self.playback_command(command)
    }
    pub fn playback_command(&self, command: super::types::PlaybackCommand) -> Result<(), String> {
        use super::types::PlaybackCommand;
        let tx = self
            .audio_tx
            .as_ref()
            .ok_or("Audio is disabled for this session")?;
        let validate_path = |path: &str| -> Result<(), String> {
            self.track(path)?;
            if !Path::new(path).is_file() {
                return Err("Audio file no longer exists".into());
            }
            Ok(())
        };
        let cmd = match command {
            PlaybackCommand::Play(path) => {
                validate_path(&path)?;
                AudioCommand::Play(path)
            }
            PlaybackCommand::QueueNext(path) => {
                validate_path(&path)?;
                AudioCommand::QueueNext(path)
            }
            PlaybackCommand::LoadPaused { path, position_ms } => {
                validate_path(&path)?;
                AudioCommand::LoadPaused(path, Duration::from_millis(position_ms.min(86_400_000)))
            }
            PlaybackCommand::Pause => AudioCommand::Pause,
            PlaybackCommand::Resume => AudioCommand::Resume,
            PlaybackCommand::Stop => AudioCommand::Stop,
            PlaybackCommand::ClearQueued => AudioCommand::ClearQueued,
            PlaybackCommand::Seek(value) => {
                AudioCommand::Seek(Duration::from_millis(value.min(86_400_000)))
            }
            PlaybackCommand::Volume(value) => {
                if !value.is_finite() {
                    return Err("Invalid numeric value".into());
                }
                AudioCommand::SetVolume(value.clamp(0.0, 1.0))
            }
        };
        tx.send(cmd).map_err(|_| "Audio device unavailable".into())
    }
}
