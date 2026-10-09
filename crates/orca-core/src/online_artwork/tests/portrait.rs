use super::*;

fn apple_response() -> Value {
    json!({"results":[{"artistId":947078210,"artistName":"Cafuné","trackName":"Friction","trackTimeMillis":187303,"collectionName":"Tek It/Friction"}]})
}

fn apple_page() -> String {
    format!(
        "<script type=\"application/ld+json\">{}</script>",
        json!({"@type":"MusicGroup","name":"Cafuné","url":"https://music.apple.com/us/artist/cafune/947078210","image":"https://is1-ssl.mzstatic.com/image/portrait.png"})
    )
}

#[test]
fn apple_portrait_uses_the_recording_id_and_stops_before_name_only_fallbacks() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Cafuné");
    query.title = Some("Friction".into());
    let script = Script::new(vec![("itunes.apple.com".into(), Ok(apple_response()))]);
    script.pages.borrow_mut().push_back((
        "music.apple.com/us/artist/-/947078210".into(),
        Ok(apple_page()),
    ));
    let stages = RefCell::new(Vec::new());
    assert!(lookup(&query, &f.0, LookupMode::Manual, |provider| {
        stages.borrow_mut().push(provider);
        script.clone()
    })
    .is_ok());
    assert_eq!(*stages.borrow(), [Provider::Itunes, Provider::AppleMusic]);
    assert_eq!(
        script.calls.borrow().last().unwrap(),
        "https://is1-ssl.mzstatic.com/image/portrait.png"
    );
}

#[test]
fn apple_page_timeout_preserves_deezer_fallback() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Cafuné");
    query.title = Some("Friction".into());
    let script = Script::new(vec![
        ("itunes.apple.com".into(), Ok(apple_response())),
        (
            "api.deezer.com/search?".into(),
            Ok(json!({"data":[{"title":"Friction","artist":{"name":"Cafuné","id":7}}]})),
        ),
        (
            "api.deezer.com/artist/7".into(),
            Ok(json!({"id":7,"name":"Cafuné","picture_xl":"https://image.test/portrait.png"})),
        ),
    ]);
    script
        .pages
        .borrow_mut()
        .push_back(("music.apple.com".into(), Err(LookupError::TimedOut)));
    assert!(lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).is_ok());
}

#[test]
fn musicbrainz_linked_apple_identity_fetches_portrait_when_itunes_search_is_missing() {
    let detail =
        json!({"relations":[{"url":{"resource":"https://music.apple.com/us/artist/947078210"}}]});
    let script = Script::new(vec![]);
    script.pages.borrow_mut().push_back((
        "music.apple.com/us/artist/-/947078210".into(),
        Ok(apple_page()),
    ));
    assert_eq!(
        super::super::portrait::from_relations(&detail, "Cafuné", &script).unwrap(),
        ["https://is1-ssl.mzstatic.com/image/portrait.png"]
    );
}

#[test]
fn manual_fetch_bypasses_success_cache_while_automatic_fetch_reuses_it() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Cafuné");
    query.title = Some("Friction".into());
    for _ in 0..2 {
        let script = Script::new(vec![("itunes.apple.com".into(), Ok(apple_response()))]);
        script
            .pages
            .borrow_mut()
            .push_back(("music.apple.com".into(), Ok(apple_page())));
        assert!(lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).is_ok());
        assert_eq!(
            script.calls.borrow().len(),
            3,
            "manual fetch must contact the provider even after a cached success"
        );
    }
    assert!(lookup(&query, &f.0, LookupMode::Automatic, |_| -> Script {
        panic!("automatic fetching must reuse valid cached artwork")
    })
    .is_ok());
}

#[test]
fn incomplete_provider_search_is_retryable_even_when_later_artist_has_no_portrait() {
    let f = Fixture::new();
    let query = ArtworkQuery::artist("Artist");
    let script = Script::new(vec![
        ("api.deezer.com".into(), Err(LookupError::TimedOut)),
        (
            "musicbrainz.org/ws/2/artist?".into(),
            Ok(json!({"artists":[{"id":"known","name":"Artist"}]})),
        ),
        ("artist/known?".into(), Ok(json!({"relations":[]}))),
    ]);
    assert_eq!(
        lookup(&query, &f.0, LookupMode::Automatic, |_| script.clone()).unwrap_err(),
        LookupError::TimedOut
    );
    assert!(cache::read(&f.0, &query, LookupMode::Automatic).is_none());
}

