use std::{
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

use orca_core::{
    audio_engine::{self, AudioCommand, PlaybackState},
    db,
    library::SongMetadataUpdate,
};
use tauri::{Emitter, Manager, State};

mod commands;
mod state;

use commands::library::{refresh_edited_song, snapshot_from_state};
use state::{
    artwork_dir, load_state, playback_snapshot_from, LibrarySnapshot, SharedOrcaState,
};

#[tauri::command]
fn library_snapshot(state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    commands::library::library_snapshot(state)
}

#[tauri::command]
fn library_folder_count(state: State<'_, SharedOrcaState>) -> Result<usize, String> {
    commands::library::library_folder_count(state)
}

#[tauri::command]
fn library_scan_roots(state: State<'_, SharedOrcaState>) -> Result<Vec<String>, String> {
    commands::library::library_scan_roots(state)
}

#[tauri::command]
async fn remove_library_scan_root(
    root: String,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::remove_library_scan_root(root, app, state).await
}

#[tauri::command]
fn list_playlists(state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn create_playlist(
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

#[tauri::command]
fn rename_playlist(
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

#[tauri::command]
fn delete_playlist(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::delete_playlist(&state.db_conn, playlist_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn add_song_to_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::add_to_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn remove_song_from_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_from_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn playlist_song_ids(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<i64>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlist_song_ids(&state.db_conn, playlist_id)
}

#[tauri::command]
fn choose_playlist_cover(
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

#[tauri::command]
fn remove_playlist_cover(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_playlist_cover(&state.db_conn, playlist_id, None)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn choose_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let Some(image_path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
    else {
        return Err("Cover selection cancelled".to_string());
    };

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_artist_artwork(
        &state.db_conn,
        &artist_name,
        Some(&image_path.to_string_lossy()),
        None,
    )?;
    snapshot_from_state(&state)
}

#[tauri::command]
fn remove_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_artist_artwork(&state.db_conn, &artist_name)?;
    snapshot_from_state(&state)
}

#[tauri::command]
fn choose_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let Some(image_path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
    else {
        return Err("Cover selection cancelled".to_string());
    };

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_album_artwork(
        &state.db_conn,
        &album_key,
        Some(&image_path.to_string_lossy()),
        None,
    )?;
    snapshot_from_state(&state)
}

#[tauri::command]
async fn remove_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_album_artwork(&state.db_conn, &album_key)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(&state)
}

#[tauri::command]
async fn fetch_artist_artwork_manual(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let url = orca_core::online_artwork::fetch_itunes_artist_image(&artist_name)
        .or_else(|| orca_core::online_artwork::fetch_deezer_artist_image(&artist_name));
    let url = url.ok_or_else(|| "Artist image not found online".to_string())?;
    
    let cache_dir = artwork_dir().join("online");
    let safe_name = artist_name.replace(|c: char| !c.is_alphanumeric(), "_");
    let prefix = format!("artist_{}", safe_name);
    
    let paths = orca_core::online_artwork::download_and_cache(&url, &cache_dir, &prefix)?;
    
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

#[tauri::command]
async fn fetch_album_artwork_manual(
    album_key: String,
    artist: String,
    album: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let url = orca_core::online_artwork::fetch_itunes_album_art(&artist, &album);
    let url = url.ok_or_else(|| "Album art not found online".to_string())?;
    
    let cache_dir = artwork_dir().join("online");
    let safe_name = album_key.replace(|c: char| !c.is_alphanumeric(), "_");
    let prefix = format!("album_{}", safe_name);
    
    let paths = orca_core::online_artwork::download_and_cache(&url, &cache_dir, &prefix)?;
    
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

#[tauri::command]
async fn fetch_all_missing_artwork(
    state: State<'_, SharedOrcaState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Emitter;
    // This runs on a separate thread so we don't block the UI
    let state_clone = state.0.lock().map_err(|e| e.to_string())?;
    let db_conn_path = state_clone.db_conn.path().map(|p| p.to_string());
    let cache_base = artwork_dir().join("online");
    
    if let Some(path) = db_conn_path {
        std::thread::spawn(move || {
            let conn = rusqlite::Connection::open(&path).ok()?;
            
            // 1. Fetch artists without artwork (tombstoned artists already excluded by query)
            if let Ok(artists) = db::get_artists_needing_artwork(&conn) {
                for artist_name in artists {
                    if let Some(url) = orca_core::online_artwork::fetch_itunes_artist_image(&artist_name)
                        .or_else(|| orca_core::online_artwork::fetch_deezer_artist_image(&artist_name)) {
                        let safe_name = artist_name.replace(|c: char| !c.is_alphanumeric(), "_");
                        let prefix = format!("artist_{}", safe_name);
                        if let Ok(paths) = orca_core::online_artwork::download_and_cache(&url, &cache_base, &prefix) {
                            let _ = db::update_artist_artwork(&conn, &artist_name, Some(&paths.full), Some(&paths.thumb));
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
            
            // 2. Fetch albums without artwork (tombstoned albums already excluded by query)
            if let Ok(albums) = db::get_albums_needing_artwork(&conn) {
                for (album_key, album_title, album_artist) in albums {
                    if let Some(url) = orca_core::online_artwork::fetch_itunes_album_art(&album_artist, &album_title) {
                        let safe_name = album_key.replace(|c: char| !c.is_alphanumeric(), "_");
                        let prefix = format!("album_{}", safe_name);
                        if let Ok(paths) = orca_core::online_artwork::download_and_cache(&url, &cache_base, &prefix) {
                            let _ = db::update_album_artwork(&conn, &album_key, Some(&paths.full), Some(&paths.thumb));
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
            
            let _ = app.emit("library-refreshed", ());
            Some(())
        });
    }
    
    Ok(())
}

#[tauri::command]
async fn update_song_metadata(
    update: SongMetadataUpdate,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let path = PathBuf::from(&update.path);
    let edit_path = path.clone();

    tauri::async_runtime::spawn_blocking(move || orca_core::library::update_song_metadata(update))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

#[tauri::command]
async fn choose_song_cover(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let Some(image_path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
    else {
        return Err("Cover selection cancelled".to_string());
    };

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

#[tauri::command]
async fn remove_song_cover(
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

#[tauri::command]
fn playback_snapshot(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(playback_snapshot_from(&state))
}

#[tauri::command]
async fn pick_and_scan_folder(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::pick_and_scan_folder(app, state).await
}

#[tauri::command]
async fn rescan_library(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::rescan_library(app, state).await
}

#[tauri::command]
fn play_song(path: String, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    if !std::path::Path::new(&path).exists() {
        return Err(
            "File not found. Your music folder may have moved — try rescanning.".to_string(),
        );
    }
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Play(path))
        .map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_millis(40));
    Ok(playback_snapshot_from(&state))
}

#[tauri::command]
fn queue_next_playback(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    if !std::path::Path::new(&path).exists() {
        return Err(
            "File not found. Your music folder may have moved — try rescanning.".to_string(),
        );
    }
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::QueueNext(path))
        .map_err(|error| error.to_string())?;
    Ok(playback_snapshot_from(&state))
}

#[tauri::command]
fn pause_playback(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
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

#[tauri::command]
fn resume_playback(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
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

#[tauri::command]
fn seek_playback(
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

#[tauri::command]
fn set_volume(volume: f32, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
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

#[tauri::command]
async fn waveform_peaks(
    path: String,
    buckets: usize,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<f32>, String> {
    // --- Cache read (lock briefly, then release) ---
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        if let Ok(Some(cached)) = db::get_cached_waveform(&guard.db_conn, &path, buckets) {
            return Ok(cached);
        }
    }

    // --- Cache miss: decode on a blocking thread ---
    let path_clone = path.clone();
    let peaks = tauri::async_runtime::spawn_blocking(move || {
        audio_engine::compute_waveform_peaks(&path_clone, buckets)
    })
    .await
    .map_err(|e| e.to_string())??;

    // --- Persist so next call is instant ---
    {
        let guard = state.0.lock().map_err(|e| e.to_string())?;
        // Non-fatal: if caching fails we still return the computed peaks.
        let _ = db::save_waveform(&guard.db_conn, &path, buckets, &peaks);
    }

    Ok(peaks)
}

#[derive(serde::Deserialize)]
pub struct MediaControlsUpdate {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<f64>,
    playing: bool,
    progress: Option<f64>,
    cover_url: Option<String>,
}

#[tauri::command]
fn update_media_controls(update: MediaControlsUpdate, state: State<'_, SharedOrcaState>) -> Result<(), String> {
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
            println!("Updating media controls metadata: {:?}", update.title);
            
            controls.set_metadata(metadata).map_err(|e| e.to_string())?;

            let progress = update.progress.map(|p| MediaPosition(Duration::from_secs_f64(p)));
            let playback = if update.playing {
                MediaPlayback::Playing { progress }
            } else {
                MediaPlayback::Paused { progress }
            };
            
            controls.set_playback(playback).map_err(|e| e.to_string())?;
        }
    }
    
    Ok(())
}

#[tauri::command]
fn pick_font_file() -> Result<String, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Font files", &["ttf", "otf", "woff", "woff2"])
        .set_title("Pick a font file")
        .pick_file()
    else {
        return Err("Font selection cancelled".to_string());
    };
    Ok(path.to_string_lossy().to_string())
}

pub fn run() {
    // Register the App User Model ID so Windows taskbar thumbnail buttons
    // work in both the installed version and the portable .exe.
    #[cfg(target_os = "windows")]
    {
        // use windows::core::PCWSTR;
        // use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        // let aumid: Vec<u16> = "dev.orca.player\0".encode_utf16().collect();
        // let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(aumid.as_ptr())) };
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_taskbar::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = load_state().map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            
            let mut state = state;

            // Initialize souvlaki MediaControls
            #[cfg(target_os = "windows")]
            {
                use souvlaki::{PlatformConfig, MediaControls, MediaControlEvent};
                use tauri::Manager;
                
                let hwnd = app.get_webview_window("main").unwrap().hwnd().unwrap().0 as *mut _;
                let config = PlatformConfig {
                    dbus_name: "orca",
                    display_name: "Orca",
                    hwnd: Some(hwnd),
                };
                
                match MediaControls::new(config) {
                    Ok(mut controls) => {
                        let app_handle = app.handle().clone();
                        controls.attach(move |event| {
                            match event {
                                MediaControlEvent::Play => { let _ = app_handle.emit("media-play", ()); }
                                MediaControlEvent::Pause => { let _ = app_handle.emit("media-pause", ()); }
                                MediaControlEvent::Toggle => { let _ = app_handle.emit("media-toggle", ()); }
                                MediaControlEvent::Next => { let _ = app_handle.emit("media-next", ()); }
                                MediaControlEvent::Previous => { let _ = app_handle.emit("media-prev", ()); }
                                _ => {}
                            }
                        }).ok();
                        
                        state.media_controls = Some(controls);
                        println!("Successfully initialized souvlaki MediaControls");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize souvlaki MediaControls: {:?}", e);
                    }
                }
            }

            app.manage(SharedOrcaState(Mutex::new(state)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_snapshot,
            library_folder_count,
            library_scan_roots,
            remove_library_scan_root,
            playback_snapshot,
            list_playlists,
            create_playlist,
            rename_playlist,
            delete_playlist,
            add_song_to_playlist,
            remove_song_from_playlist,
            playlist_song_ids,
            choose_playlist_cover,
            remove_playlist_cover,
            choose_artist_cover,
            remove_artist_cover,
            choose_album_cover,
            remove_album_cover,
            fetch_artist_artwork_manual,
            fetch_album_artwork_manual,
            fetch_all_missing_artwork,
            update_song_metadata,
            choose_song_cover,
            remove_song_cover,
            pick_and_scan_folder,
            rescan_library,
            play_song,
            queue_next_playback,
            pause_playback,
            resume_playback,
            seek_playback,
            waveform_peaks,
            playback_snapshot,
            update_media_controls,
            set_volume,
            pick_font_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
