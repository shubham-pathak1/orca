use super::*;

#[test]
fn artist_case_variants_share_tracks_counts_and_existing_portraits() {
    let f = Fixture::new();
    let b = f.backend();
    b.conn
        .execute_batch(
            "INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES
        ('one.flac','One','Grentperez','Grentperez','A',1000),
        ('two.flac','Two','Grentperez','Grentperez','A',1000),
        ('three.flac','Three','grentperez','grentperez','B',1000);",
        )
        .unwrap();
    db::update_artist_artwork(
        &b.conn,
        "grentperez",
        Some("portrait.png"),
        Some("thumb.webp"),
    )
    .unwrap();
    let artists = b.groups("artists", "").unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].subtitle, "3 songs");
    assert_eq!(artists[0].artwork, "portrait.png");
    assert_eq!(b.statistics().unwrap().artists, 1);
    assert_eq!(
        b.context_ids(&types::Query {
            kind: "artists".into(),
            key: "GRENTPEReZ".into(),
            ..Default::default()
        })
        .unwrap()
        .len(),
        3
    );
    db::update_artist_artwork(&b.conn, "Grentperez", Some("new.png"), None).unwrap();
    assert_eq!(
        b.conn
            .query_row("SELECT COUNT(*) FROM artist_artworks", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(b.groups("artists", "").unwrap()[0].artwork, "new.png");
    let overrides = serde_json::json!({serde_json::json!(["artists","grentperez",""]).to_string():{"cover":"custom.png"}});
    db::set_setting(&b.conn, "collection_overrides", &overrides.to_string()).unwrap();
    assert_eq!(b.groups("artists", "").unwrap()[0].artwork, "custom.png");
    b.clear_collection_cover_override("artists", "Grentperez", "")
        .unwrap();
    assert_eq!(b.groups("artists", "").unwrap()[0].artwork, "new.png");
    db::remove_artist_artwork(&b.conn, "GRENTPEReZ").unwrap();
    assert!(!db::collection_needs_artwork(&b.conn, "Grentperez", None).unwrap());
    assert!(db::get_artists_needing_artwork(&b.conn).unwrap().is_empty());
    let names: Vec<String> = b
        .conn
        .prepare("SELECT artist FROM songs ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(names, ["Grentperez", "Grentperez", "grentperez"]);
}

#[test]
fn album_and_artist_grids_use_previews_and_artist_portraits_take_priority() {
    let f = Fixture::new();
    let b = f.backend();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,artwork_url,artwork_preview_url,artwork_thumb_url) VALUES('cover.flac','Song','Artist','Artist','Album',1000,'original.png','preview.webp','thumb.webp')",[]).unwrap();
    let albums = b.groups("albums", "").unwrap();
    assert_eq!(albums[0].artwork, "preview.webp");
    assert_eq!(b.groups("artists", "").unwrap()[0].artwork, "preview.webp");
    db::update_artist_artwork(
        &b.conn,
        "Artist",
        Some("portrait.png"),
        Some("tiny-thumb.webp"),
    )
    .unwrap();
    assert_eq!(b.groups("artists", "").unwrap()[0].artwork, "portrait.png");
}

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
        ('piya.flac','Piya','Khwaab, Divyam Sodhi, LIKHARI','Khwaab','Piya',141000,'piya.jpg');").unwrap();
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
    let query = types::Query {
        kind: "artists".into(),
        key: "A".into(),
        sort: "title".into(),
        ..Default::default()
    };
    assert_eq!(
        b.context_paths(&query).unwrap(),
        vec!["\u{4e00}.wav", "three.wav"]
    );
    let query = types::Query {
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
    let query = types::Query {
        kind: "playlists".into(),
        key: playlist.to_string(),
        sort: "artist".into(),
        ..Default::default()
    };
    assert_eq!(
        b.context_paths(&query).unwrap(),
        vec!["three.wav", "\u{4e00}.wav"]
    );
    let query = types::Query {
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
    let q = types::Query {
        search: "%".into(),
        ..Default::default()
    };
    let page = b.page(&q, 0, 128).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.tracks[0].path, "一.wav");
    let q = types::Query {
        kind: "albums".into(),
        key: "Shared".into(),
        secondary: "B".into(),
        ..Default::default()
    };
    assert_eq!(b.page(&q, 0, 128).unwrap().tracks[0].path, "two.wav");
    assert_eq!(b.statistics().unwrap().albums, 3);
    assert_eq!(
        b.page(&types::Query::default(), 1, 1).unwrap().tracks.len(),
        1
    );
    assert!(b
        .page(&types::Query::default(), 999, 128)
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
            &types::Query {
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
    let query = types::Query {
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

#[test]
fn table_sorts_are_numeric_stable_and_shared_by_pages_and_playback() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend
        .conn
        .execute_batch(
            "INSERT INTO songs(path,title,artist,album,album_artist,duration,track_number) VALUES
        ('a.flac','Alpha','Zulu','One','Band',600000,3),
        ('b.flac','beta','Alpha','One','Band',9000,1),
        ('c.flac','Beta','Alpha','One','Band',100000,2);",
        )
        .unwrap();
    for (sort, expected) in [
        ("title-asc", ["a.flac", "b.flac", "c.flac"]),
        ("title-desc", ["c.flac", "b.flac", "a.flac"]),
        ("artist-asc", ["b.flac", "c.flac", "a.flac"]),
        ("artist-desc", ["a.flac", "c.flac", "b.flac"]),
        ("album-asc", ["a.flac", "b.flac", "c.flac"]),
        ("album-desc", ["c.flac", "b.flac", "a.flac"]),
        ("duration-asc", ["b.flac", "c.flac", "a.flac"]),
        ("duration-desc", ["a.flac", "c.flac", "b.flac"]),
    ] {
        for kind in ["", "albums", "genres", "playlists"] {
            backend
                .conn
                .execute("UPDATE songs SET genre='Genre'", [])
                .unwrap();
            backend.conn.execute_batch("INSERT OR IGNORE INTO playlists(id,name) VALUES(1,'List'); INSERT OR IGNORE INTO playlist_songs(playlist_id,song_id,position) SELECT 1,id,id FROM songs;").unwrap();
            let mut query = types::Query {
                sort: sort.into(),
                kind: kind.into(),
                ..Default::default()
            };
            match kind {
                "albums" => {
                    query.key = "One".into();
                    query.secondary = "Band".into();
                }
                "genres" => query.key = "Genre".into(),
                "playlists" => query.key = "1".into(),
                _ => {}
            }
            let paths = backend.context_paths(&query).unwrap();
            assert_eq!(paths, expected, "{kind}: {sort}");
            let ids = backend.context_ids(&query).unwrap();
            for index in 0..3 {
                let page = backend.page(&query, index as u32, 1).unwrap();
                assert_eq!(page.tracks[0].path, expected[index]);
                assert_eq!(page.tracks[0].id, ids[index]);
            }
        }
    }
    let mut query = types::Query {
        sort: "title-desc".into(),
        ..Default::default()
    };
    assert_eq!(backend.locate(&query, "B").unwrap(), 0);
    assert_eq!(backend.locate(&query, "A").unwrap(), 2);
    query.sort = "artist-asc".into();
    assert_eq!(backend.locate(&query, "Z").unwrap(), 2);
    backend.conn.execute_batch("INSERT INTO songs(path,title,artist,duration) VALUES ('symbol.flac','#Intro','Band',1000),('unicode.flac',char(22812),'Band',1000);").unwrap();
    query.sort = "title-desc".into();
    assert_eq!(backend.locate(&query, "#").unwrap(), 4);
}

#[test]
fn folder_header_sort_overrides_paths_without_changing_default_order() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend
        .conn
        .execute_batch(
            "INSERT INTO songs(path,title,artist,duration) VALUES
        ('Music/a.flac','Zulu','Band',1000),('Music/b.flac','Alpha','Band',2000);",
        )
        .unwrap();
    let mut query = types::Query {
        kind: "folders".into(),
        key: "Music".into(),
        sort: "title".into(),
        ..Default::default()
    };
    assert_eq!(
        backend.context_paths(&query).unwrap(),
        ["Music/a.flac", "Music/b.flac"]
    );
    query.sort = "title-asc".into();
    assert_eq!(
        backend.context_paths(&query).unwrap(),
        ["Music/b.flac", "Music/a.flac"]
    );
}

#[test]
fn liked_state_is_loaded_for_unplayed_songs_in_every_track_lookup() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    seed(&backend);
    let track = backend.track("two.wav").unwrap();
    assert!(!track.liked);
    backend.set_song_liked(track.id, true).unwrap();
    assert!(backend.track("two.wav").unwrap().liked);
    assert!(backend.tracks_by_ids(&[track.id]).unwrap()[0].liked);
    assert!(
        backend
            .page(&types::Query::default(), 0, 128)
            .unwrap()
            .tracks
            .iter()
            .find(|item| item.id == track.id)
            .unwrap()
            .liked
    );
    backend.set_song_liked(track.id, false).unwrap();
    assert!(!backend.track("two.wav").unwrap().liked);
}

#[test]
fn album_years_use_known_positive_dates_and_preserve_album_identity() {
    let f = Fixture::new();
    let b = f.backend();
    for (path, artist, year) in [
        ("one", "A", Some(2015)),
        ("two", "A", None),
        ("three", "B", None),
    ] {
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,year) VALUES(?1,?1,?2,?2,'Shared',1000,?3)",params![path,artist,year]).unwrap();
    }
    let groups = b.groups("albums", "").unwrap();
    assert_eq!(
        groups
            .iter()
            .find(|group| group.secondary == "A")
            .unwrap()
            .year,
        Some(2015)
    );
    assert_eq!(
        groups
            .iter()
            .find(|group| group.secondary == "B")
            .unwrap()
            .year,
        None
    );
    let detail: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"group-detail","kind":"albums","key":"Shared","secondary":"A"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["year"], 2015);
}

