//! Versioned, transactional SQLite migrations. Public callers share one schema.
use rusqlite::{Connection, TransactionBehavior};
use std::{collections::HashSet, path::PathBuf, time::Duration};

const SCHEMA_VERSION: i64 = 2;

pub fn init_db(app_dir: PathBuf) -> Result<Connection, String> {
    let mut conn = Connection::open(app_dir.join("orca.db")).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version > SCHEMA_VERSION {
        return Err(format!("Library schema {version} is newer than supported schema {SCHEMA_VERSION}; use a newer Orca build"));
    }
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA cache_size = -16000;",
    )
    .map_err(|e| e.to_string())?;
    migrate(&mut conn).map_err(|e| format!("Library migration failed: {e}"))?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == SCHEMA_VERSION {
        return tx.commit();
    }
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if version < 1 {
        tx.execute_batch(SCHEMA)?;
        // Inspect every column independently: some older libraries were only
        // partially upgraded. Never swallow DDL failures or commit partial upgrades.
        for (column, definition) in [
            ("album", "TEXT NOT NULL DEFAULT 'Unknown Album'"),
            ("album_artist", "TEXT NOT NULL DEFAULT 'Unknown Artist'"),
            ("year", "INTEGER"),
            ("track_number", "INTEGER"),
            ("disc_number", "INTEGER"),
            ("genre", "TEXT"),
            ("artwork_url", "TEXT"),
            ("artwork_thumb_url", "TEXT"),
            ("artwork_preview_url", "TEXT"),
            ("lyrics", "TEXT"),
            ("sample_rate", "INTEGER"),
            ("bitrate", "INTEGER"),
            ("bit_depth", "INTEGER"),
            ("format", "TEXT"),
            ("modified_at", "INTEGER"),
            ("file_size", "INTEGER"),
        ] {
            ensure_column(&tx, "songs", column, definition)?;
        }
        for table in ["artist_artworks", "album_artworks"] {
            ensure_column(&tx, table, "artwork_thumb_path", "TEXT")?;
        }
        ensure_column(&tx, "playlists", "cover_path", "TEXT")?;
        ensure_column(&tx, "playlists", "created_at", "TEXT")?;
        consolidate(&tx)?;
    }
    // Indexes follow collection predicates, ordering and FK deletion paths.
    if version < 2 {
        tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_songs_title ON songs(title COLLATE NOCASE,id);
            CREATE INDEX IF NOT EXISTS idx_songs_artist ON songs(artist,title COLLATE NOCASE,id);
            CREATE INDEX IF NOT EXISTS idx_songs_artist_sort ON songs(artist COLLATE NOCASE,title COLLATE NOCASE,id);
            CREATE INDEX IF NOT EXISTS idx_songs_album_order ON songs(album,album_artist,disc_number,track_number,id);
            CREATE INDEX IF NOT EXISTS idx_songs_genre ON songs(genre,title COLLATE NOCASE,id);
            CREATE INDEX IF NOT EXISTS idx_playlist_songs_order ON playlist_songs(playlist_id,position,id);
            CREATE INDEX IF NOT EXISTS idx_playlist_songs_song ON playlist_songs(song_id);")?;
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    if !columns.contains(column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn consolidate(conn: &Connection) -> rusqlite::Result<()> {
    #[cfg(target_os = "windows")]
    const IDENTITY: &str = "lower(replace(replace(path, char(92), '/'), '//?/', ''))";
    #[cfg(not(target_os = "windows"))]
    const IDENTITY: &str = "path";
    // Move dependent entries before deleting duplicates. Otherwise foreign key
    // cascades silently erase playlist membership and waveform caches.
    conn.execute_batch(&format!("
        CREATE TEMP TABLE song_merge AS SELECT id AS old_id, path AS old_path,
            MAX(id) OVER (PARTITION BY {IDENTITY}) AS keep_id FROM songs;
        UPDATE playlist_songs SET song_id=(SELECT keep_id FROM song_merge WHERE old_id=song_id);
        INSERT OR IGNORE INTO waveforms(song_path,buckets,peaks)
            SELECT songs.path,w.buckets,w.peaks FROM waveforms w
            JOIN song_merge m ON m.old_path=w.song_path JOIN songs ON songs.id=m.keep_id
            WHERE m.old_id<>m.keep_id;
        INSERT OR IGNORE INTO lyrics(song_path,lyrics_text)
            SELECT songs.path,l.lyrics_text FROM lyrics l
            JOIN song_merge m ON m.old_path=l.song_path JOIN songs ON songs.id=m.keep_id
            WHERE m.old_id<>m.keep_id;
        DELETE FROM lyrics WHERE song_path IN (SELECT old_path FROM song_merge WHERE old_id<>keep_id);
        DELETE FROM songs WHERE id IN (SELECT old_id FROM song_merge WHERE old_id<>keep_id);
        DROP TABLE song_merge;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_songs_path_unique ON songs(path);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_songs_path_norm_unique ON songs({IDENTITY});
        CREATE TEMP TABLE playlist_merge AS SELECT id AS old_id,
            MAX(id) OVER (PARTITION BY lower(trim(name))) AS keep_id FROM playlists;
        UPDATE playlist_songs SET playlist_id=(SELECT keep_id FROM playlist_merge WHERE old_id=playlist_id);
        DELETE FROM playlists WHERE id IN (SELECT old_id FROM playlist_merge WHERE old_id<>keep_id);
        DROP TABLE playlist_merge;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_playlists_name_ci_unique ON playlists(lower(trim(name)));
    "))?;
    // Combined playlists retain every entry, with deterministic contiguous order.
    conn.execute_batch("
        WITH ordering AS (SELECT id, ROW_NUMBER() OVER(PARTITION BY playlist_id ORDER BY position,id)-1 AS ordinal FROM playlist_songs)
        UPDATE playlist_songs SET position=(SELECT ordinal FROM ordering WHERE ordering.id=playlist_songs.id);
    ")?;
    Ok(())
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS songs (
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
        );
CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT
        );
CREATE TABLE IF NOT EXISTS lyrics (
            song_path TEXT PRIMARY KEY,
            lyrics_text TEXT NOT NULL
        );
CREATE TABLE IF NOT EXISTS playlists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            cover_path TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
CREATE TABLE IF NOT EXISTS playlist_songs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            playlist_id INTEGER NOT NULL,
            song_id INTEGER NOT NULL,
            position INTEGER NOT NULL,
            FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY(song_id) REFERENCES songs(id) ON DELETE CASCADE
        );
CREATE TABLE IF NOT EXISTS artist_artworks (
            artist_name TEXT PRIMARY KEY,
            artwork_path TEXT,
            artwork_thumb_path TEXT
        );
CREATE TABLE IF NOT EXISTS album_artworks (
            album_key TEXT PRIMARY KEY,
            artwork_path TEXT,
            artwork_thumb_path TEXT
        );
CREATE TABLE IF NOT EXISTS waveforms (
            song_path TEXT,
            buckets INTEGER,
            peaks TEXT NOT NULL,
            PRIMARY KEY(song_path, buckets),
            FOREIGN KEY(song_path) REFERENCES songs(path) ON DELETE CASCADE
        );
";

#[cfg(test)]
mod tests;
