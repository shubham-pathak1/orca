use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use orca_core::{
    audio_engine::{self, AudioCommand, PlaybackState, VisualizerData},
    db,
    library::{LocalSong, SongMetadataUpdate},
};
use tauri::{Manager, State};

const SETTING_LIBRARY_SCAN_ROOTS: &str = "library_scan_roots";

struct OrcaState {
    db_conn: rusqlite::Connection,
    artwork_dir: PathBuf,
    songs: Vec<LocalSong>,
    audio_tx: mpsc::Sender<AudioCommand>,
    playback_state: Arc<Mutex<PlaybackState>>,
    #[allow(dead_code)]
    visualizer_data: VisualizerData,
}

struct SharedOrcaState(Mutex<OrcaState>);

#[derive(serde::Serialize)]
struct LibrarySnapshot {
    songs: Vec<LocalSong>,
    playlists: Vec<db::Playlist>,
    playback: PlaybackState,
    folder_count: usize,
}

fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("orca")
}

fn artwork_dir() -> PathBuf {
    app_data_dir().join("artwork")
}

fn normalize_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn load_scan_roots(state: &OrcaState) -> Vec<PathBuf> {
    db::get_setting(&state.db_conn, SETTING_LIBRARY_SCAN_ROOTS)
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .map(normalize_path)
        .filter(|path| path.exists() && path.is_dir())
        .collect()
}

fn persist_scan_roots(state: &OrcaState, roots: &[PathBuf]) -> Result<(), String> {
    let encoded = serde_json::to_string(
        &roots
            .iter()
            .map(|path| normalize_path(path.clone()).to_string_lossy().to_string())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())?;
    db::set_setting(&state.db_conn, SETTING_LIBRARY_SCAN_ROOTS, &encoded)
}

fn add_scan_root(state: &OrcaState, folder: PathBuf) -> Result<Vec<PathBuf>, String> {
    let folder = normalize_path(folder);
    let mut roots = load_scan_roots(state);

    if roots.iter().any(|root| folder.starts_with(root)) {
        return Ok(roots);
    }

    roots.retain(|root| !root.starts_with(&folder));
    roots.push(folder);
    roots.sort();
    roots.dedup();
    persist_scan_roots(state, &roots)?;
    Ok(roots)
}

fn infer_scan_roots(songs: &[LocalSong]) -> Vec<PathBuf> {
    let mut dirs = songs
        .iter()
        .filter_map(|song| PathBuf::from(&song.path).parent().map(|path| normalize_path(path.to_path_buf())))
        .filter(|path| path.exists() && path.is_dir())
        .collect::<Vec<_>>();

    dirs.sort();
    dirs.dedup();

    if let Some(common) = common_ancestor(&dirs) {
        if common.components().count() > 2 && common.exists() && common.is_dir() {
            return vec![common];
        }
    }

    dirs.sort_by_key(|path| path.components().count());
    let mut roots = Vec::new();
    for dir in dirs {
        if !roots.iter().any(|root: &PathBuf| dir.starts_with(root)) {
            roots.push(dir);
        }
    }
    roots
}

fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut common = paths.first()?.clone();
    while !paths.iter().all(|path| path.starts_with(&common)) {
        if !common.pop() {
            return None;
        }
    }
    Some(common)
}

fn scan_roots(
    roots: Vec<PathBuf>,
    artwork_dir: PathBuf,
    existing_map: std::collections::HashMap<String, (i64, u64, LocalSong)>,
) -> Result<Vec<LocalSong>, String> {
    let mut songs = Vec::new();
    let mut seen_paths = HashSet::new();

    for root in roots {
        let scanned = orca_core::library::scan_music_folder(&root, &artwork_dir, &existing_map)?;
        for song in scanned {
            if seen_paths.insert(song.path.clone()) {
                songs.push(song);
            }
        }
    }

    Ok(songs)
}

fn playback_snapshot_from(state: &OrcaState) -> PlaybackState {
    state
        .playback_state
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default()
}

fn snapshot_from_state(state: &OrcaState) -> Result<LibrarySnapshot, String> {
    Ok(LibrarySnapshot {
        songs: state.songs.clone(),
        playlists: db::get_playlists(&state.db_conn)?,
        playback: playback_snapshot_from(state),
        folder_count: load_scan_roots(state).len(),
    })
}

fn refresh_edited_song(state: &mut OrcaState, path: PathBuf) -> Result<LibrarySnapshot, String> {
    let song = orca_core::library::scan_music_file(&path, &state.artwork_dir)?;
    db::save_songs_to_db(&state.db_conn, &[song])?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(state)
}

fn load_state() -> Result<OrcaState, String> {
    let app_dir = app_data_dir();
    let artwork_dir = artwork_dir();
    std::fs::create_dir_all(&app_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&artwork_dir).map_err(|error| error.to_string())?;

    let conn = db::init_db(app_dir)?;
    db::migrate_inline_artwork_to_files(&conn, &artwork_dir)?;
    let songs = db::get_all_songs(&conn)?;
    let (audio_tx, playback_state, visualizer_data) = audio_engine::spawn_audio_thread::<fn(&str, u64)>(None);

    Ok(OrcaState {
        db_conn: conn,
        artwork_dir,
        songs,
        audio_tx,
        playback_state,
        visualizer_data,
    })
}