#[test]
#[ignore = "contacts live artwork providers; uses a disposable cache"]
fn live_cafune_lookup_uses_library_recordings_to_fetch_a_portrait() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Cafuné");
    query.title = Some("Friction".into());
    query.album = "Tek It/Friction".into();
    query.duration_ms = Some(187303);
    query.artist_recordings.push(ArtistRecording {
        title: "Tek It".into(),
        album: "Running".into(),
        duration_ms: Some(191823),
    });
    let paths = fetch(&query, &f.0, LookupMode::Manual).expect("Cafuné portrait lookup");
    assert!(image::image_dimensions(&paths.full).is_ok());
}

#[test]
#[ignore = "contacts live artwork providers; uses disposable caches"]
fn live_artist_lookup_matrix_resolves_recording_based_portraits() {
    for (artist, title, album, duration) in [
        ("Cafuné", "Friction", "Tek It/Friction", Some(187303)),
        ("Gini", "Sukoon", "Sukoon", Some(186000)),
        ("again&again", "baby", "baby", Some(153000)),
        ("Balu Brigada", "Designer", "Find A Way", None),
    ] {
        let f = Fixture::new();
        let mut query = ArtworkQuery::artist(artist);
        query.title = Some(title.into());
        query.album = album.into();
        query.duration_ms = duration;
        let started = std::time::Instant::now();
        let paths = fetch(&query, &f.0, LookupMode::Manual)
            .unwrap_or_else(|error| panic!("{artist}: {error}"));
        assert!(image::image_dimensions(paths.full).is_ok());
        println!("{artist}: valid portrait in {:?}", started.elapsed());
    }
}

#[test]
#[ignore = "contacts live artwork providers; uses actual tag values with a disposable cache"]
fn live_gini_lookup_combines_ansuna_and_atmos_sukoon() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Ansuna".into());
    query.album = "Ansuna".into();
    query.duration_ms = Some(224651);
    query.artist_recordings.push(ArtistRecording {
        title: "Sukoon".into(),
        album: "Sukoon (Dolby Atmos Version)".into(),
        duration_ms: Some(186393),
    });
    let started = std::time::Instant::now();
    let result = fetch(&query, &f.0, LookupMode::Manual);
    println!(
        "Gini exact-library lookup: {:?} in {:?}",
        result.as_ref().map(|_| "portrait"),
        started.elapsed()
    );
    if result.is_err() {
        let client = Http::new(Duration::from_secs(12));
        println!("iTunes identity: {:?}", artist::identify(&query, &client));
        for provider in [Provider::Deezer, Provider::MusicBrainz] {
            let client = Http::new(Duration::from_secs(12));
            println!(
                "{provider:?}: {:?}",
                providers::candidates(provider, &query, &client)
            );
        }
    }
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn cafune_songs_resolve_identity_and_linked_deezer_portrait_without_wikidata() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Cafuné");
    query.title = Some("Friction".into());
    query.album = "Tek It/Friction".into();
    query.duration_ms = Some(187303);
    query.artist_recordings.push(ArtistRecording {
        title: "Tek It".into(),
        album: "Running".into(),
        duration_ms: Some(191823),
    });
    let recording = |title: &str, duration: u64, album: &str| {
        json!({
            "title":title,"length":duration,"releases":[{"title":album}],
            "artist-credit":[{"artist":{"name":"CAFUNÉ","id":"duo"}}]
        })
    };
    let script = Script::new(vec![
        ("itunes.apple.com".into(), Ok(json!({"results":[]}))),
        ("itunes.apple.com".into(), Ok(json!({"results":[]}))),
        ("api.deezer.com/search?".into(), Ok(json!({"data":[]}))),
        ("api.deezer.com/search?".into(), Ok(json!({"data":[]}))),
        (
            "api.deezer.com/search/artist?".into(),
            Ok(json!({"data":[
                {"name":"Cafuné","id":1},{"name":"Cafuné","id":2}
            ]})),
        ),
        (
            "musicbrainz.org/ws/2/artist?".into(),
            Ok(json!({"artists":[
                {"name":"CAFUNÉ","id":"duo"},{"name":"Cafuné","id":"other"}
            ]})),
        ),
        (
            "musicbrainz.org/ws/2/recording?".into(),
            Ok(json!({"recordings":[recording("Friction",187303,"Tek It / Friction")]})),
        ),
        (
            "musicbrainz.org/ws/2/recording?".into(),
            Ok(json!({"recordings":[recording("Tek It",191823,"Running")]})),
        ),
        (
            "artist/duo?".into(),
            Ok(json!({"relations":[
                {"type":"free streaming","url":{"resource":"https://www.deezer.com/artist/135506262"}},
                {"type":"free streaming","url":{"resource":"https://www.deezer.com/artist/7171490"}}
            ]})),
        ),
        (
            "api.deezer.com/artist/135506262".into(),
            Err(LookupError::Unavailable),
        ),
        (
            "api.deezer.com/artist/7171490".into(),
            Ok(json!({"id":7171490,"name":"Cafuné","picture_xl":"https://image.test/duo.png"})),
        ),
    ]);
    assert!(lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).is_ok());
    assert_eq!(
        script.calls.borrow().last().unwrap(),
        "https://image.test/duo.png"
    );
    assert!(script.replies.borrow().is_empty());
}

