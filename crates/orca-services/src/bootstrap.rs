//! Database import and backend initialization; import never overwrites a library.
use super::{Backend, ScanState};
use orca_core::{audio_engine, audio_engine::PlaybackState, db};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
mod artwork_migration;
/// Import committed WAL pages without replacing an existing destination.
/// Referenced artwork is copied into the native profile on backend startup.
pub fn import_library(source_db: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let database = destination.join("orca.db");
    if database.exists() {
        return Ok(());
    }
    let source = Connection::open_with_flags(source_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    source
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let temporary = destination.join("orca.import.tmp");
    source
        .backup(rusqlite::DatabaseName::Main, &temporary, None)
        .map_err(|e| e.to_string())?;
    fs::rename(temporary, database).map_err(|e| e.to_string())
}

pub fn prepare_data_dir(data_dir: &str) -> Result<(), String> {
    let directory = Path::new(data_dir);
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let database = directory.join("orca.db");
    let backup = directory.join("orca.pre-qt-backup.db");
    if !database.exists() || backup.exists() {
        return Ok(());
    }
    let source = Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    source
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let temporary = directory.join(format!("orca.pre-qt-backup.{}.tmp", std::process::id()));
    source
        .backup(rusqlite::DatabaseName::Main, &temporary, None)
        .map_err(|e| e.to_string())?;
    if backup.exists() {
        fs::remove_file(temporary).map_err(|e| e.to_string())?;
    } else {
        fs::rename(temporary, backup).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn new_backend(data_dir: &str, audio: bool) -> Result<Box<Backend>, String> {
    let data_dir = PathBuf::from(data_dir);
    fs::create_dir_all(data_dir.join("artwork")).map_err(|e| e.to_string())?;
    let conn = db::init_db(data_dir.clone())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    super::media::recover_metadata(&conn, &data_dir)?;
    artwork_migration::localize(&conn, &data_dir)?;
    let ended = Arc::new(AtomicU64::new(0));
    let transitioned = Arc::new(AtomicU64::new(0));
    let (audio_tx, playback, audio_thread) = if audio {
        let ended = ended.clone();
        let transitioned = transitioned.clone();
        let (tx, state, _, handle) =
            audio_engine::spawn_audio_thread_managed(Some(move |event: &str, _| match event {
                "playback-ended" => {
                    ended.fetch_add(1, Ordering::Relaxed);
                }
                "track-transitioned" => {
                    transitioned.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }));
        (Some(tx), state, Some(handle))
    } else {
        (None, Arc::new(Mutex::new(PlaybackState::default())), None)
    };
    Ok(Box::new(Backend {
        conn,
        data_dir,
        scan: Arc::new(ScanState::default()),
        scan_thread: None,
        audio_tx,
        playback,
        audio_thread,
        ended,
        transitioned,
    }))
}
