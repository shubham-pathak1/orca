//! Versioned, transactional SQLite migrations. Public callers share one schema.
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, TransactionBehavior};
use std::{collections::HashSet, path::PathBuf, time::Duration};

// user_version records the minimum reader schema, not the app version.
// Numbered migrations track data changes; repeatable DDL ensures additive tables.
const MIN_READER_VERSION: i64 = 2;
const LIKES_MIGRATION: i64 = 3;
const FOLDER_RULES_MIGRATION: i64 = 4;
const LYRICS_SUPPRESSION_MIGRATION: i64 = 5;
const EXPLICIT_ADVISORY_MIGRATION: i64 = 6;
const LIKES_PREVIEW_VERSION: i64 = 3;
const LIKES_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS liked_songs (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    liked_at INTEGER NOT NULL DEFAULT (unixepoch())
);";

pub fn init_db(app_dir: PathBuf) -> Result<Connection, String> {
    let mut conn = Connection::open(app_dir.join("orca.db")).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version > MIN_READER_VERSION
        && !is_likes_preview(&conn, version).map_err(|e| e.to_string())?
    {
        return Err(format!("Library schema {version} is newer than supported schema {MIN_READER_VERSION}; use a newer Orca build"));
    }
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA cache_size = -16000;",
    )
    .map_err(|e| e.to_string())?;
    migrate(&mut conn).map_err(|e| format!("Library migration failed: {e}"))?;
    conn.create_scalar_function(
        "primary_artist_names",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let artist: String = ctx.get(0)?;
            let title: String = ctx.get(1)?;
            serde_json::to_string(&crate::artist_credits::primary_artists(&artist, &title))
                .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
        },
    )
    .map_err(|error| error.to_string())?;
    conn.create_scalar_function(
        "artist_credit_contains",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let artist: String = ctx.get(0)?;
            let title: String = ctx.get(1)?;
            let name: String = ctx.get(2)?;
            Ok(crate::artist_credits::contains(&artist, &title, &name))
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > MIN_READER_VERSION && !is_likes_preview(&tx, version)? {
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
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS orca_schema_migrations (
        revision INTEGER PRIMARY KEY,
        applied_at INTEGER NOT NULL DEFAULT (unixepoch())
    );",
    )?;
    let likes_applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM orca_schema_migrations WHERE revision=?1)",
        [LIKES_MIGRATION],
        |row| row.get(0),
    )?;
    if !likes_applied {
        tx.execute_batch(LIKES_SCHEMA)?;
        tx.execute(
            "INSERT INTO orca_schema_migrations(revision) VALUES(?1)",
            [LIKES_MIGRATION],
        )?;
    }
    let folder_rules_applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM orca_schema_migrations WHERE revision=?1)",
        [FOLDER_RULES_MIGRATION],
        |row| row.get(0),
    )?;
    if !folder_rules_applied {
        ensure_column(
            &tx,
            "songs",
            "library_visible",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS folder_rules (
            path TEXT PRIMARY KEY,
            included INTEGER NOT NULL CHECK(included IN (0,1))
        );",
        )?;
        tx.execute(
            "INSERT INTO orca_schema_migrations(revision) VALUES(?1)",
            [FOLDER_RULES_MIGRATION],
        )?;
    }
    let lyrics_suppression_applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM orca_schema_migrations WHERE revision=?1)",
        [LYRICS_SUPPRESSION_MIGRATION],
        |row| row.get(0),
    )?;
    if !lyrics_suppression_applied {
        // Old empty entries were also written by unrelated metadata edits, so
        // only future explicit removals may suppress automatic lookup.
        ensure_column(&tx, "lyrics", "suppressed", "INTEGER NOT NULL DEFAULT 0")?;
        tx.execute(
            "INSERT INTO orca_schema_migrations(revision) VALUES(?1)",
            [LYRICS_SUPPRESSION_MIGRATION],
        )?;
    }
    let explicit_advisory_applied: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM orca_schema_migrations WHERE revision=?1)",
        [EXPLICIT_ADVISORY_MIGRATION],
        |row| row.get(0),
    )?;
    if !explicit_advisory_applied {
        ensure_column(&tx, "songs", "explicit_advisory", "INTEGER")?;
        ensure_column(&tx, "songs", "modified_at", "INTEGER")?;
        // Existing rows need one tag read before a badge can be shown.
        tx.execute("UPDATE songs SET modified_at=NULL", [])?;
        tx.execute(
            "INSERT INTO orca_schema_migrations(revision) VALUES(?1)",
            [EXPLICIT_ADVISORY_MIGRATION],
        )?;
    }
    tx.execute_batch(
        "CREATE VIEW IF NOT EXISTS active_songs AS SELECT * FROM songs WHERE library_visible=1;",
    )?;
    let artist_index_exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='song_primary_artists')", [], |row| row.get(0))?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS song_primary_artists (
            song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            PRIMARY KEY(song_id,name)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS idx_primary_artist_name ON song_primary_artists(name COLLATE NOCASE,song_id);
         CREATE TABLE IF NOT EXISTS artist_index_pending (song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE);
         CREATE TRIGGER IF NOT EXISTS artist_index_insert AFTER INSERT ON songs BEGIN
            INSERT INTO artist_index_pending SELECT NEW.id WHERE NOT EXISTS(SELECT 1 FROM artist_index_pending WHERE song_id=NEW.id);
         END;
         CREATE TRIGGER IF NOT EXISTS artist_index_update AFTER UPDATE OF artist,title ON songs
         WHEN OLD.artist IS NOT NEW.artist OR OLD.title IS NOT NEW.title BEGIN
            INSERT INTO artist_index_pending SELECT NEW.id WHERE NOT EXISTS(SELECT 1 FROM artist_index_pending WHERE song_id=NEW.id);
         END;",
    )?;
    if !artist_index_exists {
        tx.execute(
            "INSERT OR IGNORE INTO artist_index_pending SELECT id FROM songs",
            [],
        )?;
    }
    // Cover the active catalog order so browsing IDs does not fetch every
    // wide song row merely to check visibility. Hidden folder rules remain
    // enforced by the partial index predicate.
    // Some legacy databases reported v2 before all tag columns were present.
    ensure_column(
        &tx,
        "songs",
        "album",
        "TEXT NOT NULL DEFAULT 'Unknown Album'",
    )?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_active_title ON songs(title COLLATE NOCASE,id) WHERE library_visible=1;
         CREATE INDEX IF NOT EXISTS idx_active_artist ON songs(artist COLLATE NOCASE,title COLLATE NOCASE,id) WHERE library_visible=1;
         CREATE INDEX IF NOT EXISTS idx_active_album ON songs(album COLLATE NOCASE,title COLLATE NOCASE,id) WHERE library_visible=1;",
    )?;
    ensure_column(
        &tx,
        "songs",
        "album_artist",
        "TEXT NOT NULL DEFAULT 'Unknown Artist'",
    )?;
    tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_active_id ON songs(id) WHERE library_visible=1;
        CREATE INDEX IF NOT EXISTS idx_active_album_summary ON songs(album_artist,album,duration) WHERE library_visible=1;")?;
    // Reconciliation reads this narrow index instead of every wide metadata row.
    ensure_column(&tx, "songs", "modified_at", "INTEGER")?;
    ensure_column(&tx, "songs", "file_size", "INTEGER")?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_scan_file_state ON songs(path,modified_at,file_size)",
    )?;
    ensure_column(&tx, "songs", "genre", "TEXT")?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_active_genre ON songs(genre) WHERE library_visible=1",
    )?;
    if cfg!(windows) {
        tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_active_folder_path ON songs(replace(path,char(92),'/') COLLATE NOCASE,id) WHERE library_visible=1;")?;
    } else {
        tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_active_folder_path ON songs(path COLLATE BINARY,id) WHERE library_visible=1;")?;
    }
    for column in ["artwork_url", "artwork_thumb_url", "artwork_preview_url"] {
        ensure_column(&tx, "songs", column, "TEXT")?;
    }
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS folder_cache_state(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL);
         INSERT OR IGNORE INTO folder_cache_state VALUES(1,0);
         CREATE TABLE IF NOT EXISTS folder_cache(key TEXT UNIQUE NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL);
         CREATE TRIGGER IF NOT EXISTS folder_cache_song_insert AFTER INSERT ON songs BEGIN
            UPDATE folder_cache_state SET revision=revision+1 WHERE id=1; END;
         CREATE TRIGGER IF NOT EXISTS folder_cache_song_delete AFTER DELETE ON songs BEGIN
            UPDATE folder_cache_state SET revision=revision+1 WHERE id=1; END;
         CREATE TRIGGER IF NOT EXISTS folder_cache_song_update AFTER UPDATE ON songs
         WHEN OLD.path IS NOT NEW.path OR OLD.duration IS NOT NEW.duration
           OR OLD.library_visible IS NOT NEW.library_visible OR OLD.album IS NOT NEW.album
           OR OLD.album_artist IS NOT NEW.album_artist OR OLD.artwork_url IS NOT NEW.artwork_url
           OR OLD.artwork_thumb_url IS NOT NEW.artwork_thumb_url OR OLD.artwork_preview_url IS NOT NEW.artwork_preview_url
         BEGIN UPDATE folder_cache_state SET revision=revision+1 WHERE id=1; END;",
    )?;
    for table in ["album_artworks", "settings"] {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            let when = if table == "settings" {
                let row = if event == "DELETE" { "OLD" } else { "NEW" };
                format!("WHEN {row}.key IN ('library_scan_roots','qt_scan_roots','collection_overrides')")
            } else {
                String::new()
            };
            tx.execute_batch(&format!(
                "CREATE TRIGGER IF NOT EXISTS folder_cache_{table}_{event} AFTER {event} ON {table} {when}
                 BEGIN UPDATE folder_cache_state SET revision=revision+1 WHERE id=1; END;"
            ))?;
        }
    }
    if version != MIN_READER_VERSION {
        tx.pragma_update(None, "user_version", MIN_READER_VERSION)?;
    }
    tx.commit()
}

fn is_likes_preview(conn: &Connection, version: i64) -> rusqlite::Result<bool> {
    if version != LIKES_PREVIEW_VERSION {
        return Ok(false);
    }
    let mut statement = conn.prepare("PRAGMA table_info(liked_songs)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(columns.contains("song_id") && columns.contains("liked_at"))
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
            explicit_advisory INTEGER,
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
            lyrics_text TEXT NOT NULL,
            suppressed INTEGER NOT NULL DEFAULT 0
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
