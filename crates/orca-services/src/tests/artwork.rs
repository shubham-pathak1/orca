use super::*;

#[test]
#[ignore = "contacts artwork providers with a staged edit in a disposable library"]
fn editor_artwork_uses_current_draft_and_does_not_commit_it() {
    let f = Fixture::new();
    let b = f.backend();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('draft.flac','Wrong old title','Wrong old artist','Wrong old artist','Wrong old album',0)",[]).unwrap();
    let request=operation_types::OperationRequest::from_value(serde_json::json!({"action":"fetch-cover","kind":"albums","key":"Malcolm Todd:Two New Malcolm Todd Songs","artist":"Malcolm Todd","album":"Two New Malcolm Todd Songs","title":"You Owe Me","track_artist":"Malcolm Todd","editor_path":"draft.flac","editor_generation":1})).unwrap();
    let operation_types::OperationResult::Artwork(result) = b.execute(&request).unwrap() else {
        panic!("Expected artwork")
    };
    assert!(image::image_dimensions(result.artwork).unwrap().0 >= 256);
    assert_eq!(b.track("draft.flac").unwrap().title, "Wrong old title");
    assert_eq!(b.track("draft.flac").unwrap().artwork_original, "");
}

#[test]
fn removed_artist_cover_uses_available_song_artwork_and_new_portrait_wins() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_artist_artwork(
        &b.conn,
        "A",
        Some("old-online-portrait.png"),
        Some("old-online-thumb.png"),
    )
    .unwrap();
    b.conn
        .execute(
            "UPDATE songs SET artwork_url='',artwork_thumb_url='' WHERE artist='A'",
            [],
        )
        .unwrap();
    b.conn.execute("UPDATE songs SET artwork_preview_url='song-preview.webp' WHERE id=(SELECT MAX(id) FROM songs WHERE artist='A')",[]).unwrap();
    b.operation(r#"{"action":"collection-edit","kind":"artists","key":"A","cover":""}"#)
        .unwrap();
    let detail = || {
        serde_json::from_str::<serde_json::Value>(
            &b.operation(r#"{"action":"artist-detail","key":"A"}"#)
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(detail()["artwork"], "song-preview.webp");
    let group = b.groups("artists", "A").unwrap().remove(0);
    assert_eq!(group.artwork, "song-preview.webp");
    db::update_artist_artwork(&b.conn, "A", Some("fetched.png"), Some("fetched-thumb.png"))
        .unwrap();
    assert_eq!(detail()["artwork"], "fetched.png");
    assert_eq!(
        b.groups("artists", "A").unwrap()[0].artwork,
        "fetched-thumb.png"
    );
}

#[test]
fn empty_saved_album_cover_override_uses_song_artwork_without_losing_name() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_album_artwork(
        &b.conn,
        "A:Shared",
        Some("old-online-album.png"),
        Some("old-online-album-thumb.png"),
    )
    .unwrap();

    b.conn.execute("UPDATE songs SET artwork_url='original.png',artwork_preview_url='preview.webp',artwork_thumb_url='thumb.webp' WHERE album_artist='A' AND album='Shared'",[]).unwrap();
    b.operation(
        r#"{"action":"collection-edit","kind":"albums","key":"Shared","secondary":"A","cover":""}"#,
    )
    .unwrap();
    let before = b.collection_overrides().unwrap();
    let group = b
        .groups("albums", "")
        .unwrap()
        .into_iter()
        .find(|album| album.key == "Shared" && album.secondary == "A")
        .unwrap();
    assert_eq!(group.title, "Shared");
    assert_eq!(group.artwork, "preview.webp");
    let read = |action: &str, kind: &str, key: &str, secondary: &str| {
        serde_json::from_str::<serde_json::Value>(
            &b.operation(
                &serde_json::json!({"action":action,"kind":kind,"key":key,"secondary":secondary})
                    .to_string(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let detail = read("group-detail", "albums", "Shared", "A");
    assert_eq!(detail["artwork"], "preview.webp");
    assert_eq!(detail["backdrop"], "thumb.webp");
    assert_eq!(detail["title"], "Shared");
    assert_eq!(
        read("collection-preview", "albums", "Shared", "A")["artwork"],
        "original.png"
    );
    let artist = read("artist-detail", "artists", "A", "");
    assert!(artist["albums"]
        .as_array()
        .unwrap()
        .iter()
        .any(|album| album["title"] == "Shared" && album["artwork"] == "preview.webp"));
    assert_eq!(
        b.collection_overrides().unwrap(),
        before,
        "fallback must not rewrite user settings"
    );
    b.conn.execute("UPDATE songs SET artwork_url=NULL,artwork_preview_url=NULL,artwork_thumb_url=NULL WHERE album_artist='A' AND album='Shared'",[]).unwrap();
    assert!(read("group-detail", "albums", "Shared", "A")["artwork"].is_null());
}

#[test]
fn album_overview_and_header_fall_back_to_available_song_artwork() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    for (original, preview, thumb, expected) in [
        ("original.png", "", "", "original.png"),
        ("", "preview.webp", "thumb.webp", "preview.webp"),
        ("", "", "thumb.webp", "thumb.webp"),
        ("", "", "", ""),
    ] {
        b.conn
            .execute(
                "UPDATE songs SET artwork_url=?1,artwork_preview_url=?2,artwork_thumb_url=?3",
                params![original, preview, thumb],
            )
            .unwrap();
        let groups = b.groups("albums", "").unwrap();
        assert!(groups.iter().all(|album| album.artwork == expected));
        let album = &groups[0];
        let request = serde_json::json!({"action":"group-detail","kind":"albums","key":album.key,"secondary":album.secondary});
        let detail: serde_json::Value =
            serde_json::from_str(&b.operation(&request.to_string()).unwrap()).unwrap();
        assert_eq!(detail["artwork"].as_str().unwrap_or(""), expected);
        if expected.is_empty() {
            assert!(detail["backdrop"].is_null());
        }
    }
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('first-without-art.wav','First','A','A','Solo',1000)",[]).unwrap();
    b.conn
        .execute(
            "UPDATE songs SET artwork_url='later-cover.png' WHERE path='three.wav'",
            [],
        )
        .unwrap();
    assert_eq!(
        b.groups("albums", "")
            .unwrap()
            .iter()
            .find(|album| album.key == "Solo")
            .unwrap()
            .artwork,
        "later-cover.png"
    );
    b.conn.execute("UPDATE songs SET artwork_url='song.png',artwork_preview_url=NULL,artwork_thumb_url=NULL WHERE artist='A'",[]).unwrap();
    db::update_album_artwork(
        &b.conn,
        "A:Shared",
        Some("custom.png"),
        Some("custom-thumb.webp"),
    )
    .unwrap();
    assert_eq!(
        b.groups("albums", "")
            .unwrap()
            .iter()
            .find(|album| album.key == "Shared" && album.secondary == "A")
            .unwrap()
            .artwork,
        "custom.png"
    );
}

#[test]
fn internal_header_backgrounds_use_matching_small_artwork_and_no_default_source() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn.execute("UPDATE songs SET artwork_url='song-original',artwork_preview_url='song-preview',artwork_thumb_url='song-thumb'",[]).unwrap();
    db::update_artist_artwork(&b.conn, "A", Some("artist-original"), Some("artist-thumb")).unwrap();
    db::update_album_artwork(
        &b.conn,
        "A:Album",
        Some("album-original"),
        Some("album-thumb"),
    )
    .unwrap();
    for (source, expected) in [
        ("song-preview", "song-thumb"),
        ("song-original", "song-thumb"),
        ("artist-original", "artist-thumb"),
        ("album-original", "album-thumb"),
        ("custom-cover", "custom-cover"),
    ] {
        assert_eq!(
            b.small_artwork(Some(source)).unwrap().as_deref(),
            Some(expected)
        );
    }
    assert_eq!(b.small_artwork(None).unwrap(), None);
    assert_eq!(b.small_artwork(Some("DELETED")).unwrap(), None);
    assert_eq!(b.small_artwork(Some("")).unwrap(), None);
    let artist: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-detail","key":"A"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(artist["backdrop"], "artist-thumb");
}

#[test]
fn missing_song_artwork_inherits_album_cover_without_replacing_embedded_art() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_album_artwork(
        &b.conn,
        "A:Shared",
        Some("album.png"),
        Some("album-thumb.png"),
    )
    .unwrap();
    let q = ffi::Query {
        kind: "songs".into(),
        ..Default::default()
    };
    let page = b.page(&q, 0, 128).unwrap();
    let song = page
        .tracks
        .iter()
        .find(|s| s.artist == "A" && s.album == "Shared")
        .unwrap();
    assert_eq!(song.artwork_original, "album.png");
    assert_eq!(song.artwork_thumb, "album-thumb.png");
    b.conn.execute("UPDATE songs SET artwork_url='embedded.png',artwork_preview_url='preview.webp' WHERE artist='A' AND album='Shared'",[]).unwrap();
    let page = b.page(&q, 0, 128).unwrap();
    let song = page
        .tracks
        .iter()
        .find(|s| s.artist == "A" && s.album == "Shared")
        .unwrap();
    assert_eq!(song.artwork_original, "embedded.png");
    assert_eq!(song.artwork, "preview.webp");
}

#[test]
fn catalog_details_and_queues_preserve_identity_covers_and_order() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn.execute("UPDATE songs SET genre='Rock', artwork_preview_url='preview.webp', artwork_thumb_url='thumb.webp',track_number=9 WHERE artist='A'",[]).unwrap();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,track_number,genre) VALUES('four.wav','First track','A','A','Shared',3000,1,'Rock')",[]).unwrap();
    db::update_album_artwork(
        &b.conn,
        "A:Shared",
        Some("custom.webp"),
        Some("custom-thumb.webp"),
    )
    .unwrap();
    let read = |request: &str| -> serde_json::Value {
        serde_json::from_str(&b.operation(request).unwrap()).unwrap()
    };
    let album = read(r#"{"action":"group-detail","kind":"albums","key":"Shared","secondary":"A"}"#);
    assert_eq!(album["count"], 2);
    assert_eq!(album["duration"], 4000);
    assert_eq!(album["artwork"], "custom.webp");
    assert_eq!(album["albums"].as_array().unwrap().len(), 1);
    assert_eq!(album["albums"][0]["key"], "A:Solo");
    let queue = read(r#"{"action":"group-queue","kind":"albums","key":"Shared","secondary":"A"}"#);
    assert_eq!(queue["songs"].as_array().unwrap().len(), 2);
    assert_eq!(queue["songs"][0]["path"], "four.wav");
    assert_eq!(queue["songs"][1]["title"], "100% Love");
    let genre = read(r#"{"action":"group-detail","kind":"genres","key":"Rock"}"#);
    assert_eq!(genre["count"], 3);
    assert_eq!(genre["duration"], 5000);
    assert_eq!(genre["artwork"], "preview.webp");
    assert!(genre["albums"].as_array().unwrap().is_empty());
    let id = db::create_playlist(&b.conn, "Mix", None).unwrap();
    let last: i64 = b
        .conn
        .query_row("SELECT id FROM songs WHERE path='three.wav'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let first: i64 = b
        .conn
        .query_row("SELECT id FROM songs WHERE path='four.wav'", [], |r| {
            r.get(0)
        })
        .unwrap();
    db::add_to_playlist(&b.conn, id, last).unwrap();
    db::add_to_playlist(&b.conn, id, first).unwrap();
    let playlist = read(&format!(
        r#"{{"action":"group-detail","kind":"playlists","key":"{id}"}}"#
    ));
    assert_eq!(playlist["title"], "Mix");
    assert_eq!(playlist["count"], 2);
    assert_eq!(playlist["artwork"], "preview.webp");
    let queue = read(&format!(
        r#"{{"action":"group-queue","kind":"playlists","key":"{id}"}}"#
    ));
    assert_eq!(queue["songs"][0]["path"], "three.wav");
    assert_eq!(queue["songs"][1]["path"], "four.wav");
    db::update_playlist_cover(&b.conn, id, Some("playlist.webp")).unwrap();
    let playlist = read(&format!(
        r#"{{"action":"group-detail","kind":"playlists","key":"{id}"}}"#
    ));
    assert_eq!(playlist["artwork"], "playlist.webp");
    let empty = db::create_playlist(&b.conn, "Empty", None).unwrap();
    let playlist = read(&format!(
        r#"{{"action":"group-detail","kind":"playlists","key":"{empty}"}}"#
    ));
    assert_eq!(playlist["count"], 0);
    assert!(playlist["artwork"].is_null());
    assert!(b
        .operation(
            r#"{"action":"group-detail","kind":"albums","key":"Shared","secondary":"missing"}"#
        )
        .is_err());
    assert!(b
        .operation(r#"{"action":"group-queue","kind":"unknown"}"#)
        .is_err());
}

#[test]
fn artist_details_use_full_cover_and_keep_album_artist_boundaries() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_artist_artwork(
        &b.conn,
        "A",
        Some("portrait.webp"),
        Some("portrait-thumb.webp"),
    )
    .unwrap();
    let detail: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-detail","key":"A"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["artwork"], "portrait.webp");
    assert_eq!(detail["backdrop"], "portrait-thumb.webp");
    assert_eq!(detail["count"], 2);
    assert_eq!(detail["duration"], 2000);
    assert_eq!(detail["albumCount"], 2);
    assert!(detail["albums"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["artist"] == "A"));
    let queue: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-queue","key":"A"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(queue["songs"].as_array().unwrap().len(), 2);
    assert!(queue["songs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["artist"] == "A"));
    assert!(b
        .operation(r#"{"action":"artist-detail","key":"missing"}"#)
        .is_err());
    db::remove_artist_artwork(&b.conn, "A").unwrap();
    b.conn.execute("UPDATE songs SET artwork_url='original.webp',artwork_thumb_url='thumb.webp' WHERE artist='A'",[]).unwrap();
    let fallback: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-detail","key":"A"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(fallback["artwork"], "original.webp");
    assert_eq!(fallback["backdrop"], "thumb.webp");
}

#[test]
fn artist_albums_include_compilations_without_merging_release_identities() {
    let f = Fixture::new();
    let b = f.backend();
    for (path, artist, album_artist) in [
        ("walters.flac", "The Walters", "Various Artists"),
        ("other.flac", "Another Artist", "Various Artists"),
        ("unrelated.flac", "Another Artist", "Another Artist"),
    ] {
        b.conn.execute(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES(?1,?1,?2,?3,'Love Pop Songs',1000)",
            params![path,artist,album_artist],
        ).unwrap();
    }
    let detail: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-detail","key":"The Walters"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["count"], 1);
    assert_eq!(detail["albumCount"], 1);
    let albums = detail["albums"].as_array().unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0]["title"], "Love Pop Songs");
    assert_eq!(albums[0]["artist"], "Various Artists");
    assert_eq!(albums[0]["song_count"], 2);
    let page = b
        .page(
            &crate::types::Query {
                kind: "albums".into(),
                key: "Love Pop Songs".into(),
                secondary: "Various Artists".into(),
                ..Default::default()
            },
            0,
            20,
        )
        .unwrap();
    assert_eq!(page.tracks.len(), 2);
    assert!(page
        .tracks
        .iter()
        .all(|track| track.album_artist == "Various Artists"));
}

#[test]
fn collection_edits_preserve_identity_and_removed_cover_survives_reopening() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let artist = b.groups("artists", "").unwrap().remove(0);
    let request = serde_json::json!({"action":"collection-edit","kind":"artists","key":artist.key,"cover":""});
    b.operation(&request.to_string()).unwrap();
    let group = b.groups("artists", "A").unwrap().remove(0);
    assert_eq!(group.key, artist.key);
    assert!(group.artwork.is_empty());
    let detail:serde_json::Value=serde_json::from_str(&b.operation(&serde_json::json!({"action":"artist-detail","kind":"artists","key":artist.key,"secondary":""}).to_string()).unwrap()).unwrap();
    assert_eq!(detail["title"], "A");
    assert!(detail["artwork"].is_null());
    assert!(detail["backdrop"].is_null());
    let query = ffi::Query {
        kind: "artists".into(),
        key: artist.key.clone(),
        ..Default::default()
    };
    assert!(b.page(&query, 0, 10).unwrap().total > 0);
    b.playlist_action("create", 0, 0, "Old playlist").unwrap();
    let playlist = b.groups("playlists", "").unwrap().remove(0);
    b.operation(&serde_json::json!({"action":"collection-edit","kind":"playlists","key":playlist.key,"name":"New playlist","cover":""}).to_string()).unwrap();
    assert_eq!(b.groups("playlists", "").unwrap()[0].title, "New playlist");
    drop(b);
    let reopened = f.backend();
    assert_eq!(reopened.groups("artists", "A").unwrap()[0].key, artist.key);
    assert!(reopened.groups("playlists", "").unwrap()[0]
        .artwork
        .is_empty());
}

#[test]
fn custom_collection_covers_are_copied_with_small_backgrounds_and_album_keys_stay_distinct() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn.execute("UPDATE songs SET genre='Rock'", []).unwrap();
    let cover = f.0.join("selected.png");
    image::RgbaImage::from_pixel(320, 320, image::Rgba([90, 140, 200, 255]))
        .save(&cover)
        .unwrap();
    for (kind, key, secondary) in [("genres", "Rock", ""), ("albums", "Shared", "A")] {
        b.operation(&serde_json::json!({"action":"collection-edit","kind":kind,"key":key,"secondary":secondary,"cover":cover}).to_string()).unwrap();
        let detail:serde_json::Value=serde_json::from_str(&b.operation(&serde_json::json!({"action":"group-detail","kind":kind,"key":key,"secondary":secondary}).to_string()).unwrap()).unwrap();
        assert_eq!(detail["title"], key);
        assert_eq!(
            image::image_dimensions(detail["artwork"].as_str().unwrap()).unwrap(),
            (320, 320)
        );
        assert_eq!(
            image::image_dimensions(detail["backdrop"].as_str().unwrap()).unwrap(),
            (80, 80)
        );
        assert_ne!(detail["artwork"].as_str().unwrap(), cover.to_str().unwrap());
    }
    let same_title_other_artist = b
        .groups("albums", "")
        .unwrap()
        .into_iter()
        .find(|g| g.key == "Shared" && g.secondary == "B")
        .unwrap();
    assert_eq!(same_title_other_artist.title, "Shared");
    let related: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"artist-detail","kind":"artists","key":"A","secondary":""}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(related["albums"][0]["navigationTitle"], "Shared");
    assert_eq!(related["albums"][0]["title"], "Shared");
    fs::remove_file(&cover).unwrap();
    assert!(Path::new(&b.groups("genres", "").unwrap()[0].artwork).is_file());
    b.playlist_action("create", 0, 0, "Unchanged").unwrap();
    let playlist = b.groups("playlists", "").unwrap().remove(0);
    assert!(b.operation(&serde_json::json!({"action":"collection-edit","kind":"playlists","key":playlist.key,"name":"Rejected","cover":cover}).to_string()).is_err());
    assert_eq!(b.groups("playlists", "").unwrap()[0].title, "Unchanged");
}

#[test]
fn collection_editor_uses_full_cover_without_changing_saved_overrides() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_artist_artwork(
        &b.conn,
        "A",
        Some("full-artist.png"),
        Some("small-artist.png"),
    )
    .unwrap();
    b.conn.execute("UPDATE songs SET artwork_url='full-album.png',artwork_preview_url='preview-album.png',artwork_thumb_url='small-album.png' WHERE album_artist='A' AND album='Shared'", []).unwrap();
    let preview = |kind: &str, key: &str, secondary: &str| {
        serde_json::from_str::<serde_json::Value>(&b.operation(&serde_json::json!({"action":"collection-preview","kind":kind,"key":key,"secondary":secondary}).to_string()).unwrap()).unwrap()
    };
    let before = b.collection_overrides().unwrap();
    assert_eq!(preview("artists", "A", "")["artwork"], "full-artist.png");
    assert_eq!(
        preview("albums", "Shared", "A")["artwork"],
        "full-album.png"
    );
    assert_eq!(b.collection_overrides().unwrap(), before);
    b.operation(
        r#"{"action":"collection-edit","kind":"albums","key":"Shared","secondary":"A","cover":""}"#,
    )
    .unwrap();
    assert_eq!(
        preview("albums", "Shared", "A")["artwork"],
        "full-album.png"
    );
}

#[test]
fn removed_artwork_is_eligible_for_refetch_and_new_cover_keeps_display_name() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::update_artist_artwork(&b.conn, "A", Some("old.png"), Some("old-thumb.png")).unwrap();
    b.operation(r#"{"action":"collection-edit","kind":"artists","key":"A","cover":""}"#)
        .unwrap();
    b.operation(
        r#"{"action":"collection-edit","kind":"albums","key":"Shared","secondary":"A","cover":""}"#,
    )
    .unwrap();
    let jobs: serde_json::Value =
        serde_json::from_str(&b.operation(r#"{"action":"missing-artwork"}"#).unwrap()).unwrap();
    let jobs = jobs["jobs"].as_array().unwrap();
    assert_eq!(
        jobs.iter()
            .filter(|job| job["kind"] == "artists" && job["key"] == "A")
            .count(),
        1
    );
    assert_eq!(
        jobs.iter()
            .filter(|job| job["kind"] == "albums" && job["key"] == "A:Shared")
            .count(),
        1
    );
    // A successful provider result must replace the removal override too.
    db::update_artist_artwork(&b.conn, "A", Some("new.png"), Some("new-thumb.png")).unwrap();
    b.clear_collection_cover_override("artists", "A", "")
        .unwrap();
    let group = b
        .groups("artists", "")
        .unwrap()
        .into_iter()
        .find(|g| g.key == "A")
        .unwrap();
    assert_eq!(group.title, "A");
    assert_eq!(group.artwork, "new-thumb.png");
}

#[test]
fn full_player_artwork_stays_separate_from_preview_and_thumbnail_paths() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn.execute("UPDATE songs SET artwork_url='original.png',artwork_preview_url='preview.webp',artwork_thumb_url='thumb.webp' WHERE path='two.wav'",[]).unwrap();
    let track = b.track("two.wav").unwrap();
    assert_eq!(track.artwork_original, "original.png");
    assert_eq!(track.artwork, "preview.webp");
    assert_eq!(track.artwork_thumb, "thumb.webp");
}
