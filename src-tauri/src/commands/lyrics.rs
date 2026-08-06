use std::fs;

use orca_core::db;
use tauri::State;

use crate::state::SharedOrcaState;

pub(crate) fn cached_lyrics(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Option<String>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(db::get_lyrics(&state.db_conn, &path))
}

pub(crate) fn cache_lyrics(
    path: String,
    lyrics: String,
    state: State<'_, SharedOrcaState>,
) -> Result<(), String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::set_lyrics(&state.db_conn, &path, &lyrics)
}

pub(crate) fn pick_lyrics_file() -> Result<Option<String>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Lyrics", &["lrc", "txt"])
        .set_title("Import lyrics")
        .pick_file()
    else {
        return Ok(None);
    };

    let lyrics = fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(Some(lyrics.trim_start_matches('\u{feff}').to_string()))
}
