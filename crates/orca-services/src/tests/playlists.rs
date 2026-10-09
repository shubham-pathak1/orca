use super::*;

#[test]
fn failed_playlist_import_rolls_back_playlist_and_all_entries() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON playlist_songs WHEN NEW.song_id=2 BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    let file = f.0.join("failed.m3u8");
    fs::write(&file, "#EXTM3U\n../two.wav\n").unwrap();
    // Absolute indexed paths avoid dependence on the process working directory.
    let one = f.0.join("one.wav");
    let two = f.0.join("two.wav");
    b.conn
        .execute(
            "UPDATE songs SET path=?1 WHERE id=1",
            [one.to_string_lossy().as_ref()],
        )
        .unwrap();
    b.conn
        .execute(
            "UPDATE songs SET path=?1 WHERE id=2",
            [two.to_string_lossy().as_ref()],
        )
        .unwrap();
    fs::write(&file, format!("{}\n{}\n", one.display(), two.display())).unwrap();
    let request = operation_types::OperationRequest::ImportPlaylist {
        file: file.to_string_lossy().into(),
    };
    assert!(b.execute(&request).is_err());
    assert_eq!(db::get_playlists(&b.conn).unwrap().len(), 0);
    assert_eq!(
        b.conn
            .query_row("SELECT count(*) FROM playlist_songs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn playlist_overview_and_detail_use_the_first_available_cover_or_override() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let id = db::create_playlist(&b.conn, "Fallback", None).unwrap();
    let ids: Vec<i64> = b
        .conn
        .prepare("SELECT id FROM songs ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for song in &ids {
        db::add_to_playlist(&b.conn, id, *song).unwrap();
    }
    b.conn.execute("UPDATE songs SET artwork_url='full.png',artwork_preview_url='',artwork_thumb_url='second.webp' WHERE id=?1",[ids[1]]).unwrap();
    let detail = || {
        serde_json::from_str::<serde_json::Value>(
            &b.operation(&format!(
                r#"{{"action":"group-detail","kind":"playlists","key":"{id}"}}"#
            ))
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(b.groups("playlists", "").unwrap()[0].artwork, "second.webp");
    assert_eq!(detail()["artwork"], "second.webp");
    db::update_playlist_cover(&b.conn, id, Some("selected.png")).unwrap();
    assert_eq!(
        b.groups("playlists", "").unwrap()[0].artwork,
        "selected.png"
    );
    assert_eq!(detail()["artwork"], "selected.png");
    db::update_playlist_cover(&b.conn, id, None).unwrap();
    b.conn
        .execute(
            "UPDATE songs SET artwork_url=NULL,artwork_preview_url=NULL,artwork_thumb_url=NULL",
            [],
        )
        .unwrap();
    assert!(b.groups("playlists", "").unwrap()[0].artwork.is_empty());
    assert!(detail()["artwork"].is_null());
}

#[test]
fn playlist_import_deduplicates_and_exports_in_file_order() {
    let f = Fixture::new();
    let b = f.backend();
    for name in ["first.wav", "second.wav"] {
        let path = f.0.join(name);
        fs::write(&path, b"fixture").unwrap();
        b.conn
            .execute(
                "INSERT INTO songs(path,title,artist,duration) VALUES(?1,?2,'Artist',12000)",
                params![path.to_str().unwrap(), name],
            )
            .unwrap();
    }
    let import = f.0.join("Ordered.m3u8");
    fs::write(
        &import,
        "\u{feff}#EXTM3U\nsecond.wav\nfirst.wav\nsecond.wav\nmissing.wav\n",
    )
    .unwrap();
    let request = serde_json::json!({"action":"import-playlist", "file":import});
    let result: serde_json::Value =
        serde_json::from_str(&b.operation(&request.to_string()).unwrap()).unwrap();
    assert_eq!(result["imported"], 2);
    assert_eq!(result["unavailable"], 1);
    let group = b.groups("playlists", "").unwrap().remove(0);
    let query = ffi::Query {
        kind: "playlists".into(),
        key: group.key.clone(),
        sort: "title".into(),
        ..Default::default()
    };
    let page = b.page(&query, 0, 128).unwrap();
    assert_eq!(page.tracks[0].title, "second.wav");
    assert_eq!(page.tracks[1].title, "first.wav");
    let export = f.0.join("Exported.m3u8");
    let request = serde_json::json!({"action":"export-playlist", "id":group.key.parse::<i64>().unwrap(), "file":export});
    b.operation(&request.to_string()).unwrap();
    let exported = fs::read_to_string(export).unwrap();
    let entries: Vec<_> = exported
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(Path::new(entries[0]).file_name().unwrap(), "second.wav");
    assert_eq!(Path::new(entries[1]).file_name().unwrap(), "first.wav");
}
