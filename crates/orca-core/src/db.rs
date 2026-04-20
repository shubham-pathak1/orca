use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use rusqlite::{params, Connection};

use crate::library::LocalSong;

pub fn init_db(app_dir: PathBuf) -> Result<Connection, String> {
    let db_path = app_dir.join("orca.db");
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS songs (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            album_artist TEXT NOT NULL DEFAULT 'Unknown Artist',
            album TEXT NOT NULL DEFAULT 'Unknown Album',
            year INTEGER,
            track_number INTEGER,
            disc_number INTEGER,
            path TEXT NOT NULL UNIQUE,
            duration INTEGER NOT NULL,
            artwork_url TEXT,
            lyrics TEXT,
            sample_rate INTEGER,
            bitrate INTEGER,
            bit_depth INTEGER,
            format TEXT
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    // Migration: add lyrics column if missing
    let has_lyrics: bool = conn
        .prepare("SELECT lyrics FROM songs LIMIT 0")
        .is_ok();
    if !has_lyrics {
        conn.execute("ALTER TABLE songs ADD COLUMN lyrics TEXT", [])
            .ok();
    }

    // Migration: add album column if missing
    let has_album: bool = conn
        .prepare("SELECT album FROM songs LIMIT 0")
        .is_ok();
    if !has_album {
        conn.execute(
            "ALTER TABLE songs ADD COLUMN album TEXT NOT NULL DEFAULT 'Unknown Album'",
            [],
        )
        .ok();
    }

    let has_album_artist: bool = conn.prepare("SELECT album_artist FROM songs LIMIT 0").is_ok();
    if !has_album_artist {
        conn.execute(
            "ALTER TABLE songs ADD COLUMN album_artist TEXT NOT NULL DEFAULT 'Unknown Artist'",
            [],
        )
        .ok();
    }

    let has_year: bool = conn.prepare("SELECT year FROM songs LIMIT 0").is_ok();
    if !has_year {
        conn.execute("ALTER TABLE songs ADD COLUMN year INTEGER", []).ok();
    }

    let has_track_number: bool = conn.prepare("SELECT track_number FROM songs LIMIT 0").is_ok();
    if !has_track_number {
        conn.execute("ALTER TABLE songs ADD COLUMN track_number INTEGER", [])
            .ok();
    }

    let has_disc_number: bool = conn.prepare("SELECT disc_number FROM songs LIMIT 0").is_ok();
    if !has_disc_number {
        conn.execute("ALTER TABLE songs ADD COLUMN disc_number INTEGER", [])
            .ok();
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS lyrics (
            song_path TEXT PRIMARY KEY,
            lyrics_text TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS playlists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            cover_path TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS playlist_songs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            playlist_id INTEGER NOT NULL,
            song_id INTEGER NOT NULL,
            position INTEGER NOT NULL,
            FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY(song_id) REFERENCES songs(id) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    let has_quality: bool = conn.prepare("SELECT sample_rate FROM songs LIMIT 0").is_ok();
    if !has_quality {
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN sample_rate INTEGER", []);
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN bitrate INTEGER", []);
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN bit_depth INTEGER", []);
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN format TEXT", []);
    }

    enforce_song_path_uniqueness(&conn)?;
    enforce_playlist_name_uniqueness(&conn)?;

    Ok(conn)
}

fn enforce_song_path_uniqueness(conn: &Connection) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        conn.execute(
            "DELETE FROM songs
             WHERE id NOT IN (
                 SELECT MAX(id) FROM songs
                 GROUP BY lower(replace(replace(path, char(92), '/'), '//?/', ''))
             )",
            [],
        )
        .map_err(|e| e.to_string())?;

        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_songs_path_norm_unique
             ON songs(lower(replace(replace(path, char(92), '/'), '//?/', '')))",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute(
        "DELETE FROM songs
         WHERE id NOT IN (
             SELECT MAX(id) FROM songs GROUP BY path
         )",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_songs_path_unique ON songs(path)",
        [],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn enforce_playlist_name_uniqueness(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "DELETE FROM playlists
         WHERE id NOT IN (
             SELECT MAX(id) FROM playlists
             GROUP BY lower(trim(name))
         )",
        [],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_playlists_name_ci_unique
         ON playlists(lower(trim(name)))",
        [],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn save_songs_to_db(conn: &Connection, songs: &[LocalSong]) -> Result<(), String> {
    for song in songs {
        upsert_song(conn, song)?;
    }
    Ok(())
}

fn upsert_song(conn: &Connection, song: &LocalSong) -> Result<(), String> {
    conn.execute(
        "INSERT INTO songs (title, artist, album_artist, album, year, track_number, disc_number, path, duration, artwork_url, lyrics, sample_rate, bitrate, bit_depth, format)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT DO UPDATE SET
             title = excluded.title,
             artist = excluded.artist,
             album_artist = excluded.album_artist,
             album = excluded.album,
             year = excluded.year,
             track_number = excluded.track_number,
             disc_number = excluded.disc_number,
             path = excluded.path,
             duration = excluded.duration,
             artwork_url = COALESCE(excluded.artwork_url, songs.artwork_url),
             lyrics = COALESCE(excluded.lyrics, songs.lyrics),
             sample_rate = excluded.sample_rate,
             bitrate = excluded.bitrate,
             bit_depth = excluded.bit_depth,
             format = excluded.format",
        params![
            song.title,
            song.artist,
            song.album_artist,
            song.album,
            song.year,
            song.track_number,
            song.disc_number,
            song.path,
            song.duration,
            song.artwork,
            song.lyrics,
            song.sample_rate,
            song.bitrate,
            song.bit_depth,
            song.format
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_all_songs(conn: &Connection) -> Result<Vec<LocalSong>, String> {
    let mut stmt = conn
        .prepare("SELECT id, title, artist, album_artist, album, year, track_number, disc_number, path, duration, artwork_url, lyrics, sample_rate, bitrate, bit_depth, format FROM songs")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LocalSong {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                artist: row.get(2)?,
                album_artist: row.get(3)?,
                album: row.get(4)?,
                year: row.get(5)?,
                track_number: row.get(6)?,
                disc_number: row.get(7)?,
                path: row.get(8)?,
                duration: row.get(9)?,
                artwork: row.get(10)?,
                lyrics: row.get(11)?,
                sample_rate: row.get(12)?,
                bitrate: row.get(13)?,
                bit_depth: row.get(14)?,
                format: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut songs = Vec::new();
    for row in rows {
        songs.push(row.map_err(|e| e.to_string())?);
    }
    Ok(songs)
}

pub fn migrate_legacy_songs_if_needed(
    conn: &Connection,
    legacy_app_data_dir: PathBuf,
) -> Result<usize, String> {
    let legacy_db_path = legacy_app_data_dir.join("orca.db");
    if !legacy_db_path.exists() {
        return Ok(0);
    }

    let current_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM songs", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if current_count > 0 {
        return Ok(0);
    }

    let legacy_conn = Connection::open(legacy_db_path).map_err(|e| e.to_string())?;
    let legacy_has_songs_table: i64 = legacy_conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'songs'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if legacy_has_songs_table == 0 {
        return Ok(0);
    }

    let mut stmt = legacy_conn
        .prepare("SELECT title, artist, path, duration, artwork_url FROM songs")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LocalSong {
                path: row.get(2)?,
                title: row.get(0)?,
                artist: row.get(1)?,
                album_artist: row.get(1)?,
                album: "Unknown Album".to_string(),
                year: None,
                track_number: None,
                disc_number: None,
                duration: row.get(3)?,
                artwork: row.get(4)?,
                lyrics: None,
                sample_rate: None,
                bitrate: None,
                bit_depth: None,
                format: None,
                id: None,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    for row in rows {
        let song = row.map_err(|e| e.to_string())?;
        upsert_song(conn, &song)?;
        imported += 1;
    }

    Ok(imported)
}

pub fn migrate_inline_artwork_to_files(
    conn: &Connection,
    artwork_dir: &Path,
) -> Result<usize, String> {
    fs::create_dir_all(artwork_dir).map_err(|e| e.to_string())?;

    let mut inline_rows: Vec<(i64, String, String)> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, path, artwork_url FROM songs WHERE artwork_url LIKE 'data:%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            inline_rows.push(row.map_err(|e| e.to_string())?);
        }
    }

    let mut migrated = 0usize;
    for (id, song_path, artwork_url) in inline_rows {
        let Some((mime, base64_payload)) = parse_data_url(&artwork_url) else {
            continue;
        };

        let decoded = match BASE64_STANDARD.decode(base64_payload.as_bytes()) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if decoded.is_empty() {
            continue;
        }

        let ext = artwork_extension_from_mime(mime);
        let file_path = artwork_dir.join(format!("{}.{}", hash_song_path(&song_path), ext));
        fs::write(&file_path, &decoded).map_err(|e| e.to_string())?;

        conn.execute(
            "UPDATE songs SET artwork_url = ?1 WHERE id = ?2",
            params![file_path.to_string_lossy().to_string(), id],
        )
        .map_err(|e| e.to_string())?;
        migrated += 1;
    }

    Ok(migrated)
}

fn parse_data_url(value: &str) -> Option<(&str, &str)> {
    if !value.starts_with("data:") {
        return None;
    }
    let mut segments = value.splitn(2, ',');
    let header = segments.next()?;
    let payload = segments.next()?.trim();
    if !header.contains(";base64") {
        return None;
    }
    let mime = header
        .trim_start_matches("data:")
        .split(';')
        .next()
        .unwrap_or("image/jpeg");
    Some((mime, payload))
}

fn hash_song_path(path: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn artwork_extension_from_mime(mime: &str) -> &'static str {
    let normalized = mime.to_ascii_lowercase();
    if normalized.contains("png") {
        "png"
    } else if normalized.contains("webp") {
        "webp"
    } else if normalized.contains("gif") {
        "gif"
    } else if normalized.contains("bmp") {
        "bmp"
    } else {
        "jpg"
    }
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_song_by_path(conn: &Connection, path: &str) -> Result<(), String> {
    conn.execute("DELETE FROM songs WHERE path = ?1", params![path])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_lyrics(conn: &Connection, song_path: &str) -> Option<String> {
    conn.query_row(
        "SELECT lyrics_text FROM lyrics WHERE song_path = ?1",
        params![song_path],
        |row| row.get(0),
    )
    .ok()
}

pub fn set_lyrics(conn: &Connection, song_path: &str, lyrics_text: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO lyrics (song_path, lyrics_text) VALUES (?1, ?2)
         ON CONFLICT(song_path) DO UPDATE SET lyrics_text = excluded.lyrics_text",
        params![song_path, lyrics_text],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize, Debug)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub cover_path: Option<String>,
    pub song_count: i64,
}

pub fn create_playlist(conn: &Connection, name: &str, cover_path: Option<&str>) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO playlists (name, cover_path) VALUES (?1, ?2)",
        params![name, cover_path],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_playlist_cover(conn: &Connection, playlist_id: i64, cover_path: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE playlists SET cover_path = ?1 WHERE id = ?2",
        params![cover_path, playlist_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_playlists(conn: &Connection) -> Result<Vec<Playlist>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.cover_path, COUNT(ps.id) as song_count
             FROM playlists p
             LEFT JOIN playlist_songs ps ON p.id = ps.playlist_id
             GROUP BY p.id
             ORDER BY p.created_at DESC"
        )
        .map_err(|e| e.to_string())?;
        
    let iter = stmt.query_map([], |row| {
        Ok(Playlist {
            id: row.get(0)?,
            name: row.get(1)?,
            cover_path: row.get(2)?,
            song_count: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut playlists = Vec::new();
    for p in iter {
        playlists.push(p.map_err(|e| e.to_string())?);
    }
    Ok(playlists)
}

pub fn add_to_playlist(conn: &Connection, playlist_id: i64, song_id: i64) -> Result<(), String> {
    // Get max position
    let current_max: Option<i64> = conn.query_row(
        "SELECT MAX(position) FROM playlist_songs WHERE playlist_id = ?1",
        params![playlist_id],
        |row| row.get(0)
    ).unwrap_or(None);
    
    let next_pos = current_max.unwrap_or(0) + 1;

    conn.execute(
        "INSERT INTO playlist_songs (playlist_id, song_id, position) VALUES (?1, ?2, ?3)",
        params![playlist_id, song_id, next_pos],
    )
    .map_err(|e| e.to_string())?;
    
    Ok(())
}

pub fn get_playlist_song_ids(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare("SELECT song_id FROM playlist_songs WHERE playlist_id = ?1 ORDER BY position ASC")
        .map_err(|e| e.to_string())?;
        
    let iter = stmt.query_map(params![playlist_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
        
    let mut ids = Vec::new();
    for id in iter {
        ids.push(id.map_err(|e| e.to_string())?);
    }
    Ok(ids)
}

pub fn delete_playlist(conn: &Connection, playlist_id: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1",
        params![playlist_id],
    )
    .map_err(|e| e.to_string())?;

    conn.execute(
        "DELETE FROM playlists WHERE id = ?1",
        params![playlist_id],
    )
    .map_err(|e| e.to_string())?;
    
    Ok(())
}

pub fn rename_playlist(conn: &Connection, playlist_id: i64, new_name: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE playlists SET name = ?1 WHERE id = ?2",
        params![new_name, playlist_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
