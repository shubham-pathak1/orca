use super::read_text;
use crate::types::Track;
use crate::Backend;
use orca_core::db;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::atomic::Ordering,
};

const SESSION_LYRICS_LIMIT: usize = 256;

/// Read original LRC from cache entries written by the retired rich-lyrics
/// feature. This compatibility reader never fetches or renders extra text.
pub fn original_lyrics(text: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Legacy {
        orca_lyrics: u8,
        original_lrc: String,
    }
    if text.len() <= 1024 * 1024 && text.trim_start().starts_with('{') {
        if let Ok(old) = serde_json::from_str::<Legacy>(text) {
            if old.orca_lyrics == 1 {
                return old.original_lrc;
            }
        }
    }
    text.to_owned()
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn retired_cache_entries_keep_original_lrc_and_empty_misses_expire() {
        let old = serde_json::json!({"orca_lyrics":1,"original_lrc":"[00:01.00]Original","translations":["unused"]}).to_string();
        assert_eq!(original_lyrics(&old), "[00:01.00]Original");
        assert_eq!(
            original_lyrics("[00:02.00]Local edit"),
            "[00:02.00]Local edit"
        );
        let track = Track {
            path: "test.wav".into(),
            title: "Song".into(),
            ..Default::default()
        };
        let mut cache = SessionLyrics::default();
        cache.insert(&track, String::new());
        assert_eq!(cache.get(&track), Some(String::new()));
        cache.entries.get_mut("test.wav").unwrap().fetched_at =
            std::time::Instant::now() - std::time::Duration::from_secs(31);
        assert!(cache.get(&track).is_none());
        cache.insert(&track, "[00:01.00]Found".into());
        cache.entries.get_mut("test.wav").unwrap().fetched_at =
            std::time::Instant::now() - std::time::Duration::from_secs(31);
        assert_eq!(cache.get(&track).as_deref(), Some("[00:01.00]Found"));
    }
}

#[derive(Default)]
pub(crate) struct SessionLyrics {
    entries: HashMap<String, SessionLyricEntry>,
    order: VecDeque<String>,
}

struct SessionLyricEntry {
    fetched_at: std::time::Instant,
    title: String,
    artist: String,
    album: String,
    duration_ms: u64,
    lyrics: String,
    replaced_saved: Option<String>,
}

impl SessionLyrics {
    fn get(&self, track: &Track) -> Option<String> {
        self.entries.get(&track.path).and_then(|entry| {
            ((!entry.lyrics.is_empty()
                || entry.fetched_at.elapsed() < std::time::Duration::from_secs(30))
                && entry.title == track.title
                && entry.artist == track.artist
                && entry.album == track.album
                && entry.duration_ms == track.duration_ms)
                .then(|| entry.lyrics.clone())
        })
    }

    fn override_for(&self, track: &Track, saved: &str) -> Option<String> {
        self.entries.get(&track.path).and_then(|entry| {
            (entry.title == track.title
                && entry.artist == track.artist
                && entry.album == track.album
                && entry.duration_ms == track.duration_ms
                && entry.replaced_saved.as_deref() == Some(saved))
            .then(|| entry.lyrics.clone())
        })
    }

    fn insert(&mut self, track: &Track, lyrics: String) {
        self.insert_with_baseline(track, lyrics, None);
    }

