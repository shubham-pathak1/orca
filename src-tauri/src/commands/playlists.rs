use orca_core::db;
use tauri::State;

use crate::state::SharedOrcaState;

pub(crate) fn list_playlists(
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn create_playlist(
    name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Playlist name cannot be empty".to_string());
    }

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::create_playlist(&state.db_conn, name, None)?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn rename_playlist(
    playlist_id: i64,
    name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Playlist name cannot be empty".to_string());
    }

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::rename_playlist(&state.db_conn, playlist_id, name)?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn delete_playlist(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::delete_playlist(&state.db_conn, playlist_id)?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn add_song_to_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::add_to_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn remove_song_from_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_from_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn playlist_song_ids(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<i64>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlist_song_ids(&state.db_conn, playlist_id)
}

pub(crate) fn choose_playlist_cover(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
    else {
        return Err("Cover selection cancelled".to_string());
    };

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_playlist_cover(&state.db_conn, playlist_id, Some(&path.to_string_lossy()))?;
    db::get_playlists(&state.db_conn)
}

pub(crate) fn remove_playlist_cover(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_playlist_cover(&state.db_conn, playlist_id, None)?;
    db::get_playlists(&state.db_conn)
}
