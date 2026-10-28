use super::*;

#[test]
fn playback_ids_follow_the_visible_collection_order() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend
        .conn
        .execute_batch(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES
        ('b.flac','B','Artist','Artist','Album',1000),
        ('a.flac','A','Artist','Artist','Album',1000),
        ('x.flac','X','Other','Other','Other',1000);",
        )
        .unwrap();
    let query = types::Query {
        kind: "albums".into(),
        key: "Album".into(),
        secondary: "Artist".into(),
        ..Default::default()
    };
    let ids = backend.context_ids(&query).unwrap();
    let tracks = backend.tracks_by_ids(&[ids[1], -1, ids[0]]).unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| track.path.as_str())
            .collect::<Vec<_>>(),
        ["b.flac", "", "a.flac"]
    );
    let paths: Vec<_> = ids
        .into_iter()
        .map(|id| backend.path_for_id(id).unwrap().unwrap())
        .collect();
    assert_eq!(paths, ["a.flac", "b.flac"]);
    assert_eq!(
        backend.context_paths_limited(&query, 1).unwrap(),
        ["a.flac"]
    );
    assert_eq!(backend.id_for_path("missing.flac").unwrap(), None);
}

#[test]
fn indexed_artist_membership_tracks_edits_visibility_and_deletion() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES('a.flac','Track','Primary; Guest','Primary','Album',1000)", []).unwrap();
    let query = types::Query {
        kind: "artists".into(),
        key: "Guest".into(),
        ..Default::default()
    };
    assert_eq!(backend.context_ids(&query).unwrap().len(), 1);
    backend
        .conn
        .execute("UPDATE songs SET title='Track (feat. Guest)'", [])
        .unwrap();
    assert!(backend.context_ids(&query).unwrap().is_empty());
    backend
        .conn
        .execute("UPDATE songs SET title='Track'", [])
        .unwrap();
    assert_eq!(backend.context_ids(&query).unwrap().len(), 1);
    backend
        .conn
        .execute("UPDATE songs SET library_visible=0", [])
        .unwrap();
    assert!(backend.context_ids(&query).unwrap().is_empty());
    backend
        .conn
        .execute("UPDATE songs SET library_visible=1", [])
        .unwrap();
    assert_eq!(backend.context_ids(&query).unwrap().len(), 1);
    backend.conn.execute("DELETE FROM songs", []).unwrap();
    assert!(backend.context_ids(&query).unwrap().is_empty());
}

