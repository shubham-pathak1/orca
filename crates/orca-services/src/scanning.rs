//! Cooperative scanning and its joined worker lifecycle.
use super::{Backend, ScanState};
use orca_core::{db, library, scanner};
use rusqlite::params;
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};
impl Backend {
    pub fn start_scan(&mut self, folder: &str) -> Result<(), String> {
        if self.scan.active.load(Ordering::Acquire) {
            return Err("A scan is already running".into());
        }
        let folder = PathBuf::from(folder).canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Library folder is unavailable. Reconnect it, or remove it in Settings > Library."
                    .to_string()
            } else {
                e.to_string()
            }
        })?;
        if !folder.is_dir() {
            return Err("Choose a music folder".into());
        }
        if let Some(handle) = self.scan_thread.take() {
            handle.join().map_err(|_| "Scanner thread panicked")?;
        }
        let mut roots = self.statistics()?.roots;
        let root = folder.to_string_lossy().to_string();
        if !roots.contains(&root) {
            roots.push(root);
        }
        db::set_setting(
            &self.conn,
            "library_scan_roots",
            &serde_json::to_string(&roots).map_err(|e| e.to_string())?,
        )?;
        self.scan.cancel.store(false, Ordering::Release);
        self.scan.active.store(true, Ordering::Release);
        self.scan.count.store(0, Ordering::Relaxed);
        *self.scan.error.lock().map_err(|e| e.to_string())? = String::new();
        let data_dir = self.data_dir.clone();
        let scan = self.scan.clone();
        self.scan_thread = Some(thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                scan_folder(&data_dir, &folder, &scan)
            }))
            .unwrap_or_else(|_| Err("Scanner thread panicked".into()));
            if let Err(error) = result {
                if let Ok(mut value) = scan.error.lock() {
                    *value = error;
                }
            }
            scan.revision.fetch_add(1, Ordering::Relaxed);
            scan.active.store(false, Ordering::Release);
        }));
        Ok(())
    }

    pub fn cancel_scan(&self) {
        self.scan.cancel.store(true, Ordering::Release);
    }

    pub fn cancel_and_join_scan(&mut self) {
        self.cancel_scan();
        if let Some(handle) = self.scan_thread.take() {
            let _ = handle.join();
        }
    }
}

fn scan_folder(data_dir: &Path, folder: &Path, state: &ScanState) -> Result<(), String> {
    let conn = db::init_db(data_dir.to_path_buf())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let artwork = data_dir.join("artwork");
    // Amortize durable commits on large libraries while still publishing small
    // or slow scans promptly. Every committed batch remains transactional.
    const BATCH_SIZE: usize = 256;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut last_commit = Instant::now();
    let mut seen = std::collections::HashSet::new();
    for entry in walkdir::WalkDir::new(folder).follow_links(false) {
        if state.cancel.load(Ordering::Acquire) {
            break;
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() || !scanner::is_supported_audio_file(entry.path()) {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        seen.insert(path.clone());
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| t.as_secs() as i64)
            .unwrap_or(0);
        let unchanged = conn
            .query_row(
                "SELECT COUNT(*) FROM songs WHERE path=?1 AND modified_at=?2 AND file_size=?3",
                params![path, modified, metadata.len()],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())?
            > 0;
        if !unchanged {
            match library::scan_music_file(entry.path(), &artwork) {
                Ok(song) => batch.push(song),
                Err(error) => {
                    if let Ok(mut value) = state.error.lock() {
                        *value = format!("Skipped {path}: {error}");
                    }
                }
            }
        }
        state.count.fetch_add(1, Ordering::Relaxed);
        if batch.len() >= BATCH_SIZE
            || (!batch.is_empty() && last_commit.elapsed() >= Duration::from_millis(500))
        {
            db::save_songs_to_db(&conn, &batch)?;
            batch.clear();
            last_commit = Instant::now();
            state.revision.fetch_add(1, Ordering::Relaxed);
        }
    }
    if !batch.is_empty() {
        db::save_songs_to_db(&conn, &batch)?;
    }
    if !state.cancel.load(Ordering::Acquire) {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for (path, id) in db::get_song_path_index(&tx)? {
            if Path::new(&path).starts_with(folder) && !seen.contains(&path) {
                tx.execute("DELETE FROM playlist_songs WHERE song_id=?1", [id])
                    .map_err(|e| e.to_string())?;
                db::delete_song_by_path(&tx, &path)?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}
