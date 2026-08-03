use std::path::PathBuf;

use orca_core::{db, library::SongMetadataUpdate};
use tauri::{Emitter, State};

use crate::{
    commands::library::{refresh_edited_song, snapshot_from_state},
    state::{artwork_dir, LibrarySnapshot, SharedOrcaState},
};

pub(crate) fn choose_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let image_path = choose_image()?;
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_artist_artwork(
        &state.db_conn,
        &artist_name,
        Some(&image_path.to_string_lossy()),
        None,
    )?;
    snapshot_from_state(&state)
}

pub(crate) fn remove_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_artist_artwork(&state.db_conn, &artist_name)?;
    snapshot_from_state(&state)
}

pub(crate) fn choose_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let image_path = choose_image()?;
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_album_artwork(
        &state.db_conn,
        &album_key,
        Some(&image_path.to_string_lossy()),
        None,
    )?;
    snapshot_from_state(&state)
}

pub(crate) async fn remove_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_album_artwork(&state.db_conn, &album_key)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(&state)
}

pub(crate) async fn fetch_artist_artwork_manual(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let url = orca_core::online_artwork::fetch_itunes_artist_image(&artist_name)
        .or_else(|| orca_core::online_artwork::fetch_deezer_artist_image(&artist_name))
        .ok_or_else(|| "Artist image not found online".to_string())?;
    let prefix = format!("artist_{}", safe_name(&artist_name));
    let paths = orca_core::online_artwork::download_and_cache(&url, &artwork_dir().join("online"), &prefix)?;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_artist_artwork(
        &state.db_conn,
        &artist_name,
        Some(&paths.full),
        Some(&paths.thumb),
    )?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(&state)
}

pub(crate) async fn fetch_album_artwork_manual(
    album_key: String,
    artist: String,
    album: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let url = orca_core::online_artwork::fetch_itunes_album_art(&artist, &album)
        .ok_or_else(|| "Album art not found online".to_string())?;
    let prefix = format!("album_{}", safe_name(&album_key));
    let paths = orca_core::online_artwork::download_and_cache(&url, &artwork_dir().join("online"), &prefix)?;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_album_artwork(
        &state.db_conn,
        &album_key,
        Some(&paths.full),
        Some(&paths.thumb),
    )?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(&state)
}

pub(crate) async fn fetch_all_missing_artwork(
    state: State<'_, SharedOrcaState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db_path = state
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .db_conn
        .path()
        .map(|path| path.to_string());
    let cache_base = artwork_dir().join("online");

    if let Some(path) = db_path {
        std::thread::spawn(move || {
            let conn = rusqlite::Connection::open(path).ok()?;
            fetch_missing_artist_artwork(&conn, &cache_base);
            fetch_missing_album_artwork(&conn, &cache_base);
            let _ = app.emit("library-refreshed", ());
            Some(())
        });
    }

    Ok(())
}

pub(crate) async fn update_song_metadata(
    update: SongMetadataUpdate,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let edit_path = PathBuf::from(&update.path);
    tauri::async_runtime::spawn_blocking(move || orca_core::library::update_song_metadata(update))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

pub(crate) async fn choose_song_cover(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let image_path = choose_image()?;
    let song_path = PathBuf::from(path);
    let edit_path = song_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        orca_core::library::replace_song_cover(&song_path, &image_path)
    })
    .await
    .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

pub(crate) async fn remove_song_cover(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let song_path = PathBuf::from(path);
    let edit_path = song_path.clone();
    tauri::async_runtime::spawn_blocking(move || orca_core::library::remove_song_cover(&song_path))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

fn choose_image() -> Result<PathBuf, String> {
    rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
        .ok_or_else(|| "Cover selection cancelled".to_string())
}

fn safe_name(value: &str) -> String {
    value.replace(|character: char| !character.is_alphanumeric(), "_")
}

fn fetch_missing_artist_artwork(conn: &rusqlite::Connection, cache_base: &std::path::Path) {
    if let Ok(artists) = db::get_artists_needing_artwork(conn) {
        for artist_name in artists {
            if let Some(url) = orca_core::online_artwork::fetch_itunes_artist_image(&artist_name)
                .or_else(|| orca_core::online_artwork::fetch_deezer_artist_image(&artist_name))
            {
                let prefix = format!("artist_{}", safe_name(&artist_name));
                if let Ok(paths) = orca_core::online_artwork::download_and_cache(&url, cache_base, &prefix) {
                    let _ = db::update_artist_artwork(
                        conn,
                        &artist_name,
                        Some(&paths.full),
                        Some(&paths.thumb),
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}

fn fetch_missing_album_artwork(conn: &rusqlite::Connection, cache_base: &std::path::Path) {
    if let Ok(albums) = db::get_albums_needing_artwork(conn) {
        for (album_key, album_title, album_artist) in albums {
            if let Some(url) = orca_core::online_artwork::fetch_itunes_album_art(&album_artist, &album_title) {
                let prefix = format!("album_{}", safe_name(&album_key));
                if let Ok(paths) = orca_core::online_artwork::download_and_cache(&url, cache_base, &prefix) {
                    let _ = db::update_album_artwork(
                        conn,
                        &album_key,
                        Some(&paths.full),
                        Some(&paths.thumb),
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}