#[test]
fn collaborations_have_individual_artist_pages_while_featured_guests_do_not() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute_batch("INSERT INTO songs(path,title,artist,album_artist,album,duration,artwork_url) VALUES
        ('khaare.flac','Khaare Raste','Raghav Kaushik; Yashika Sikka','Raghav Kaushik','Khaare Raste',180000,'khaare.jpg'),
        ('visa.flac','visa (feat. Abhi The Nomad)','again&again; Abhi The Nomad','again&again','visa',166000,'visa.jpg'),
        ('piya.flac','Piya','Khwaab; Divyam Sodhi; LIKHARI','Khwaab','Piya',141000,'piya.jpg');").unwrap();
    let groups = backend.groups("artists", "").unwrap();
    let names: std::collections::HashSet<_> =
        groups.iter().map(|group| group.key.as_str()).collect();
    assert_eq!(groups.len(), 6);
    for name in [
        "Raghav Kaushik",
        "Yashika Sikka",
        "again&again",
        "Khwaab",
        "Divyam Sodhi",
        "LIKHARI",
    ] {
        assert!(names.contains(name), "missing {name}");
    }
    assert!(!names.contains("Abhi The Nomad"));
    assert!(!names.contains("Raghav Kaushik; Yashika Sikka"));
    assert_eq!(backend.statistics().unwrap().artists, 6);

    let query = types::Query {
        kind: "artists".into(),
        key: "Yashika Sikka".into(),
        ..Default::default()
    };
    let page = backend.page(&query, 0, 10).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.tracks[0].artist, "Raghav Kaushik; Yashika Sikka");
    assert_eq!(backend.context_paths(&query).unwrap(), ["khaare.flac"]);
    let detail: serde_json::Value = serde_json::from_str(
        &backend
            .operation(r#"{"action":"artist-detail","key":"Yashika Sikka"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["count"], 1);
    assert_eq!(detail["albumCount"], 1);
    assert_eq!(detail["artwork"], "khaare.jpg");

    let guest = types::Query {
        kind: "artists".into(),
        key: "Abhi The Nomad".into(),
        ..Default::default()
    };
    assert_eq!(backend.page(&guest, 0, 10).unwrap().total, 0);
    assert_eq!(
        db::get_artists_needing_artwork(&backend.conn)
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn song_pages_and_now_playing_preserve_the_explicit_flag() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend
        .conn
        .execute_batch(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration,explicit_advisory)
        VALUES('explicit.flac','Explicit song','Artist','Artist','Album',1000,1),
              ('unknown.flac','Unknown song','Artist','Artist','Album',1000,NULL);",
        )
        .unwrap();
    let page = backend.page(&types::Query::default(), 0, 10).unwrap();
    assert!(
        page.tracks
            .iter()
            .find(|song| song.path == "explicit.flac")
            .unwrap()
            .explicit
    );
    assert!(
        !page
            .tracks
            .iter()
            .find(|song| song.path == "unknown.flac")
            .unwrap()
            .explicit
    );
    assert!(backend.track("explicit.flac").unwrap().explicit);
}

#[test]
fn oversized_waveform_requests_reuse_the_decoder_sized_cache_without_opening_audio() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    let peaks = vec![0.25; orca_core::audio_engine::MAX_WAVEFORM_BUCKETS];
    db::save_waveform(&backend.conn, "two.wav", peaks.len(), &peaks).unwrap();
    assert_eq!(backend.waveform("two.wav", u32::MAX).unwrap(), peaks);
    assert_eq!(backend.waveform("two.wav", 2048).unwrap(), peaks);
}

#[test]
fn collection_lookup_binds_genre_and_album_identity() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute(
        "INSERT INTO songs(path,title,artist,album_artist,album,genre,duration) VALUES('one.flac','One','A','A','Shared','Alternative & Indie',1000)",
        [],
    ).unwrap();
    assert!(backend
        .group_exists("genres", "Alternative & Indie", "")
        .unwrap());
    assert!(!backend.group_exists("genres", "Missing", "").unwrap());
    assert!(backend.group_exists("albums", "Shared", "A").unwrap());
    assert!(!backend.group_exists("albums", "Shared", "B").unwrap());
}

#[test]
fn playback_context_paths_preserve_filter_and_album_playlist_order() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let query = ffi::Query {
        kind: "artists".into(),
        key: "A".into(),
        sort: "title".into(),
        ..Default::default()
    };
    assert_eq!(
        b.context_paths(&query).unwrap(),
        vec!["\u{4e00}.wav", "three.wav"]
    );
    let query = ffi::Query {
        search: "100%".into(),
        ..Default::default()
    };
    assert_eq!(b.context_paths(&query).unwrap(), vec!["\u{4e00}.wav"]);
    let playlist = db::create_playlist(&b.conn, "Playback", None).unwrap();
    for path in ["three.wav", "\u{4e00}.wav"] {
        let id = b
            .conn
            .query_row("SELECT id FROM songs WHERE path=?1", [path], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap();
        db::add_to_playlist(&b.conn, playlist, id).unwrap();
    }
    let query = ffi::Query {
        kind: "playlists".into(),
        key: playlist.to_string(),
        sort: "artist".into(),
        ..Default::default()
    };
    assert_eq!(
        b.context_paths(&query).unwrap(),
        vec!["three.wav", "\u{4e00}.wav"]
    );
    let query = ffi::Query {
        kind: "albums".into(),
        key: "Shared".into(),
        secondary: "B".into(),
        ..Default::default()
    };
    assert_eq!(b.context_paths(&query).unwrap(), vec!["two.wav"]);
}

