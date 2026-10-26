use super::*;
use crate::migrate_profile_directory;
use rusqlite::Connection;

fn legacy_profile(fixture: &Fixture) -> (PathBuf, PathBuf) {
    let legacy = fixture.0.join("OrcaSlintTauri");
    let cover = legacy.join("artwork/cover.jpg");
    let font = legacy.join("fonts/custom.ttf");
    fs::create_dir_all(cover.parent().unwrap()).unwrap();
    fs::create_dir_all(font.parent().unwrap()).unwrap();
    fs::write(&cover, b"disposable cover").unwrap();
    fs::write(&font, b"disposable font").unwrap();
    fs::write(
        legacy.join("slint-settings.json"),
        serde_json::to_vec(&serde_json::json!({"font_path":font})).unwrap(),
    )
    .unwrap();
    fs::write(legacy.join("slint-session.json"), b"saved session").unwrap();
    let conn = db::init_db(legacy.clone()).unwrap();
    let old_cover = cover.to_string_lossy().into_owned();
    conn.execute(
        "INSERT INTO songs(path,title,artist,album_artist,album,duration,artwork_url,artwork_thumb_url,artwork_preview_url) VALUES('track.wav','Song','Artist','Artist','Album',1000,?1,?1,?1)",
        [&old_cover],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO artist_artworks(artist_name,artwork_path,artwork_thumb_path) VALUES('Artist',?1,?1)",
        [&old_cover],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO album_artworks(album_key,artwork_path,artwork_thumb_path) VALUES('Artist:Album',?1,?1)",
        [&old_cover],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO playlists(name,cover_path) VALUES('Mix',?1)",
        [&old_cover],
    )
    .unwrap();
    db::set_setting(
        &conn,
        "collection_overrides",
        &serde_json::json!({"artists:Artist":{"cover":old_cover,"thumb":old_cover}}).to_string(),
    )
    .unwrap();
    db::set_setting(&conn, "native_artwork_migration_v1", "complete").unwrap();
    (legacy, cover)
}

fn assert_profile_paths(fixture: &Fixture, legacy: &Path) {
    let current = fixture.0.join("Orca");
    let cover = current.join("artwork/cover.jpg");
    let font = current.join("fonts/custom.ttf");
    assert!(cover.is_file());
    assert!(font.is_file());
    assert_eq!(
        fs::read(current.join("slint-session.json")).unwrap(),
        b"saved session"
    );
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(current.join("slint-settings.json")).unwrap()).unwrap();
    assert_eq!(settings["font_path"], font.to_string_lossy().as_ref());
    let conn = Connection::open(current.join("orca.db")).unwrap();
    for (table, column) in [
        ("songs", "artwork_url"),
        ("songs", "artwork_thumb_url"),
        ("songs", "artwork_preview_url"),
        ("artist_artworks", "artwork_path"),
        ("artist_artworks", "artwork_thumb_path"),
        ("album_artworks", "artwork_path"),
        ("album_artworks", "artwork_thumb_path"),
        ("playlists", "cover_path"),
    ] {
        let actual: String = conn
            .query_row(
                &format!("SELECT {column} FROM {table} LIMIT 1"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actual, cover.to_string_lossy(), "{table}.{column}");
    }
    let overrides: serde_json::Value =
        serde_json::from_str(&db::get_setting(&conn, "collection_overrides").unwrap()).unwrap();
    assert_eq!(
        overrides["artists:Artist"]["cover"],
        cover.to_string_lossy().as_ref()
    );
    assert_eq!(
        db::get_setting(&conn, "native_profile_directory_v1").as_deref(),
        Some("complete")
    );
    assert!(!legacy.exists());
}

#[test]
fn profile_move_preserves_library_settings_and_local_artwork_paths() {
    let fixture = Fixture::new();
    let (legacy, _) = legacy_profile(&fixture);
    assert_eq!(
        migrate_profile_directory(&fixture.0).unwrap(),
        fixture.0.join("Orca")
    );
    assert_profile_paths(&fixture, &legacy);
    migrate_profile_directory(&fixture.0).unwrap();
    assert_profile_paths(&fixture, &legacy);
}

#[test]
fn interrupted_profile_move_repairs_paths_on_next_startup() {
    let fixture = Fixture::new();
    let (legacy, _) = legacy_profile(&fixture);
    fs::rename(&legacy, fixture.0.join("Orca")).unwrap();
    migrate_profile_directory(&fixture.0).unwrap();
    assert_profile_paths(&fixture, &legacy);
}

#[test]
fn occupied_new_folder_is_archived_before_moving_the_older_profile() {
    let fixture = Fixture::new();
    let (legacy, _) = legacy_profile(&fixture);
    let current = fixture.0.join("Orca");
    fs::create_dir_all(&current).unwrap();
    fs::write(current.join("notes.txt"), b"unrelated file").unwrap();
    assert_eq!(migrate_profile_directory(&fixture.0).unwrap(), current);
    assert!(!legacy.exists());
    assert!(current.join("orca.db").is_file());
    assert_eq!(
        fs::read(fixture.0.join("Orca.pre-migration-backup-1/notes.txt")).unwrap(),
        b"unrelated file"
    );
}

#[test]
fn two_unmigrated_libraries_prefer_the_older_native_library_and_keep_the_other() {
    let fixture = Fixture::new();
    let (legacy, _) = legacy_profile(&fixture);
    let current = fixture.0.join("Orca");
    fs::create_dir_all(&current).unwrap();
    let new_conn = db::init_db(current.clone()).unwrap();
    new_conn
        .execute(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('another.wav','Another','Artist','Artist','Album',1000)",
            [],
        )
        .unwrap();
    drop(new_conn);

    assert_eq!(migrate_profile_directory(&fixture.0).unwrap(), current);
    assert!(!legacy.exists());
    let conn = Connection::open(current.join("orca.db")).unwrap();
    let title: String = conn
        .query_row("SELECT title FROM songs LIMIT 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(title, "Song");
    let backup = Connection::open(fixture.0.join("Orca.pre-migration-backup-1/orca.db")).unwrap();
    let title: String = backup
        .query_row("SELECT title FROM songs LIMIT 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(title, "Another");
}

#[test]
fn migrated_profile_remains_authoritative_if_an_older_build_recreates_its_folder() {
    let fixture = Fixture::new();
    let (legacy, _) = legacy_profile(&fixture);
    let current = migrate_profile_directory(&fixture.0).unwrap();
    fs::create_dir_all(&legacy).unwrap();
    let old_conn = db::init_db(legacy.clone()).unwrap();
    old_conn
        .execute(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('stale.wav','Stale','Artist','Artist','Album',1000)",
            [],
        )
        .unwrap();
    drop(old_conn);

    assert_eq!(migrate_profile_directory(&fixture.0).unwrap(), current);
    let conn = Connection::open(current.join("orca.db")).unwrap();
    let title: String = conn
        .query_row("SELECT title FROM songs LIMIT 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(title, "Song");
    assert!(legacy.join("orca.db").is_file());
}
