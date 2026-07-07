use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;
use rusqlite::{params, Connection, OptionalExtension};

use crate::library::LocalSong;

pub fn init_db(app_dir: PathBuf) -> Result<Connection, String> {
    let db_path = app_dir.join("orca.db");
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute("PRAGMA foreign_keys = ON;", []).map_err(|e| e.to_string())?;
    // WAL allows concurrent reads during writes (e.g. scan + UI queries running together).
    conn.execute("PRAGMA journal_mode = WAL;", []).map_err(|e| e.to_string())?;
    // Keep 16 MB of pages in memory to reduce disk I/O for repeated aggregation queries.
    conn.execute("PRAGMA cache_size = -16000;", []).map_err(|e| e.to_string())?;


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
            genre TEXT,
            path TEXT NOT NULL UNIQUE,
            duration INTEGER NOT NULL,
            artwork_url TEXT,
            artwork_thumb_url TEXT,
            artwork_preview_url TEXT,
            lyrics TEXT,
            sample_rate INTEGER,
            bitrate INTEGER,
            bit_depth INTEGER,
            format TEXT,
            modified_at INTEGER,
            file_size INTEGER
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    // add lyrics column if missing
    let has_lyrics: bool = conn
        .prepare("SELECT lyrics FROM songs LIMIT 0")
        .is_ok();
    if !has_lyrics {
        conn.execute("ALTER TABLE songs ADD COLUMN lyrics TEXT", [])
            .ok();
    }

    // add album column if missing
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

    let has_genre: bool = conn.prepare("SELECT genre FROM songs LIMIT 0").is_ok();
    if !has_genre {
        conn.execute("ALTER TABLE songs ADD COLUMN genre TEXT", []).ok();
    }

    let has_artwork_thumb: bool = conn.prepare("SELECT artwork_thumb_url FROM songs LIMIT 0").is_ok();
    if !has_artwork_thumb {
        conn.execute("ALTER TABLE songs ADD COLUMN artwork_thumb_url TEXT", []).ok();
    }

    let has_artwork_preview: bool = conn.prepare("SELECT artwork_preview_url FROM songs LIMIT 0").is_ok();
    if !has_artwork_preview {
        conn.execute("ALTER TABLE songs ADD COLUMN artwork_preview_url TEXT", []).ok();
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

    let has_modified_at: bool = conn.prepare("SELECT modified_at FROM songs LIMIT 0").is_ok();
    if !has_modified_at {
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN modified_at INTEGER", []);
    }

    let has_file_size: bool = conn.prepare("SELECT file_size FROM songs LIMIT 0").is_ok();
    if !has_file_size {
        let _ = conn.execute("ALTER TABLE songs ADD COLUMN file_size INTEGER", []);
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS waveforms (
            song_path TEXT,
            buckets INTEGER,
            peaks TEXT NOT NULL,
            PRIMARY KEY(song_path, buckets),
            FOREIGN KEY(song_path) REFERENCES songs(path) ON DELETE CASCADE
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

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
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
        .map_err(|e| e.to_string())?;

    for song in songs {
        if let Err(error) = upsert_song(conn, song) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    Ok(())
}

pub fn replace_songs_in_db(conn: &Connection, songs: &[LocalSong]) -> Result<(), String> {
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
        .map_err(|e| e.to_string())?;

    for song in songs {
        if let Err(error) = upsert_song(conn, song) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error);
        }
    }

    let scanned_paths = songs
        .iter()
        .map(|song| song.path.as_str())
        .collect::<std::collections::HashSet<_>>();

    let existing = {
        let mut stmt = match conn.prepare("SELECT id, path FROM songs") {
            Ok(stmt) => stmt,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error.to_string());
            }
        };
        let rows = match stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))) {
            Ok(rows) => rows,
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error.to_string());
            }
        };

        let mut existing = Vec::new();
        for row in rows {
            match row {
                Ok(value) => existing.push(value),
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(error.to_string());
                }
            }
        }
        existing
    };

    for (song_id, path) in existing {
        if scanned_paths.contains(path.as_str()) {
            continue;
        }

        if let Err(error) = conn.execute("DELETE FROM playlist_songs WHERE song_id = ?1", params![song_id]) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error.to_string());
        }

        if let Err(error) = conn.execute("DELETE FROM songs WHERE id = ?1", params![song_id]) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(error.to_string());
        }
    }

    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_song(conn: &Connection, song: &LocalSong) -> Result<(), String> {
    conn.execute(
        "INSERT INTO songs (title, artist, album_artist, album, year, track_number, disc_number, genre, path, duration, artwork_url, artwork_thumb_url, artwork_preview_url, lyrics, sample_rate, bitrate, bit_depth, format, modified_at, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT DO UPDATE SET
             title = excluded.title,
             artist = excluded.artist,
             album_artist = excluded.album_artist,
             album = excluded.album,
             year = excluded.year,
             track_number = excluded.track_number,
             disc_number = excluded.disc_number,
             genre = excluded.genre,
             path = excluded.path,
             duration = excluded.duration,
             artwork_url = excluded.artwork_url,
             artwork_thumb_url = excluded.artwork_thumb_url,
             artwork_preview_url = excluded.artwork_preview_url,
             lyrics = excluded.lyrics,
             sample_rate = excluded.sample_rate,
             bitrate = excluded.bitrate,
             bit_depth = excluded.bit_depth,
             format = excluded.format,
             modified_at = excluded.modified_at,
             file_size = excluded.file_size",
        params![
            song.title,
            song.artist,
            song.album_artist,
            song.album,
            song.year,
            song.track_number,
            song.disc_number,
            song.genre,
            song.path,
            song.duration,
            song.artwork,
            song.artwork_thumb,
            song.artwork_preview,
            song.lyrics,
            song.sample_rate,
            song.bitrate,
            song.bit_depth,
            song.format,
            song.modified_at,
            song.file_size
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_all_songs(conn: &Connection) -> Result<Vec<LocalSong>, String> {
    let mut stmt = conn
        .prepare("SELECT id, title, artist, album_artist, album, year, track_number, disc_number, genre, path, duration, artwork_url, artwork_thumb_url, artwork_preview_url, lyrics, sample_rate, bitrate, bit_depth, format, modified_at, file_size FROM songs")
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
                genre: row.get(8)?,
                path: row.get(9)?,
                duration: row.get(10)?,
                artwork: row.get(11)?,
                artwork_thumb: row.get(12)?,
                artwork_preview: row.get(13)?,
                lyrics: row.get(14)?,
                sample_rate: row.get(15)?,
                bitrate: row.get(16)?,
                bit_depth: row.get(17)?,
                format: row.get(18)?,
                modified_at: row.get(19)?,
                file_size: row.get(20)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut songs = Vec::new();
    for row in rows {
        songs.push(row.map_err(|e| e.to_string())?);
    }
    Ok(songs)
}

pub fn get_existing_songs_map(conn: &Connection) -> Result<std::collections::HashMap<String, (i64, u64, LocalSong)>, String> {
    let songs = get_all_songs(conn)?;
    let mut map = std::collections::HashMap::new();
    for song in songs {
        let mtime = song.modified_at.unwrap_or(0);
        let size = song.file_size.unwrap_or(0);
        map.insert(song.path.clone(), (mtime, size, song));
    }
    Ok(map)
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
                genre: None,
                duration: row.get(3)?,
                artwork: row.get(4)?,
                artwork_thumb: None,
                artwork_preview: None,
                lyrics: None,
                sample_rate: None,
                bitrate: None,
                bit_depth: None,
                format: None,
                id: None,
                modified_at: None,
                file_size: None,
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
        let hash = hash_song_path(&song_path);
        let file_path = artwork_dir.join(format!("{}.{}", hash, ext));
        let thumb_path = artwork_dir.join(format!("thumb_{}_80.webp", hash));
        let preview_path = artwork_dir.join(format!("preview_{}_256.webp", hash));
        fs::write(&file_path, &decoded).map_err(|e| e.to_string())?;
        let (thumb_url, preview_url) = write_artwork_derivatives(&decoded, &thumb_path, &preview_path)
            .unwrap_or((None, None));

        conn.execute(
            "UPDATE songs SET artwork_url = ?1, artwork_thumb_url = ?2, artwork_preview_url = ?3 WHERE id = ?4",
            params![
                file_path.to_string_lossy().to_string(),
                thumb_url,
                preview_url,
                id
            ],
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

fn write_artwork_derivatives(
    bytes: &[u8],
    thumb_path: &Path,
    preview_path: &Path,
) -> Result<(Option<String>, Option<String>), String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let thumb = write_webp_derivative(&image, thumb_path, 80).ok().map(|_| thumb_path.to_string_lossy().to_string());
    let preview = write_webp_derivative(&image, preview_path, 256).ok().map(|_| preview_path.to_string_lossy().to_string());
    Ok((thumb, preview))
}

fn write_webp_derivative(
    image: &image::DynamicImage,
    output_path: &Path,
    size: u32,
) -> Result<(), String> {
    let resized = image.resize(size, size, FilterType::Triangle).to_rgba8();
    let mut output: Vec<u8> = Vec::new();
    let encoder = WebPEncoder::new_lossless(Cursor::new(&mut output));
    encoder
        .encode(
            resized.as_raw(),
            resized.width(),
            resized.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
    fs::write(output_path, &output).map_err(|error| error.to_string())
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

/// Returns cached waveform peaks for `song_path` with the exact `buckets` resolution,
/// or `None` if no entry exists yet.
pub fn get_cached_waveform(
    conn: &Connection,
    song_path: &str,
    buckets: usize,
) -> Result<Option<Vec<f32>>, String> {
    let row: Option<String> = conn
        .query_row(
            "SELECT peaks FROM waveforms WHERE song_path = ?1 AND buckets = ?2",
            params![song_path, buckets as i64],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    match row {
        None => Ok(None),
        Some(s) => {
            let peaks: Vec<f32> = s
                .split(',')
                .filter_map(|v| v.parse::<f32>().ok())
                .collect();
            Ok(Some(peaks))
        }
    }
}

/// Persists waveform peaks for `song_path` so future lookups are instant.
pub fn save_waveform(
    conn: &Connection,
    song_path: &str,
    buckets: usize,
    peaks: &[f32],
) -> Result<(), String> {
    let peaks_str: String = peaks
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");

    conn.execute(
        "INSERT INTO waveforms (song_path, buckets, peaks)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(song_path, buckets) DO UPDATE SET peaks = excluded.peaks",
        params![song_path, buckets as i64, peaks_str],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize, Debug)]
pub struct ArtistEntry {
    pub name: String,
    pub song_count: i64,
    pub artwork: Option<String>,
}

#[derive(serde::Serialize, Debug)]
pub struct AlbumEntry {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub song_count: i64,
    pub duration: i64,
    pub artwork: Option<String>,
}

pub fn get_artists(conn: &Connection) -> Result<Vec<ArtistEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT artist, COUNT(*), COALESCE(MAX(artwork_preview_url), MAX(artwork_thumb_url))
             FROM songs
             GROUP BY artist
             ORDER BY artist COLLATE NOCASE ASC"
        )
        .map_err(|e| e.to_string())?;
    
    let iter = stmt.query_map([], |row| {
        Ok(ArtistEntry {
            name: row.get(0)?,
            song_count: row.get(1)?,
            artwork: row.get(2)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut artists = Vec::new();
    for a in iter {
        artists.push(a.map_err(|e| e.to_string())?);
    }
    Ok(artists)
}

pub fn get_albums(conn: &Connection) -> Result<Vec<AlbumEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT album_artist || ':' || album as key, album, album_artist, COUNT(*), SUM(duration), COALESCE(MAX(artwork_preview_url), MAX(artwork_thumb_url))
             FROM songs
             GROUP BY album_artist, album
             ORDER BY album COLLATE NOCASE ASC"
        )
        .map_err(|e| e.to_string())?;
    
    let iter = stmt.query_map([], |row| {
        Ok(AlbumEntry {
            key: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            song_count: row.get(3)?,
            duration: row.get(4)?,
            artwork: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut albums = Vec::new();
    for a in iter {
        albums.push(a.map_err(|e| e.to_string())?);
    }
    Ok(albums)
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
    let already_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?1 AND song_id = ?2",
        params![playlist_id, song_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;
    if already_exists > 0 {
        return Ok(());
    }

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

pub fn remove_from_playlist(conn: &Connection, playlist_id: i64, song_id: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1 AND song_id = ?2",
        params![playlist_id, song_id],
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
