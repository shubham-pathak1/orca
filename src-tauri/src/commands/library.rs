use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
        Arc,
    },
    time::Duration,
};

use orca_core::{audio_engine::AudioCommand, db, library::LocalSong};
use serde::Serialize;
use tauri::{Emitter, State};

use crate::state::{
    normalize_path, playback_snapshot_from, LibrarySnapshot, LibraryWatchMessage, OrcaState,
    SharedOrcaState,
};

const SETTING_LIBRARY_SCAN_ROOTS: &str = "library_scan_roots";
const SCAN_SONG_BATCH_SIZE: usize = 96;
const SCAN_SONG_BATCH_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibrarySource {
    path: String,
    available: bool,
    song_count: usize,
}

fn stored_scan_roots(state: &OrcaState) -> Vec<PathBuf> {
    db::get_setting(&state.db_conn, SETTING_LIBRARY_SCAN_ROOTS)
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .map(normalize_path)
        .collect()
}

fn load_scan_roots(state: &OrcaState) -> Vec<PathBuf> {
    stored_scan_roots(state)
        .into_iter()
        .filter(|path| path.exists() && path.is_dir())
        .collect()
}

fn available_scan_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter(|path| path.exists() && path.is_dir())
        .cloned()
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

fn notify_library_watcher(state: &OrcaState, roots: Vec<PathBuf>) {
    let _ = state
        .library_watch_tx
        .send(LibraryWatchMessage::UpdateRoots(roots));
}

pub(crate) fn watcher_roots(state: &OrcaState) -> Vec<PathBuf> {
    load_scan_roots(state)
}

