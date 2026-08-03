use tauri::State;

use crate::state::SharedOrcaState;

#[derive(serde::Deserialize)]
pub(crate) struct MediaControlsUpdate {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<f64>,
    playing: bool,
    progress: Option<f64>,
    cover_url: Option<String>,
}

pub(crate) fn update_media_controls(
    update: MediaControlsUpdate,
    state: State<'_, SharedOrcaState>,
) -> Result<(), String> {
    let mut state = state.0.lock().map_err(|error| error.to_string())?;

    #[cfg(target_os = "windows")]
    {
        if let Some(controls) = &mut state.media_controls {
            use souvlaki::{MediaMetadata, MediaPlayback, MediaPosition};
            use std::time::Duration;

            let metadata = MediaMetadata {
                title: update.title.as_deref(),
                album: update.album.as_deref(),
                artist: update.artist.as_deref(),
                duration: update.duration.map(Duration::from_secs_f64),
                cover_url: update.cover_url.as_deref(),
                ..Default::default()
            };

            controls.set_metadata(metadata).map_err(|error| error.to_string())?;

            let progress = update.progress.map(|value| MediaPosition(Duration::from_secs_f64(value)));
            let playback = if update.playing {
                MediaPlayback::Playing { progress }
            } else {
                MediaPlayback::Paused { progress }
            };
            controls.set_playback(playback).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}