#[test]
fn genre_tiles_deduplicate_previews_and_keep_case_distinct_groups() {
    let f = Fixture::new();
    let b = f.backend();
    for index in 0..7 {
        let cover = format!("preview-{}.webp", index.min(5));
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,genre,artwork_preview_url,artwork_thumb_url) VALUES(?1,?1,'A','A','Album',1000,'Pop',?2,'thumb.webp')",params![format!("Track {index}"),cover]).unwrap();
    }
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,genre,artwork_thumb_url) VALUES('lower','lower','A','A','Album',1000,'pop','fallback-thumb.webp')",[]).unwrap();
    let groups = b.groups("genres", "").unwrap();
    let upper = groups.iter().find(|g| g.key == "Pop").unwrap();
    assert_eq!(upper.count, 7);
    assert_eq!(
        upper.artwork_tiles,
        vec![
            "preview-0.webp",
            "preview-1.webp",
            "preview-2.webp",
            "preview-3.webp"
        ]
    );
    let lower = groups.iter().find(|g| g.key == "pop").unwrap();
    assert_eq!(lower.count, 1);
    assert_eq!(lower.artwork_tiles, vec!["fallback-thumb.webp"]);
}

#[test]
fn enqueue_group_spans_pages_without_duplicate_or_missing_tracks() {
    let f = Fixture::new();
    let b = f.backend();
    b.conn.execute("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<600) INSERT INTO songs(path,title,artist,album_artist,album,duration,genre) SELECT 'track-'||i,'Track '||i,'A','A','Album',1000,'Rock' FROM n",[]).unwrap();
    let queue: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"group-queue","kind":"genres","key":"Rock"}"#)
            .unwrap(),
    )
    .unwrap();
    let songs = queue["songs"].as_array().unwrap();
    assert_eq!(songs.len(), 600);
    let paths: std::collections::HashSet<_> =
        songs.iter().map(|s| s["path"].as_str().unwrap()).collect();
    assert_eq!(paths.len(), 600);
}

#[test]
fn pages_filter_literal_search_and_distinguish_album_artists() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let q = ffi::Query {
        search: "%".into(),
        ..Default::default()
    };
    let page = b.page(&q, 0, 128).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.tracks[0].path, "一.wav");
    let q = ffi::Query {
        kind: "albums".into(),
        key: "Shared".into(),
        secondary: "B".into(),
        ..Default::default()
    };
    assert_eq!(b.page(&q, 0, 128).unwrap().tracks[0].path, "two.wav");
    assert_eq!(b.statistics().unwrap().albums, 3);
    assert_eq!(
        b.page(&ffi::Query::default(), 1, 1).unwrap().tracks.len(),
        1
    );
    assert!(b
        .page(&ffi::Query::default(), 999, 128)
        .unwrap()
        .tracks
        .is_empty());
}

#[test]
fn recently_added_library_orders_newest_ids_first() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let page = b
        .page(
            &ffi::Query {
                sort: "recent".into(),
                ..Default::default()
            },
            0,
            128,
        )
        .unwrap();
    assert!(page.tracks.len() > 1);
    assert!(page.tracks.windows(2).all(|pair| pair[0].id > pair[1].id));
}

#[test]
fn album_pages_follow_track_numbers_instead_of_library_sort() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.conn
        .execute(
            "UPDATE songs SET album='Solo',track_number=1 WHERE path='two.wav'",
            [],
        )
        .unwrap();
    b.conn
        .execute("UPDATE songs SET track_number=2 WHERE path='three.wav'", [])
        .unwrap();
    b.conn
        .execute("UPDATE songs SET album_artist='A' WHERE path='two.wav'", [])
        .unwrap();
    let query = ffi::Query {
        kind: "albums".into(),
        key: "Solo".into(),
        secondary: "A".into(),
        sort: "title".into(),
        ..Default::default()
    };
    let page = b.page(&query, 0, 128).unwrap();
    assert_eq!(page.tracks[0].path, "two.wav");
    assert_eq!(page.tracks[0].track_number, 1);
    assert_eq!(page.tracks[1].track_number, 2);
}
