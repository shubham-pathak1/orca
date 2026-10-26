use super::*;

#[test]
fn liked_songs_are_a_permanent_playable_collection() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    let playlists = backend.groups("playlists", "").unwrap();
    assert_eq!(playlists[0].key, "liked");
    assert_eq!(playlists[0].count, 0);
    assert!(playlists[0].artwork.is_empty());
    backend
        .conn
        .execute(
            "UPDATE songs SET artwork_preview_url='liked-preview.png' WHERE id=2",
            [],
        )
        .unwrap();
    let library_revision = backend.snapshot().unwrap().revision;
    backend.set_song_liked(2, true).unwrap();
    assert_eq!(backend.snapshot().unwrap().revision, library_revision);
    assert!(backend.is_song_liked(2).unwrap());
    let playlists = backend.groups("playlists", "").unwrap();
    assert_eq!(playlists[0].count, 1);
    assert_eq!(playlists[0].artwork, "liked-preview.png");
    let query = types::Query {
        kind: "playlists".into(),
        key: "liked".into(),
        sort: "title".into(),
        ..Default::default()
    };
    assert_eq!(backend.page(&query, 0, 20).unwrap().tracks[0].id, 2);
    assert_eq!(backend.context_paths(&query).unwrap(), vec!["two.wav"]);
    let detail: serde_json::Value = serde_json::from_str(
        &backend
            .operation(r#"{"action":"group-detail","kind":"playlists","key":"liked"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["title"], "Liked Songs");
    assert_eq!(detail["count"], 1);
    assert_eq!(detail["artwork"], "liked-preview.png");
    let custom_cover = fixture.0.join("liked-cover.png");
    image::save_buffer(
        &custom_cover,
        &[238, 142, 110, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    backend
        .operation(
            &serde_json::json!({"action":"collection-edit","kind":"playlists","key":"liked","cover":custom_cover}).to_string(),
        )
        .unwrap();
    let custom_artwork = backend.groups("playlists", "").unwrap()[0].artwork.clone();
    assert_ne!(custom_artwork, "liked-preview.png");
    assert!(std::path::Path::new(&custom_artwork).exists());
    backend
        .operation(r#"{"action":"collection-edit","kind":"playlists","key":"liked","cover":""}"#)
        .unwrap();
    assert_eq!(
        backend.groups("playlists", "").unwrap()[0].artwork,
        "liked-preview.png"
    );
    let queue: serde_json::Value = serde_json::from_str(
        &backend
            .operation(r#"{"action":"group-queue","kind":"playlists","key":"liked"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(queue["songs"][0]["path"], "two.wav");
    assert!(backend.song_playlist_ids(2).unwrap().is_empty());
    backend.set_song_liked(2, false).unwrap();
    assert_eq!(backend.snapshot().unwrap().revision, library_revision);
    assert_eq!(backend.page(&query, 0, 20).unwrap().total, 0);
    assert_eq!(backend.groups("playlists", "").unwrap()[0].key, "liked");
}

#[test]
fn playlist_letter_lookup_respects_search_filter() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    backend.set_song_liked(1, true).unwrap();
    backend.set_song_liked(2, true).unwrap();
    let query = types::Query {
        kind: "playlists".into(),
        key: "liked".into(),
        search: "Other".into(),
        sort: "title".into(),
        ..Default::default()
    };
    assert_eq!(backend.locate(&query, "Z").unwrap(), 1);
}

#[test]
fn song_playlist_membership_follows_add_and_remove() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    let first = db::create_playlist(&backend.conn, "First", None).unwrap();
    let second = db::create_playlist(&backend.conn, "Second", None).unwrap();
    assert!(backend.song_playlist_ids(1).unwrap().is_empty());
    backend
        .edit_playlist(crate::types::PlaylistEdit::Add {
            id: first,
            song_id: 1,
        })
        .unwrap();
    backend
        .edit_playlist(crate::types::PlaylistEdit::Add {
            id: second,
            song_id: 1,
        })
        .unwrap();
    let mut memberships = backend.song_playlist_ids(1).unwrap();
    memberships.sort();
    assert_eq!(memberships, vec![first, second]);
    backend
        .edit_playlist(crate::types::PlaylistEdit::Remove {
            id: first,
            song_id: 1,
        })
        .unwrap();
    assert_eq!(backend.song_playlist_ids(1).unwrap(), vec![second]);
}

#[test]
fn batch_removal_updates_only_the_chosen_collection() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    let first = db::create_playlist(&backend.conn, "First", None).unwrap();
    let second = db::create_playlist(&backend.conn, "Second", None).unwrap();
    for id in [1, 2] {
        db::add_to_playlist(&backend.conn, first, id).unwrap();
        db::add_to_playlist(&backend.conn, second, id).unwrap();
        backend.set_song_liked(id, true).unwrap();
    }
    backend
        .edit_playlist(crate::types::PlaylistEdit::RemoveMany {
            id: Some(first),
            song_ids: vec![1, 2],
        })
        .unwrap();
    assert!(db::get_playlist_song_ids(&backend.conn, first)
        .unwrap()
        .is_empty());
    assert_eq!(
        db::get_playlist_song_ids(&backend.conn, second).unwrap(),
        vec![1, 2]
    );
    assert!(backend.is_song_liked(1).unwrap());
    backend
        .edit_playlist(crate::types::PlaylistEdit::RemoveMany {
            id: None,
            song_ids: vec![1],
        })
        .unwrap();
    assert!(!backend.is_song_liked(1).unwrap());
    assert!(backend.is_song_liked(2).unwrap());
    assert_eq!(
        db::get_playlist_song_ids(&backend.conn, second).unwrap(),
        vec![1, 2]
    );
}

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
    assert_eq!(user_playlist(&b).artwork, "second.webp");
    assert_eq!(detail()["artwork"], "second.webp");
    db::update_playlist_cover(&b.conn, id, Some("selected.png")).unwrap();
    assert_eq!(user_playlist(&b).artwork, "selected.png");
    assert_eq!(detail()["artwork"], "selected.png");
    db::update_playlist_cover(&b.conn, id, None).unwrap();
    b.conn
        .execute(
            "UPDATE songs SET artwork_url=NULL,artwork_preview_url=NULL,artwork_thumb_url=NULL",
            [],
        )
        .unwrap();
    assert!(user_playlist(&b).artwork.is_empty());
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
    let group = user_playlist(&b);
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
