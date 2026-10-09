use super::read_text;
use crate::Backend;
use orca_core::db;
use std::{path::Path, sync::atomic::Ordering};

impl Backend {
    pub fn read_lyrics_file(&self, path: &str) -> Result<String, String> {
        read_text(Path::new(path), 1024 * 1024)
            .map(|text| text.trim_start_matches('\u{feff}').into())
    }
    pub fn lyrics(&self, path: &str) -> Result<String, String> {
        self.track(path)?;
        // An explicitly saved empty cache is a removal marker, including for sidecars.
        if db::get_lyrics(&self.conn, path).is_some_and(|text| text.trim().is_empty()) {
            return Ok(String::new());
        }
        let local = Path::new(path).with_extension("lrc");
        if local.is_file() {
            let value = read_text(&local, 1024 * 1024)
                .map(|s| s.trim_start_matches('\u{feff}').to_string())
                .map_err(|e| e.to_string())?;
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
        if let Some(value) = db::get_lyrics(&self.conn, path).filter(|s| !s.trim().is_empty()) {
            return Ok(value);
        }
        self.conn
            .query_row(
                "SELECT COALESCE(lyrics,'') FROM songs WHERE path=?1",
                [path],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }

    pub fn fetch_missing_lyrics(&self, path: &str) -> Result<String, String> {
        self.fetch_missing_lyrics_cancellable(path, None)
    }
    pub fn fetch_missing_lyrics_cancellable(
        &self,
        path: &str,
        stop: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<String, String> {
        let cached = self.lyrics(path)?;
        if !cached.trim().is_empty() {
            return Ok(cached);
        }
        if db::get_lyrics(&self.conn, path).is_some_and(|text| text.trim().is_empty()) {
            return Ok(String::new());
        }
        let track = self.track(path)?;
        let fetched = orca_core::lyrics::fetch_lyrics_with_album_cancellable(
            &track.title,
            &track.artist,
            &track.album,
            track.duration_ms,
            stop,
        )?;
        if stop.is_some_and(|s| s.load(Ordering::Relaxed)) {
            return Err("Lyrics search cancelled".into());
        }
        // A metadata edit may have supplied lyrics while the network was pending.
        let current = self.lyrics(path)?;
        if !current.trim().is_empty() {
            return Ok(current);
        }
        // Serialize the final cache decision with metadata saves on other connections.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(|e| e.to_string())?;
        if let Some(current) = db::get_lyrics(&tx, path) {
            return Ok(current);
        }
        db::set_lyrics(&tx, path, &fetched)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(fetched)
    }
}