#[test]
fn optional_year_and_composer_pages_query_only_the_selected_metadata() {
    let f = Fixture::new();
    let b = f.backend();
    for (path, year, composer) in [
        ("first", Some(2015), Some("Composer")),
        ("second", Some(2015), None),
        ("third", Some(2020), Some("Composer")),
        ("unknown", None, None),
    ] {
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,year,composer) VALUES(?1,?1,'Artist','Artist','Album',1000,?2,?3)",params![path,year,composer]).unwrap();
    }
    let years = b.groups("years", "").unwrap();
    assert_eq!(years.len(), 2);
    assert_eq!(years[0].key, "2020");
    assert_eq!(b.groups("composers", "").unwrap()[0].count, 2);
    let query = types::Query {
        kind: "years".into(),
        key: "2015".into(),
        ..Default::default()
    };
    assert_eq!(b.context_paths(&query).unwrap().len(), 2);
    let query = types::Query {
        kind: "composers".into(),
        key: "Composer".into(),
        ..Default::default()
    };
    assert_eq!(b.context_paths(&query).unwrap().len(), 2);
    let detail: serde_json::Value = serde_json::from_str(
        &b.operation(r#"{"action":"group-detail","kind":"years","key":"2015"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail["count"], 2);
    assert!(b.group_exists("composers", "Composer", "").unwrap());
    assert!(!b.group_exists("years", "2015 OR 1=1", "").unwrap());
}

#[test]
fn duration_and_recent_alphabet_jumps_find_titles_without_changing_sort_order() {
    let f = Fixture::new();
    let b = f.backend();
    for (path, title, duration) in [
        ("first", "Zulu", 1000),
        ("second", "Alpha", 2000),
        ("third", "Beta", 3000),
    ] {
        b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration) VALUES(?1,?2,'Artist','Artist','Album',?3)",params![path,title,duration]).unwrap();
    }
    for (sort, expected) in [("duration-asc", 1), ("duration-desc", 1)] {
        let query = types::Query {
            sort: sort.into(),
            ..Default::default()
        };
        assert_eq!(b.locate(&query, "A").unwrap(), expected);
    }
    let query = types::Query {
        sort: "recent".into(),
        ..Default::default()
    };
    let paths = b.context_paths(&query).unwrap();
    let expected = paths.iter().position(|path| path == "first").unwrap() as u32;
    assert_eq!(b.locate(&query, "Z").unwrap(), expected);
}
#[test]
fn facet_headers_use_local_full_covers_instead_of_thumbnails() {
    let f = Fixture::new();
    let b = f.backend();
    b.conn.execute("INSERT INTO songs(path,title,artist,album_artist,album,duration,year,composer,artwork_url,artwork_thumb_url) VALUES('test','Test','Artist','Artist','Album',1000,2024,'Composer','original.png','thumbnail.webp')",[]).unwrap();
    for (kind, key) in [("years", "2024"), ("composers", "Composer")] {
        let request = serde_json::json!({"action":"group-detail","kind":kind,"key":key});
        let detail: serde_json::Value =
            serde_json::from_str(&b.operation(&request.to_string()).unwrap()).unwrap();
        assert_eq!(detail["artwork"], "original.png");
    }
}

#[test]
fn older_artist_index_is_rebuilt_for_comma_credits_without_changing_tags() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute_batch("INSERT INTO songs(id,path,title,artist,duration) VALUES(9001,'comma.flac','Song','First, Second',1000);
        DELETE FROM artist_index_pending;
        INSERT INTO song_primary_artists(song_id,name) VALUES(9001,'First, Second');
        DELETE FROM orca_schema_migrations WHERE revision=8;").unwrap();
    drop(backend);
    let backend = fixture.backend();
    let groups = backend.groups("artists", "").unwrap();
    let names: std::collections::HashSet<_> = groups.iter().map(|g| g.key.as_str()).collect();
    assert!(names.contains("First") && names.contains("Second"));
    assert!(!names.contains("First, Second"));
    let original: String = backend
        .conn
        .query_row("SELECT artist FROM songs WHERE id=9001", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(original, "First, Second");
}
