use super::*;

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
#[ignore = "benchmark with 100000 synthetic tracks in a disposable library"]
fn large_library_catalog_benchmark() {
    let f = Fixture::new();
    let b = f.backend();
    let started = std::time::Instant::now();
    b.conn.execute_batch("WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<100000)
        INSERT INTO songs(path,title,artist,album,album_artist,genre,duration)
        SELECT 'song-'||i||'.flac',printf('Track %06d',i),'Artist '||(i%1000),'Album '||(i%10000),'Artist '||(i%1000),'Rock',180000 FROM n;").unwrap();
    eprintln!("100k synthetic inserts: {:?}", started.elapsed());
    for offset in [0, 50000, 99900] {
        let started = std::time::Instant::now();
        let page = b
            .page(
                &types::Query {
                    sort: "title".into(),
                    ..Default::default()
                },
                offset,
                128,
            )
            .unwrap();
        assert_eq!(page.total, 100000);
        assert_eq!(page.tracks.len(), if offset == 99900 { 100 } else { 128 });
        eprintln!("128-row page at {offset}: {:?}", started.elapsed());
    }
    let started = std::time::Instant::now();
    assert_eq!(b.groups("artists", "").unwrap().len(), 1000);
    assert_eq!(b.groups("albums", "").unwrap().len(), 10000);
    assert_eq!(
        b.context_paths(&types::Query::default()).unwrap().len(),
        100000
    );
    eprintln!(
        "artist/album catalogs + full playback context: {:?}",
        started.elapsed()
    );
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