    fn insert_with_baseline(
        &mut self,
        track: &Track,
        lyrics: String,
        replaced_saved: Option<String>,
    ) {
        if self.entries.contains_key(&track.path) {
            self.order.retain(|key| key != &track.path);
        }
        self.order.push_back(track.path.clone());
        self.entries.insert(
            track.path.clone(),
            SessionLyricEntry {
                fetched_at: std::time::Instant::now(),
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_ms: track.duration_ms,
                lyrics,
                replaced_saved,
            },
        );
        while self.entries.len() > SESSION_LYRICS_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

impl Backend {
    pub fn read_lyrics_file(&self, path: &str) -> Result<String, String> {
        read_text(Path::new(path), 1024 * 1024)
            .map(|text| text.trim_start_matches('\u{feff}').into())
    }
    pub(crate) fn saved_lyrics(&self, path: &str) -> Result<String, String> {
        self.track(path)?;
        // An explicit clear hides sidecars and any older automatic result.
        if db::lyrics_suppressed(&self.conn, path) {
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
            return Ok(original_lyrics(&value));
        }
        let embedded: String = self
            .conn
            .query_row(
                "SELECT COALESCE(lyrics,'') FROM active_songs WHERE path=?1",
                [path],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(embedded)
    }

    pub fn lyrics(&self, path: &str) -> Result<String, String> {
        let saved = self.saved_lyrics(path)?;
        if db::lyrics_suppressed(&self.conn, path) {
            return Ok(saved);
        }
        let track = self.track(path)?;
        let session = self
            .session_lyrics
            .lock()
            .map_err(|_| "Lyrics session cache unavailable")?;
        if let Some(refreshed) = session.override_for(&track, &saved) {
            return Ok(refreshed);
        }
        if !saved.trim().is_empty() {
            return Ok(saved);
        }
        Ok(session.get(&track).unwrap_or_default())
    }

    pub fn fetch_missing_lyrics(&self, path: &str) -> Result<String, String> {
        self.fetch_missing_lyrics_cancellable(path, None)
    }
    pub fn fetch_missing_lyrics_cancellable(
        &self,
        path: &str,
        stop: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<String, String> {
        self.fetch_missing_lyrics_with(
            path,
            stop,
            orca_core::lyrics::fetch_lyrics_with_album_cancellable,
        )
    }

    pub fn refresh_lyrics_cancellable(
        &self,
        path: &str,
        stop: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<String, String> {
        self.refresh_lyrics_with(
            path,
            stop,
            orca_core::lyrics::fetch_lyrics_with_album_cancellable,
        )
    }

    pub(crate) fn refresh_lyrics_with(
        &self,
        path: &str,
        stop: Option<&std::sync::atomic::AtomicBool>,
        lookup: impl FnOnce(
            &str,
            &str,
            &str,
            u64,
            Option<&std::sync::atomic::AtomicBool>,
        ) -> Result<String, String>,
    ) -> Result<String, String> {
        let track = self.track(path)?;
        if db::lyrics_suppressed(&self.conn, path) {
            return Ok(String::new());
        }
        let saved = self.saved_lyrics(path)?;
        let fetched = lookup(
            &track.title,
            &track.artist,
            &track.album,
            track.duration_ms,
            stop,
        )?;
        if stop.is_some_and(|s| s.load(Ordering::Relaxed)) {
            return Err("Lyrics search cancelled".into());
        }
        let current = self.track(path)?;
        if current.title != track.title
            || current.artist != track.artist
            || current.album != track.album
            || current.duration_ms != track.duration_ms
        {
            return Err("Track changed during lyrics search".into());
        }
        if db::lyrics_suppressed(&self.conn, path) {
            return Ok(String::new());
        }
        let current_saved = self.saved_lyrics(path)?;
        if current_saved != saved {
            return Ok(current_saved);
        }
        if !fetched.trim().is_empty() {
            self.session_lyrics
                .lock()
                .map_err(|_| "Lyrics session cache unavailable")?
                .insert_with_baseline(&track, fetched.clone(), Some(saved));
        }
        Ok(fetched)
    }

    pub(crate) fn fetch_missing_lyrics_with(
        &self,
        path: &str,
        stop: Option<&std::sync::atomic::AtomicBool>,
        lookup: impl FnOnce(
            &str,
            &str,
            &str,
            u64,
            Option<&std::sync::atomic::AtomicBool>,
        ) -> Result<String, String>,
    ) -> Result<String, String> {
        // An explicit clear is intentional, even if an old sidecar is present.
        if db::lyrics_suppressed(&self.conn, path) {
            self.track(path)?;
            return Ok(String::new());
        }
        let cached = self.lyrics(path)?;
        if !cached.trim().is_empty() {
            return Ok(cached);
        }
        let track = self.track(path)?;
        if let Some(checked) = self
            .session_lyrics
            .lock()
            .map_err(|_| "Lyrics session cache unavailable")?
            .get(&track)
        {
            return Ok(checked);
        }
        let result = lookup(
            &track.title,
            &track.artist,
            &track.album,
            track.duration_ms,
            stop,
        );
        if stop.is_some_and(|s| s.load(Ordering::Relaxed)) {
            return Err("Lyrics search cancelled".into());
        }
        // Don't attach a response to a song whose identity changed during lookup.
        let current_track = self.track(path)?;
        if current_track.title != track.title
            || current_track.artist != track.artist
            || current_track.album != track.album
            || current_track.duration_ms != track.duration_ms
        {
            return Err("Track changed during lyrics search".into());
        }
        if db::lyrics_suppressed(&self.conn, path) {
            return Ok(String::new());
        }
        // A sidecar, tag, or user save may have arrived while the request ran.
        let current = self.lyrics(path)?;
        if !current.trim().is_empty() {
            return Ok(current);
        }
        let fetched = match result {
            Ok(text) => text,
            Err(error)
                if error == "Lyrics not found"
                    || error == "No lyrics found for a matching recording" =>
            {
                String::new()
            }
            Err(error) => return Err(error),
        };
        self.session_lyrics
            .lock()
            .map_err(|_| "Lyrics session cache unavailable")?
            .insert(&track, fetched.clone());
        Ok(fetched)
    }
}
