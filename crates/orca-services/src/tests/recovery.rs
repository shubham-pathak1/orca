use super::*;

#[test]
fn imported_and_existing_native_profiles_keep_artwork_without_the_old_cache() {
    for existing_native in [false, true] {
        let source = Fixture::new();
        let backend = source.backend();
        seed(&backend);
        let full = source.0.join("full.png");
        let thumb = source.0.join("thumb.webp");
        let preview = source.0.join("preview.webp");
        for (path, bytes) in [
            (&full, b"original".as_slice()),
            (&thumb, b"thumbnail"),
            (&preview, b"preview"),
        ] {
            fs::write(path, bytes).unwrap();
        }
        let full = full.to_str().unwrap();
        let thumb = thumb.to_str().unwrap();
        let preview = preview.to_str().unwrap();
        backend
            .conn
            .execute(
                "UPDATE songs SET artwork_url=?1,artwork_thumb_url=?2,artwork_preview_url=?3",
                params![full, thumb, preview],
            )
            .unwrap();
        db::update_artist_artwork(&backend.conn, "A", Some(full), Some(thumb)).unwrap();
        db::update_album_artwork(&backend.conn, "A:Shared", Some(full), Some(thumb)).unwrap();
        backend
            .conn
            .execute(
                "INSERT INTO playlists(name,cover_path) VALUES('Mix',?1)",
                [full],
            )
            .unwrap();
        backend
            .conn
            .execute(
                "INSERT INTO playlist_songs(playlist_id,song_id,position) VALUES(1,1,0)",
                [],
            )
            .unwrap();
        db::set_setting(&backend.conn,"collection_overrides", &serde_json::json!({"folder":{"cover":full,"thumb":thumb},"removed":{"cover":"","thumb":""}}).to_string()).unwrap();
        backend
            .conn
            .execute(
                "DELETE FROM settings WHERE key='native_artwork_migration_v1'",
                [],
            )
            .unwrap();
        let destination = Fixture::new();
        import_library(&source.0.join("orca.db"), &destination.0).unwrap();
        if existing_native {
            let conn = Connection::open(destination.0.join("orca.db")).unwrap();
            conn.execute("UPDATE playlists SET name='Native edit'", [])
                .unwrap();
        }
        let migrated = destination.backend();
        let local = db::get_all_songs(&migrated.conn).unwrap();
        assert_eq!(local.len(), 3);
        let paths: Vec<String> = migrated.conn.prepare("SELECT artwork_url FROM songs UNION SELECT artwork_thumb_url FROM songs UNION SELECT artwork_preview_url FROM songs UNION SELECT artwork_path FROM artist_artworks UNION SELECT artwork_thumb_path FROM artist_artworks UNION SELECT artwork_path FROM album_artworks UNION SELECT artwork_thumb_path FROM album_artworks UNION SELECT cover_path FROM playlists").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(paths.len(), 3);
        for path in &paths {
            assert!(Path::new(path).starts_with(destination.0.canonicalize().unwrap()));
        }
        let overrides: serde_json::Value =
            serde_json::from_str(&db::get_setting(&migrated.conn, "collection_overrides").unwrap())
                .unwrap();
        assert!(Path::new(overrides["folder"]["cover"].as_str().unwrap())
            .starts_with(destination.0.canonicalize().unwrap()));
        assert_eq!(overrides["removed"]["cover"], "");
        assert_eq!(
            migrated
                .conn
                .query_row("SELECT count(*) FROM playlist_songs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            migrated
                .conn
                .query_row("SELECT name FROM playlists", [], |r| r.get::<_, String>(0))
                .unwrap(),
            if existing_native {
                "Native edit"
            } else {
                "Mix"
            }
        );
        assert_eq!(
            backend
                .conn
                .query_row("SELECT cover_path FROM playlists", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            full
        );
        drop(migrated);
        for path in [&full, &thumb, &preview] {
            fs::remove_file(path).unwrap();
        }
        let reopened = destination.backend();
        assert_eq!(reopened.statistics().unwrap().songs, 3);
        let mut bytes = paths
            .iter()
            .map(|p| fs::read(p).unwrap())
            .collect::<Vec<_>>();
        bytes.sort();
        let mut expected = vec![
            b"original".to_vec(),
            b"thumbnail".to_vec(),
            b"preview".to_vec(),
        ];
        expected.sort();
        assert_eq!(bytes, expected);
    }
}

#[test]
fn unavailable_imported_artwork_is_preserved_and_retried_on_next_startup() {
    let source = Fixture::new();
    let destination = Fixture::new();
    let b = destination.backend();
    seed(&b);
    let missing = source.0.join("later.png");
    b.conn
        .execute(
            "UPDATE songs SET artwork_url=?1",
            [missing.to_str().unwrap()],
        )
        .unwrap();
    b.conn
        .execute(
            "DELETE FROM settings WHERE key='native_artwork_migration_v1'",
            [],
        )
        .unwrap();
    drop(b);
    let b = destination.backend();
    assert!(db::get_setting(&b.conn, "native_artwork_migration_v1").is_none());
    drop(b);
    fs::write(&missing, b"returned cover").unwrap();
    let b = destination.backend();
    let path: String = b
        .conn
        .query_row("SELECT artwork_url FROM songs LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(Path::new(&path).starts_with(destination.0.canonicalize().unwrap()));
    assert_eq!(fs::read(path).unwrap(), b"returned cover");
}

#[test]
fn importing_tauri_library_captures_wal_and_preserves_existing_destination() {
    let source = Fixture::new();
    let backend = source.backend();
    seed(&backend);
    let destination = Fixture::new();
    import_library(&source.0.join("orca.db"), &destination.0).unwrap();
    let copied = Connection::open(destination.0.join("orca.db")).unwrap();
    assert_eq!(db::get_all_songs(&copied).unwrap().len(), 3);
    copied.execute("DELETE FROM songs", []).unwrap();
    import_library(&source.0.join("orca.db"), &destination.0).unwrap();
    assert!(db::get_all_songs(&copied).unwrap().is_empty());
    assert_eq!(backend.statistics().unwrap().songs, 3);
}

#[test]
fn failed_artwork_copy_preserves_references_and_can_retry() {
    let source = Fixture::new();
    let destination = Fixture::new();
    let cover = source.0.join("cover.png");
    fs::write(&cover, b"cover bytes").unwrap();
    let backend = destination.backend();
    seed(&backend);
    backend
        .conn
        .execute("UPDATE songs SET artwork_url=?1", [cover.to_str().unwrap()])
        .unwrap();
    backend
        .conn
        .execute(
            "DELETE FROM settings WHERE key='native_artwork_migration_v1'",
            [],
        )
        .unwrap();
    drop(backend);
    let blocked = destination.0.join("artwork/imported");
    fs::write(&blocked, b"blocks directory creation").unwrap();
    assert!(new_backend(destination.0.to_str().unwrap(), false).is_err());
    let conn = Connection::open(destination.0.join("orca.db")).unwrap();
    let saved: String = conn
        .query_row("SELECT artwork_url FROM songs LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(saved, cover.to_str().unwrap());
    assert!(db::get_setting(&conn, "native_artwork_migration_v1").is_none());
    drop(conn);
    fs::remove_file(blocked).unwrap();
    let backend = destination.backend();
    let saved: String = backend
        .conn
        .query_row("SELECT artwork_url FROM songs LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fs::read(saved).unwrap(), b"cover bytes");
    assert_eq!(fs::read(cover).unwrap(), b"cover bytes");
}

#[test]
fn imported_legacy_profile_survives_upgrade_and_second_startup() {
    let source = Fixture::new();
    let old = Connection::open(source.0.join("orca.db")).unwrap();
    old.execute_batch(r#"
        CREATE TABLE songs(id INTEGER PRIMARY KEY,title TEXT NOT NULL,artist TEXT NOT NULL,path TEXT NOT NULL UNIQUE,duration INTEGER NOT NULL,sample_rate INTEGER);
        INSERT INTO songs VALUES(1,'Legacy song','Artist','C:/Music/direct.flac',1000,44100);
        CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT);
        INSERT INTO settings VALUES('library_scan_roots','["C:/Music"]');
        CREATE TABLE lyrics(song_path TEXT PRIMARY KEY,lyrics_text TEXT NOT NULL);
        INSERT INTO lyrics VALUES('C:/Music/direct.flac','[00:00.00]Preserved lyrics');
        CREATE TABLE playlists(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,cover_path TEXT,created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
        INSERT INTO playlists(id,name,cover_path) VALUES(1,'Saved mix','chosen-cover.png');
        CREATE TABLE playlist_songs(id INTEGER PRIMARY KEY AUTOINCREMENT,playlist_id INTEGER NOT NULL,song_id INTEGER NOT NULL,position INTEGER NOT NULL);
        INSERT INTO playlist_songs VALUES(1,1,1,0);
    "#).unwrap();
    drop(old);
    let original = fs::read(source.0.join("orca.db")).unwrap();
    let destination = Fixture::new();
    import_library(&source.0.join("orca.db"), &destination.0).unwrap();
    prepare_data_dir(destination.0.to_str().unwrap()).unwrap();
    let backup = fs::read(destination.0.join("orca.pre-qt-backup.db")).unwrap();
    for _ in 0..2 {
        let backend = destination.backend();
        assert_eq!(backend.statistics().unwrap().songs, 1);
        assert_eq!(
            backend.track("C:/Music/direct.flac").unwrap().title,
            "Legacy song"
        );
        assert_eq!(
            backend.lyrics("C:/Music/direct.flac").unwrap(),
            "[00:00.00]Preserved lyrics"
        );
        assert_eq!(backend.library_roots().unwrap(), vec!["C:/Music"]);
        assert_eq!(
            backend.raw_groups("playlists", "").unwrap()[0].title,
            "Saved mix"
        );
        assert_eq!(
            backend
                .conn
                .query_row("SELECT cover_path FROM playlists WHERE id=1", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            "chosen-cover.png"
        );
        assert_eq!(
            backend
                .conn
                .query_row(
                    "SELECT song_id FROM playlist_songs WHERE playlist_id=1",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            backend
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        drop(backend);
        prepare_data_dir(destination.0.to_str().unwrap()).unwrap();
        import_library(&source.0.join("orca.db"), &destination.0).unwrap();
    }
    assert_eq!(fs::read(source.0.join("orca.db")).unwrap(), original);
    assert_eq!(
        fs::read(destination.0.join("orca.pre-qt-backup.db")).unwrap(),
        backup
    );
}

#[test]
fn shared_database_preserves_roots_covers_and_wal_backup() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_setting(&b.conn, "library_scan_roots", "[\"original-music\"]").unwrap();
    db::set_setting(&b.conn, "qt_scan_roots", "[\"prototype-music\"]").unwrap();
    db::update_artist_artwork(&b.conn, "A", Some("artist.webp"), Some("artist-thumb.webp"))
        .unwrap();
    db::update_album_artwork(
        &b.conn,
        "A:Shared",
        Some("album.webp"),
        Some("album-thumb.webp"),
    )
    .unwrap();
    assert_eq!(b.statistics().unwrap().roots, vec!["original-music"]);
    assert_eq!(
        b.groups("artists", "A").unwrap()[0].artwork,
        "artist-thumb.webp"
    );
    assert_eq!(
        b.groups("albums", "A")
            .unwrap()
            .iter()
            .find(|a| a.title == "Shared")
            .unwrap()
            .artwork,
        "album.webp"
    );
    prepare_data_dir(f.0.to_str().unwrap()).unwrap();
    let backup = Connection::open(f.0.join("orca.pre-qt-backup.db")).unwrap();
    assert_eq!(db::get_all_songs(&backup).unwrap().len(), 3);
    b.conn.execute("DELETE FROM songs", []).unwrap();
    prepare_data_dir(f.0.to_str().unwrap()).unwrap();
    assert_eq!(db::get_all_songs(&backup).unwrap().len(), 3);
    drop(backup);
    drop(b);
    let reopened = f.backend();
    assert_eq!(reopened.statistics().unwrap().roots, vec!["original-music"]);
    assert_eq!(reopened.statistics().unwrap().songs, 0);
}
