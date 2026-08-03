use std::time::Duration;

use orca_core::{
    audio_engine::{self, AudioCommand, PlaybackState},
    db,
};
use tauri::State;

use crate::state::{playback_snapshot_from, SharedOrcaState};

pub(crate) fn playback_snapshot(
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn play_song(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    ensure_song_exists(&path)?;
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Play(path))
        .map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_millis(40));
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn queue_next_playback(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    ensure_song_exists(&path)?;
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::QueueNext(path))
        .map_err(|error| error.to_string())?;
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn pause_playback(
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Pause)
        .map_err(|error| error.to_string())?;
    if let Ok(mut playback) = state.playback_state.lock() {
        playback.is_playing = false;
    }
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn resume_playback(
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Resume)
        .map_err(|error| error.to_string())?;
    if let Ok(mut playback) = state.playback_state.lock() {
        playback.is_playing = true;
    }
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn seek_playback(
    position_ms: u64,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Seek(Duration::from_millis(position_ms)))
        .map_err(|error| error.to_string())?;
    if let Ok(mut playback) = state.playback_state.lock() {
        playback.position_ms = position_ms;
    }
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn set_volume(
    volume: f32,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    let volume = volume.clamp(0.0, 1.0);
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::SetVolume(volume))
        .map_err(|error| error.to_string())?;
    if let Ok(mut playback) = state.playback_state.lock() {
        playback.volume = volume;
    }
    Ok(playback_snapshot_from(&state))
}

pub(crate) async fn waveform_peaks(
    path: String,
    buckets: usize,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<f32>, String> {
    {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        if let Ok(Some(cached)) = db::get_cached_waveform(&state.db_conn, &path, buckets) {
            return Ok(cached);
        }
    }

    let waveform_path = path.clone();
    let peaks = tauri::async_runtime::spawn_blocking(move || {
        audio_engine::compute_waveform_peaks(&waveform_path, buckets)
    })
    .await
    .map_err(|error| error.to_string())??;

    {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let _ = db::save_waveform(&state.db_conn, &path, buckets, &peaks);
    }

    Ok(peaks)
}

fn ensure_song_exists(path: &str) -> Result<(), String> {
    if std::path::Path::new(path).exists() {
        return Ok(());
    }

    Err("File not found. Your music folder may have moved - try rescanning.".to_string())
}