fn add_scan_root(state: &OrcaState, folder: PathBuf) -> Result<Vec<PathBuf>, String> {
    let folder = normalize_path(folder);
    let mut roots = stored_scan_roots(state);
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

fn is_song_in_root(song: &LocalSong, root: &std::path::Path) -> bool {
    PathBuf::from(&song.path).starts_with(root)
}

fn merge_scanned_with_unavailable_sources(
    scanned: Vec<LocalSong>,
    existing: &[LocalSong],
    roots: &[PathBuf],
    available_roots: &[PathBuf],
) -> Vec<LocalSong> {
    let unavailable_roots = roots
        .iter()
        .filter(|root| !available_roots.iter().any(|available| available == *root))
        .collect::<Vec<_>>();
    let mut songs_by_path = HashMap::new();

    for song in existing {
        if unavailable_roots
            .iter()
            .any(|root| is_song_in_root(song, root))
        {
            songs_by_path.insert(song.path.clone(), song.clone());
        }
    }
    for song in scanned {
        songs_by_path.insert(song.path.clone(), song);
    }

    songs_by_path.into_values().collect()
}

fn stop_playback_if_track_was_removed(state: &OrcaState) {
    let current_path = state
        .playback_state
        .lock()
        .ok()
        .and_then(|playback| playback.current_path.clone());
    let Some(current_path) = current_path else {
        return;
    };

    if state.songs.iter().any(|song| song.path == current_path) {
        return;
    }

    let _ = state.audio_tx.send(AudioCommand::Stop);
    if let Ok(mut playback) = state.playback_state.lock() {
        playback.current_path = None;
        playback.position_ms = 0;
        playback.duration_ms = 0;
        playback.is_playing = false;
    }
}

fn infer_scan_roots(songs: &[LocalSong]) -> Vec<PathBuf> {
    let mut directories = songs
        .iter()
        .filter_map(|song| {
            PathBuf::from(&song.path)
                .parent()
                .map(|path| normalize_path(path.to_path_buf()))
        })
        .filter(|path| path.exists() && path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories.dedup();

    if let Some(common) = common_ancestor(&directories) {
        if common.components().count() > 2 && common.exists() && common.is_dir() {
            return vec![common];
        }
    }

    directories.sort_by_key(|path| path.components().count());
    let mut roots = Vec::new();
    for directory in directories {
        if !roots
            .iter()
            .any(|root: &PathBuf| directory.starts_with(root))
        {
            roots.push(directory);
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

fn scan_roots<F, G>(
    roots: Vec<PathBuf>,
    artwork_dir: PathBuf,
    existing_map: std::collections::HashMap<String, (i64, u64, LocalSong)>,
    on_progress: F,
    on_song_indexed: G,
) -> Result<Vec<LocalSong>, String>
where
    F: Fn() + Send + Sync + Clone,
    G: Fn(LocalSong) + Send + Sync + Clone,
{
    let mut songs = Vec::new();
    let mut seen_paths = HashSet::new();
    for root in roots {
        let scanned = orca_core::scanner::scan_music_folder(
            &root,
            &artwork_dir,
            &existing_map,
            on_progress.clone(),
            on_song_indexed.clone(),
        )?;
        for song in scanned {
            if seen_paths.insert(song.path.clone()) {
                songs.push(song);
            }
        }
    }
    Ok(songs)
}

pub(crate) fn snapshot_from_state(state: &OrcaState) -> Result<LibrarySnapshot, String> {
    Ok(LibrarySnapshot {
        songs: state.songs.clone(),
        playlists: db::get_playlists(&state.db_conn)?,
        artists: db::get_artists(&state.db_conn)?,
        albums: db::get_albums(&state.db_conn)?,
        genres: db::get_genres(&state.db_conn)?,
        playback: playback_snapshot_from(state),
        folder_count: stored_scan_roots(state).len(),
    })
}

pub(crate) fn refresh_edited_song(
    state: &mut OrcaState,
    path: PathBuf,
) -> Result<LibrarySnapshot, String> {
    let song = orca_core::library::scan_music_file(&path, &state.artwork_dir)?;
    db::save_songs_to_db(&state.db_conn, &[song])?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    snapshot_from_state(state)
}

pub(crate) fn library_snapshot(
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    snapshot_from_state(&state)
}

pub(crate) fn library_folder_count(state: State<'_, SharedOrcaState>) -> Result<usize, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(stored_scan_roots(&state).len())
}

pub(crate) fn library_scan_roots(state: State<'_, SharedOrcaState>) -> Result<Vec<String>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(load_scan_roots(&state)
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect())
}

pub(crate) fn library_sources(
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<LibrarySource>, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    Ok(stored_scan_roots(&state)
        .into_iter()
        .map(|root| LibrarySource {
            song_count: state
                .songs
                .iter()
                .filter(|song| is_song_in_root(song, &root))
                .count(),
            available: root.exists() && root.is_dir(),
            path: root.to_string_lossy().to_string(),
        })
        .collect())
}

pub(crate) async fn remove_library_scan_root(
    root: String,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let target = normalize_path(PathBuf::from(root));
    let (artwork_dir, roots, active_roots, existing_map, existing_songs) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let mut roots = stored_scan_roots(&state);
        roots.retain(|candidate| normalize_path(candidate.clone()) != target);
        let map = db::get_existing_songs_map(&state.db_conn)?;
        let active_roots = available_scan_roots(&roots);
        (
            state.artwork_dir.clone(),
            roots,
            active_roots,
            map,
            state.songs.clone(),
        )
    };

    let scanned = scan_with_progress(app, active_roots.clone(), artwork_dir, existing_map).await?;
    let scanned = merge_scanned_with_unavailable_sources(
        scanned,
        &existing_songs,
        &roots,
        &active_roots,
    );
    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    persist_scan_roots(&state, &roots)?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    stop_playback_if_track_was_removed(&state);
    notify_library_watcher(&state, active_roots);
    snapshot_from_state(&state)
}

pub(crate) async fn pick_and_scan_folder(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let Some(folder) = rfd::FileDialog::new().pick_folder() else {
        return Err("Folder selection cancelled".to_string());
    };

    let (artwork_dir, roots, active_roots, existing_map, existing_songs) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let roots = add_scan_root(&state, folder)?;
        let map = db::get_existing_songs_map(&state.db_conn)?;
        let active_roots = available_scan_roots(&roots);
        (
            state.artwork_dir.clone(),
            roots,
            active_roots,
            map,
            state.songs.clone(),
        )
    };

    let scanned = scan_with_progress(app, active_roots.clone(), artwork_dir, existing_map).await?;
    let scanned = merge_scanned_with_unavailable_sources(
        scanned,
        &existing_songs,
        &roots,
        &active_roots,
    );
    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    stop_playback_if_track_was_removed(&state);
    notify_library_watcher(&state, active_roots);
    snapshot_from_state(&state)
}

pub(crate) async fn rescan_library(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let (artwork_dir, roots, active_roots, existing_map, existing_songs) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        let mut roots = stored_scan_roots(&state);
        if roots.is_empty() {
            roots = infer_scan_roots(&state.songs);
            if !roots.is_empty() {
                persist_scan_roots(&state, &roots)?;
            }
        }
        let active_roots = available_scan_roots(&roots);
        if active_roots.is_empty() {
            return Err("No known library folder yet. Add a folder first.".to_string());
        }

        let map = db::get_existing_songs_map(&state.db_conn)?;
        (
            state.artwork_dir.clone(),
            roots,
            active_roots,
            map,
            state.songs.clone(),
        )
    };

    let scanned = scan_with_progress(app, active_roots.clone(), artwork_dir, existing_map).await?;
    let scanned = merge_scanned_with_unavailable_sources(
        scanned,
        &existing_songs,
        &roots,
        &active_roots,
    );
    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    stop_playback_if_track_was_removed(&state);
    notify_library_watcher(&state, active_roots);
    snapshot_from_state(&state)
}

pub(crate) async fn rescan_library_source(
    root: String,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    let target = normalize_path(PathBuf::from(root));
    let (artwork_dir, existing_map, existing_songs) = {
        let state = state.0.lock().map_err(|error| error.to_string())?;
        if !stored_scan_roots(&state).iter().any(|root| root == &target) {
            return Err("This folder is no longer part of the library.".to_string());
        }
        if !target.exists() || !target.is_dir() {
            return Err("This music folder is currently unavailable.".to_string());
        }
        (
            state.artwork_dir.clone(),
            db::get_existing_songs_map(&state.db_conn)?,
            state.songs.clone(),
        )
    };

    let scanned = scan_with_progress(app, vec![target.clone()], artwork_dir, existing_map).await?;
    let mut merged = existing_songs
        .into_iter()
        .filter(|song| !is_song_in_root(song, &target))
        .collect::<Vec<_>>();
    merged.extend(scanned);

    let mut state = state.0.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &merged)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    stop_playback_if_track_was_removed(&state);
    snapshot_from_state(&state)
}