#[tauri::command]
fn library_snapshot(state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    snapshot_from_state(&state)
}

#[tauri::command]
fn library_folder_count(state: State<'_, SharedOrcaState>) -> Result<usize, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(load_scan_roots(&state).len())
}

#[tauri::command]
fn library_scan_roots(state: State<'_, SharedOrcaState>) -> Result<Vec<String>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(load_scan_roots(&state)
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
async fn remove_library_scan_root(root: String, state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    let target = normalize_path(PathBuf::from(root));
    let (artwork_dir, roots, existing_map) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let mut roots = load_scan_roots(&state);
        roots.retain(|candidate| normalize_path(candidate.clone()) != target);
        let map = db::get_existing_songs_map(&state.db_conn)?;
        (state.artwork_dir.clone(), roots, map)
    };

    let scanned = if roots.is_empty() {
        Vec::new()
    } else {
        let remaining_roots = roots.clone();
        tauri::async_runtime::spawn_blocking(move || scan_roots(remaining_roots, artwork_dir, existing_map))
            .await
            .map_err(|error| error.to_string())??
    };

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    persist_scan_roots(&state, &roots)?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(&state)
}

#[tauri::command]
fn list_playlists(state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn create_playlist(name: String, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Playlist name cannot be empty".to_string());
    }

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::create_playlist(&state.db_conn, name, None)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn rename_playlist(playlist_id: i64, name: String, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Playlist name cannot be empty".to_string());
    }

    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::rename_playlist(&state.db_conn, playlist_id, name)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn delete_playlist(playlist_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::delete_playlist(&state.db_conn, playlist_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn add_song_to_playlist(playlist_id: i64, song_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::add_to_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn remove_song_from_playlist(playlist_id: i64, song_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::remove_from_playlist(&state.db_conn, playlist_id, song_id)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
fn playlist_song_ids(playlist_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<i64>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::get_playlist_song_ids(&state.db_conn, playlist_id)
}

#[tauri::command]
fn choose_playlist_cover(playlist_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
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
fn remove_playlist_cover(playlist_id: i64, state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    db::update_playlist_cover(&state.db_conn, playlist_id, None)?;
    db::get_playlists(&state.db_conn)
}

#[tauri::command]
async fn update_song_metadata(update: SongMetadataUpdate, state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    let path = PathBuf::from(&update.path);
    let edit_path = path.clone();

    tauri::async_runtime::spawn_blocking(move || orca_core::library::update_song_metadata(update))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

#[tauri::command]
async fn choose_song_cover(path: String, state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    let Some(image_path) = rfd::FileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .pick_file()
    else {
        return Err("Cover selection cancelled".to_string());
    };

    let song_path = PathBuf::from(path);
    let edit_path = song_path.clone();
    tauri::async_runtime::spawn_blocking(move || orca_core::library::replace_song_cover(&song_path, &image_path))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    refresh_edited_song(&mut state, edit_path)
}

#[tauri::command]
async fn remove_song_cover(path: String, state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
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
async fn pick_and_scan_folder(state: State<'_, SharedOrcaState>) -> Result<Vec<LocalSong>, String> {
    let Some(folder) = rfd::FileDialog::new().pick_folder() else {
        return Err("Folder selection cancelled".to_string());
    };

    let (artwork_dir, roots, existing_map) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let roots = add_scan_root(&state, folder)?;
        let map = db::get_existing_songs_map(&state.db_conn)?;
        (state.artwork_dir.clone(), roots, map)
    };

    let scanned = tauri::async_runtime::spawn_blocking(move || scan_roots(roots, artwork_dir, existing_map))
    .await
    .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    Ok(state.songs.clone())
}

#[tauri::command]
async fn rescan_library(state: State<'_, SharedOrcaState>) -> Result<Vec<LocalSong>, String> {
    let (artwork_dir, roots, existing_map) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let mut roots = load_scan_roots(&state);

        if roots.is_empty() {
            roots = infer_scan_roots(&state.songs);
            if !roots.is_empty() {
                persist_scan_roots(&state, &roots)?;
            }
        }

        if roots.is_empty() {
            return Err("No known library folder yet. Add a folder first.".to_string());
        }

        let map = db::get_existing_songs_map(&state.db_conn)?;
        (state.artwork_dir.clone(), roots, map)
    };

    let scanned = tauri::async_runtime::spawn_blocking(move || scan_roots(roots, artwork_dir, existing_map))
        .await
        .map_err(|error| error.to_string())??;

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    Ok(state.songs.clone())
}

#[tauri::command]
fn play_song(path: String, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .audio_tx
        .send(AudioCommand::Play(path))
        .map_err(|error| error.to_string())?;
    std::thread::sleep(Duration::from_millis(40));
    Ok(playback_snapshot_from(&state))
}

#[tauri::command]
fn queue_next_playback(path: String, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
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
fn seek_playback(position_ms: u64, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
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
async fn waveform_peaks(path: String, buckets: usize) -> Result<Vec<f32>, String> {
    tauri::async_runtime::spawn_blocking(move || audio_engine::compute_waveform_peaks(&path, buckets))
        .await
        .map_err(|error| error.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = load_state().map_err(|error| Box::<dyn std::error::Error>::from(error))?;
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
            set_volume,
            waveform_peaks
        ])
        .run(tauri::generate_context!())
        .expect("error while running Orca");
}
