use super::*;

fn legacy() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;
        CREATE TABLE songs(id INTEGER PRIMARY KEY,title TEXT NOT NULL,artist TEXT NOT NULL,path TEXT NOT NULL UNIQUE,duration INTEGER NOT NULL,sample_rate INTEGER);
        INSERT INTO songs VALUES(1,'Song','Artist','song.flac',100,44100);
    ").unwrap();
    conn
}

#[test]
fn partial_legacy_upgrade_adds_every_column_and_is_idempotent() {
    let mut conn = legacy();
    migrate(&mut conn).unwrap();
    conn.prepare("SELECT album_artist,bitrate,bit_depth,format,file_size,lyrics FROM songs")
        .unwrap();
    let before: i64 = conn
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap();
    migrate(&mut conn).unwrap();
    let after: i64 = conn
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        before, after,
        "current libraries must not repeat migrations"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM songs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn failed_migration_rolls_back_columns_data_and_version() {
    let mut conn = legacy();
    conn.execute_batch("CREATE TABLE playlists(id INTEGER PRIMARY KEY,name TEXT);
        INSERT INTO playlists VALUES(1,'Mix'),(2,' mix ');
        CREATE TRIGGER refuse_delete BEFORE DELETE ON playlists BEGIN SELECT RAISE(ABORT,'injected migration failure'); END;").unwrap();
    assert!(migrate(&mut conn).is_err());
    assert!(conn.prepare("SELECT album_artist FROM songs").is_err());
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM playlists", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    conn.execute_batch("DROP TRIGGER refuse_delete").unwrap();
    migrate(&mut conn).unwrap();
}

#[test]
fn duplicate_playlist_cleanup_preserves_all_memberships_in_order() {
    let mut conn = legacy();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute_batch(
        "INSERT INTO playlists(id,name) VALUES(1,'Mix'),(2,' mix ');
        INSERT INTO playlist_songs(playlist_id,song_id,position) VALUES(1,1,0),(2,1,0);",
    )
    .unwrap();
    migrate(&mut conn).unwrap();
    let entries: Vec<(i64, i64)> = conn
        .prepare("SELECT playlist_id,position FROM playlist_songs ORDER BY position")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(entries, vec![(2, 0), (2, 1)]);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM playlists", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[cfg(target_os = "windows")]
#[test]
fn duplicate_windows_paths_preserve_playlist_and_waveform_references() {
    let mut conn = legacy();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute_batch("INSERT INTO songs(id,title,artist,path,duration) VALUES(2,'Song','Artist','SONG.FLAC',100);
        INSERT INTO playlists(id,name) VALUES(1,'Mix');
        INSERT INTO playlist_songs(playlist_id,song_id,position) VALUES(1,1,0);
        INSERT INTO waveforms VALUES('song.flac',2,'[0.2,0.4]');
        INSERT INTO lyrics VALUES('song.flac','Saved lyrics');").unwrap();
    migrate(&mut conn).unwrap();
    assert_eq!(
        conn.query_row("SELECT song_id FROM playlist_songs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT song_path FROM waveforms", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "SONG.FLAC"
    );
    assert_eq!(
        conn.query_row(
            "SELECT lyrics_text FROM lyrics WHERE song_path='SONG.FLAC'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Saved lyrics"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM songs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn future_schema_is_rejected_without_changing_database_bytes() {
    let directory = std::env::temp_dir().join(format!("orca-future-schema-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("orca.db");
    let conn = Connection::open(&file).unwrap();
    conn.execute_batch(
        "PRAGMA user_version=999; CREATE TABLE IF NOT EXISTS future_data(value TEXT);",
    )
    .unwrap();
    drop(conn);
    let before = std::fs::read(&file).unwrap();
    let error = init_db(directory.clone()).unwrap_err();
    assert!(error.contains("newer than supported"));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(!directory.join("orca.db-wal").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn version_two_adds_query_indexes_without_repeating_duplicate_cleanup() {
    let mut conn = legacy();
    migrate(&mut conn).unwrap();
    conn.execute_batch("PRAGMA user_version=1;
        DROP INDEX idx_songs_title;
        CREATE TRIGGER forbid_song_cleanup BEFORE DELETE ON songs BEGIN SELECT RAISE(ABORT,'cleanup repeated'); END;").unwrap();
    migrate(&mut conn).unwrap();
    let detail: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM songs ORDER BY title COLLATE NOCASE,id",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(detail.contains("idx_songs_title"), "{detail}");
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
}