async fn scan_with_progress(
    app: tauri::AppHandle,
    roots: Vec<PathBuf>,
    artwork_dir: PathBuf,
    existing_map: std::collections::HashMap<String, (i64, u64, LocalSong)>,
) -> Result<Vec<LocalSong>, String> {
    let progress = Arc::new(AtomicUsize::new(0));
    let progress_clone = Arc::clone(&progress);
    let progress_app = app.clone();
    let on_progress = move || {
        let scanned = progress_clone.fetch_add(1, Ordering::Relaxed) + 1;
        progress_app.emit("scan-progress", scanned).ok();
    };

    let (indexed_song_tx, indexed_song_rx) = mpsc::channel::<LocalSong>();
    let batch_app = app.clone();
    let batch_worker = std::thread::spawn(move || {
        let mut batch = Vec::with_capacity(SCAN_SONG_BATCH_SIZE);
        let flush = |batch: &mut Vec<LocalSong>| {
            if !batch.is_empty() {
                batch_app.emit("scan-song-batch", std::mem::take(batch)).ok();
            }
        };

        loop {
            match indexed_song_rx.recv_timeout(SCAN_SONG_BATCH_INTERVAL) {
                Ok(song) => {
                    batch.push(song);
                    if batch.len() >= SCAN_SONG_BATCH_SIZE {
                        flush(&mut batch);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => flush(&mut batch),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    flush(&mut batch);
                    break;
                }
            }
        }
    });

    let scanned = tauri::async_runtime::spawn_blocking(move || {
        let on_song_indexed = move |song| {
            let _ = indexed_song_tx.send(song);
        };
        scan_roots(roots, artwork_dir, existing_map, on_progress, on_song_indexed)
    })
    .await
    .map_err(|error| error.to_string())?;

    let _ = batch_worker.join();
    scanned
}

pub(crate) fn rescan_watched_library(
    app: &tauri::AppHandle,
    shared_state: &std::sync::Arc<std::sync::Mutex<OrcaState>>,
) -> Result<(), String> {
    let (artwork_dir, roots, existing_map) = {
        let state = shared_state.lock().map_err(|error| error.to_string())?;
        let roots = load_scan_roots(&state);
        if roots.is_empty() {
            return Ok(());
        }
        let existing_map = db::get_existing_songs_map(&state.db_conn)?;
        (state.artwork_dir.clone(), roots, existing_map)
    };

    let progress = Arc::new(AtomicUsize::new(0));
    let progress_clone = Arc::clone(&progress);
    let progress_app = app.clone();
    let on_progress = move || {
        let scanned = progress_clone.fetch_add(1, Ordering::Relaxed) + 1;
        progress_app.emit("scan-progress", scanned).ok();
    };
    let scanned = scan_roots(roots, artwork_dir, existing_map, on_progress, |_| {})?;

    let mut state = shared_state.lock().map_err(|error| error.to_string())?;
    db::replace_songs_in_db(&state.db_conn, &scanned)?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    app.emit("library-watcher-refreshed", ()).ok();
    Ok(())
}

pub(crate) fn refresh_watched_library(
    app: &tauri::AppHandle,
    shared_state: &std::sync::Arc<std::sync::Mutex<OrcaState>>,
    changed_paths: Vec<PathBuf>,
) -> Result<(), String> {
    let (artwork_dir, roots, existing_songs) = {
        let state = shared_state.lock().map_err(|error| error.to_string())?;
        (
            state.artwork_dir.clone(),
            stored_scan_roots(&state),
            state.songs.clone(),
        )
    };
    if roots.is_empty() {
        return Ok(());
    }

    let existing_map = existing_songs
        .iter()
        .map(|song| {
            (
                song.path.clone(),
                (
                    song.modified_at.unwrap_or(0),
                    song.file_size.unwrap_or(0),
                    song.clone(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut updated_by_path = HashMap::new();
    let mut removed_paths = HashSet::new();

    for path in changed_paths {
        if !roots.iter().any(|root| path.starts_with(root)) {
            continue;
        }

        if path.is_file() {
            if orca_core::scanner::is_supported_audio_file(&path) {
                if let Ok(song) = orca_core::library::scan_music_file(&path, &artwork_dir) {
                    updated_by_path.insert(song.path.clone(), song);
                }
            } else if existing_map.contains_key(&path.to_string_lossy().to_string()) {
                removed_paths.insert(path.to_string_lossy().to_string());
            }
            continue;
        }

        if path.is_dir() {
            for song in
                orca_core::scanner::scan_music_folder(&path, &artwork_dir, &existing_map, || {}, |_| {})?
            {
                updated_by_path.insert(song.path.clone(), song);
            }
            continue;
        }

        for song in &existing_songs {
            if PathBuf::from(&song.path).starts_with(&path) {
                removed_paths.insert(song.path.clone());
            }
        }
    }

    removed_paths.retain(|path| !updated_by_path.contains_key(path));
    if updated_by_path.is_empty() && removed_paths.is_empty() {
        return Ok(());
    }

    let mut state = shared_state.lock().map_err(|error| error.to_string())?;
    db::apply_song_changes(
        &state.db_conn,
        &updated_by_path.into_values().collect::<Vec<_>>(),
        &removed_paths.into_iter().collect::<Vec<_>>(),
    )?;
    state.songs = db::get_all_songs(&state.db_conn)?;
    stop_playback_if_track_was_removed(&state);
    app.emit("library-watcher-refreshed", ()).ok();
    Ok(())
}
