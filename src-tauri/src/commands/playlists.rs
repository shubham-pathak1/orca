use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use orca_core::db;
use serde::Serialize;
use tauri::State;

use crate::state::SharedOrcaState;

#[derive(Serialize)]
pub(crate) struct PlaylistImportResult {
    playlists: Vec<db::Playlist>,
    playlist_name: String,
    imported_tracks: usize,
    unavailable_tracks: usize,
}

#[derive(Serialize)]
pub(crate) struct PlaylistExportResult {
    exported_tracks: usize,
}

fn playlist_path_key(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let value = path.to_string_lossy().replace('/', "\\");

    #[cfg(target_os = "windows")]
    {
        value.to_ascii_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        value
    }
}

fn resolve_m3u_path(value: &str, playlist_dir: &Path) -> Option<PathBuf> {
    let value = value.trim().trim_start_matches('\u{feff}');
    if value.is_empty() || value.starts_with('#') {
        return None;
    }

    let value = value
        .strip_prefix("file:///")
        .or_else(|| value.strip_prefix("file://"))
        .unwrap_or(value);
    let path = PathBuf::from(value);
    Some(if path.is_absolute() {
        path
    } else {
        playlist_dir.join(path)
    })
}

fn m3u_playlist_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Imported Playlist")
        .to_string()
}

fn m3u_field(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn m3u_filename(value: &str) -> String {
    let filename = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    let filename = filename.trim_matches([' ', '.']);

    if filename.is_empty() {
        "playlist".to_string()
    } else {
        filename.to_string()
    }
}

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

pub(crate) fn import_playlist(
    state: State<'_, SharedOrcaState>,
) -> Result<PlaylistImportResult, String> {
    let Some(playlist_path) = rfd::FileDialog::new()
        .add_filter("M3U playlists", &["m3u", "m3u8"])
        .set_title("Import playlist")
        .pick_file()
    else {
        return Err("Playlist import cancelled".to_string());
    };
    let contents = fs::read_to_string(&playlist_path).map_err(|error| error.to_string())?;
    let playlist_dir = playlist_path.parent().unwrap_or_else(|| Path::new("."));
    let entries = contents
        .lines()
        .filter_map(|line| resolve_m3u_path(line, playlist_dir))
        .collect::<Vec<_>>();

    let state = state.0.lock().map_err(|error| error.to_string())?;
    let path_index = db::get_song_path_index(&state.db_conn)?
        .into_iter()
        .map(|(path, id)| (playlist_path_key(Path::new(&path)), id))
        .collect::<HashMap<_, _>>();
    let mut matched_song_ids = Vec::new();
    let mut seen_song_ids = HashSet::new();
    let mut unavailable_tracks = 0;

    for entry in entries {
        match path_index.get(&playlist_path_key(&entry)) {
            Some(song_id) if seen_song_ids.insert(*song_id) => matched_song_ids.push(*song_id),
            Some(_) => {}
            None => unavailable_tracks += 1,
        }
    }

    if matched_song_ids.is_empty() {
        return Err("No tracks from this playlist are in your library".to_string());
    }

    let playlist_name = m3u_playlist_name(&playlist_path);
    let playlist_id = db::create_playlist(&state.db_conn, &playlist_name, None)?;
    for song_id in &matched_song_ids {
        db::add_to_playlist(&state.db_conn, playlist_id, *song_id)?;
    }

    Ok(PlaylistImportResult {
        playlists: db::get_playlists(&state.db_conn)?,
        playlist_name,
        imported_tracks: matched_song_ids.len(),
        unavailable_tracks,
    })
}

pub(crate) fn export_playlist(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaylistExportResult, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    let songs = db::get_playlist_export_songs(&state.db_conn, playlist_id)?;
    let playlist_name = db::get_playlists(&state.db_conn)?
        .into_iter()
        .find(|playlist| playlist.id == playlist_id)
        .map(|playlist| playlist.name)
        .ok_or_else(|| "Playlist not found".to_string())?;

    let Some(path) = rfd::FileDialog::new()
        .add_filter("M3U playlist", &["m3u"])
        .set_title("Export playlist")
        .set_file_name(format!("{}.m3u", m3u_filename(&playlist_name)))
        .save_file()
    else {
        return Err("Playlist export cancelled".to_string());
    };
    let path = if path.extension().is_some() {
        path
    } else {
        path.with_extension("m3u")
    };

    let mut contents = String::from("#EXTM3U\n");
    for song in &songs {
        contents.push_str(&format!(
            "#EXTINF:{},{} - {}\n{}\n",
            song.duration,
            m3u_field(&song.artist),
            m3u_field(&song.title),
            song.path
        ));
    }
    fs::write(path, contents).map_err(|error| error.to_string())?;

    Ok(PlaylistExportResult {
        exported_tracks: songs.len(),
    })
}