#[test]
fn identified_artist_without_portrait_replaces_earlier_ambiguity_message() {
    let f = Fixture::new();
    let query = ArtworkQuery::artist("Cafuné");
    let script = Script::new(vec![
        (
            "api.deezer.com".into(),
            Ok(json!({"data":[{"id":1,"name":"Cafuné"},{"id":2,"name":"Cafuné"}]})),
        ),
        (
            "musicbrainz.org/ws/2/artist?".into(),
            Ok(json!({"artists":[{"id":"duo","name":"Cafuné"}]})),
        ),
        ("artist/duo?".into(), Ok(json!({"relations":[]}))),
    ]);
    assert_eq!(
        lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).unwrap_err(),
        LookupError::PortraitNotFound
    );
    assert_eq!(
        lookup(&query, &f.0, LookupMode::Automatic, |_| script.clone()).unwrap_err(),
        LookupError::NotFound
    );
    assert!(script.replies.borrow().is_empty());
}

#[test]
fn linked_portraits_reject_spoofed_hosts_duplicate_links_and_wrong_artist_details() {
    let detail = json!({"relations":[
        {"url":{"resource":"https://deezer.com.evil.test/artist/3"}},
        {"url":{"resource":"https://www.deezer.com/album/3"}},
        {"url":{"resource":"https://www.deezer.com/artist/1"}},
        {"url":{"resource":"https://www.deezer.com/artist/1"}},
        {"url":{"resource":"https://www.deezer.com/en/artist/2"}},
        {"url":{"resource":"https://www.deezer.com/artist/3"}}
    ]});
    let script = Script::new(vec![
        (
            "artist/1".into(),
            Ok(json!({"id":1,"name":"Other artist","picture_xl":"https://wrong"})),
        ),
        (
            "artist/2".into(),
            Ok(json!({"id":3,"name":"Cafuné","picture_xl":"https://wrong"})),
        ),
    ]);
    assert_eq!(
        super::super::portrait::from_relations(&detail, "Cafuné", &script).unwrap_err(),
        LookupError::PortraitNotFound
    );
    assert_eq!(script.calls.borrow().len(), 2);
}

#[test]
fn wikidata_failure_does_not_prevent_a_verified_linked_portrait() {
    let detail = json!({"relations":[
        {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q123"}},
        {"url":{"resource":"https://www.deezer.com/artist/7"}}
    ]});
    let script = Script::new(vec![
        ("wikidata.org".into(), Err(LookupError::Unavailable)),
        (
            "artist/7".into(),
            Ok(json!({"id":7,"name":"Cafuné","picture_xl":"https://image.test/duo.png"})),
        ),
    ]);
    assert_eq!(
        super::super::portrait::from_relations(&detail, "Cafuné", &script).unwrap(),
        ["https://image.test/duo.png"]
    );
}

#[test]
fn linked_portrait_misses_do_not_hide_an_earlier_timeout() {
    let detail = json!({"relations":[
        {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q123"}},
        {"url":{"resource":"https://www.deezer.com/artist/7"}}
    ]});
    let script = Script::new(vec![
        ("wikidata.org".into(), Err(LookupError::TimedOut)),
        ("artist/7".into(), Err(LookupError::NotFound)),
    ]);
    assert_eq!(
        super::super::portrait::from_relations(&detail, "Artist", &script).unwrap_err(),
        LookupError::TimedOut
    );
}
