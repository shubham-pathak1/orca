use super::*;

#[test]
fn folder_cover_falls_back_to_first_available_song_and_edits_preserve_paths() {
    let f = Fixture::new();
    let b = f.backend();
    db::set_setting(&b.conn, "library_scan_roots", r#"["C:/Music"]"#).unwrap();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('C:/Music/a.flac','No cover','A','A','Album',1000)", []).unwrap();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,artwork_url) VALUES('C:/Music/Sub/b.flac','Cover','A','A','Album',2000,'song-cover.png')", []).unwrap();
    let group = b.folder_groups("", "").unwrap().remove(0);
    assert_eq!(group.artwork, "song-cover.png");
    assert_eq!(group.count, 2);
    b.operation(r#"{"action":"collection-edit","kind":"folders","key":"C:/Music","cover":""}"#)
        .unwrap();
    assert_eq!(b.folder_groups("", "Music").unwrap()[0].title, "Music");
    let detail = b
        .operation(r#"{"action":"group-detail","kind":"folders","key":"C:/Music","secondary":""}"#)
        .unwrap();
    let detail: serde_json::Value = serde_json::from_str(&detail).unwrap();
    assert_eq!(detail["artwork"], "song-cover.png");
    assert_eq!(detail["duration"], 3000);
    b.operation(r#"{"action":"collection-edit","kind":"folders","key":"C:/Music/Sub","secondary":"C:/Music"}"#).unwrap();
    assert_eq!(b.folder_groups("C:/Music", "").unwrap()[0].title, "Sub");
    assert_eq!(b.folder_group("C:/Music/Sub").unwrap().title, "Sub");
    assert!(b
        .operation(
            r#"{"action":"collection-edit","kind":"folders","key":"D:/NotAdded","name":"Invalid"}"#
        )
        .is_err());
    let paths: Vec<String> = b
        .conn
        .prepare("SELECT path FROM songs ORDER BY path")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(paths, ["C:/Music/Sub/b.flac", "C:/Music/a.flac"]);
    assert!(b.folder_exists("C:/Music/Sub").unwrap());
    b.conn
        .execute("DELETE FROM songs WHERE path='C:/Music/Sub/b.flac'", [])
        .unwrap();
    assert!(!b.folder_exists("C:/Music/Sub").unwrap());
    assert!(b.folder_exists("C:/Music").unwrap());
}

#[test]
fn folder_browsing_and_playback_respect_roots_and_nested_boundaries() {
    let f = Fixture::new();
    let b = f.backend();
    db::set_setting(
        &b.conn,
        "library_scan_roots",
        r#"["C:/Music_100%","D:/Other","E:/Empty"]"#,
    )
    .unwrap();
    for path in [
        "C:/Music_100%/direct.flac",
        "C:/Music_100%/Artist/a.flac",
        "C:/Music_100%/Artist/Album/b.flac",
        "C:/Music_100%-copy/outside.flac",
        "D:/Other/other.flac",
    ] {
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES(?1,?1,'A','A','Album',1000)", [path]).unwrap();
    }
    let roots = b.folder_groups("", "").unwrap();
    assert_eq!(roots.len(), 3);
    assert_eq!(roots[0].count, 3);
    assert_eq!(roots[2].count, 0);
    let children = b.folder_groups("C:/Music_100%", "").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].key, "C:/Music_100%/Artist");
    assert_eq!(children[0].secondary, "C:/Music_100%");
    assert_eq!(children[0].count, 2);
    assert_eq!(
        b.folder_groups("C:/Music_100%/Artist", "").unwrap()[0].title,
        "Album"
    );
    assert!(b.folder_groups("", "missing").unwrap().is_empty());
    let mut query = ffi::Query {
        kind: "folders".into(),
        key: "C:/Music_100%/".into(),
        ..Default::default()
    };
    assert_eq!(b.page(&query, 0, 128).unwrap().total, 3);
    let paths = b.context_paths(&query).unwrap();
    assert_eq!(paths.len(), 3);
    assert!(paths.iter().all(|path| !path.contains("-copy")));
    query.key = "C:/Music_100%/Artist/Album".into();
    assert_eq!(
        b.context_paths(&query).unwrap(),
        vec!["C:/Music_100%/Artist/Album/b.flac"]
    );
    query.key = "E:/Empty".into();
    assert!(b.context_paths(&query).unwrap().is_empty());
    b.conn
        .execute("DELETE FROM songs WHERE path LIKE '%/Artist/%'", [])
        .unwrap();
    assert!(b.folder_groups("C:/Music_100%", "").unwrap().is_empty());
    query.key = "C:/Music_100%".into();
    assert_eq!(b.page(&query, 0, 128).unwrap().total, 1);
}

#[test]
fn removing_source_preserves_files_other_roots_and_overlapping_sources() {
    let f = Fixture::new();
    let b = f.backend();
    let root = f.0.join("music");
    let nested = root.join("keep");
    let sibling = f.0.join("music-extra");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    for path in [
        root.join("remove.wav"),
        nested.join("keep.wav"),
        sibling.join("other.wav"),
    ] {
        fs::write(&path, "unchanged file").unwrap();
        b.conn.execute("INSERT INTO songs(path,title,artist,album,duration) VALUES(?1,'Song','Artist','Album',1000)",[path.to_str().unwrap()]).unwrap();
    }
    db::set_setting(
        &b.conn,
        "library_scan_roots",
        &serde_json::to_string(&[&root, &nested, &sibling]).unwrap(),
    )
    .unwrap();
    let playlist = db::create_playlist(&b.conn, "Sources", None).unwrap();
    let id = b
        .conn
        .query_row(
            "SELECT id FROM songs WHERE path=?1",
            [root.join("remove.wav").to_str().unwrap()],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    db::add_to_playlist(&b.conn, playlist, id).unwrap();
    let sources: serde_json::Value =
        serde_json::from_str(&b.operation(r#"{"action":"library-sources"}"#).unwrap()).unwrap();
    assert_eq!(sources["sources"][0]["songCount"], 2);
    b.operation(&serde_json::json!({"action":"remove-source","key":root}).to_string())
        .unwrap();
    assert_eq!(b.statistics().unwrap().songs, 2);
    assert!(root.join("remove.wav").is_file());
    assert_eq!(b.statistics().unwrap().roots.len(), 2);
    assert!(db::get_playlist_export_songs(&b.conn, playlist)
        .unwrap()
        .is_empty());
    drop(b);
    let reopened = f.backend();
    assert_eq!(reopened.statistics().unwrap().roots.len(), 2);
    assert!(reopened
        .operation(&serde_json::json!({"action":"remove-source","key":f.0}).to_string())
        .is_err());
}
