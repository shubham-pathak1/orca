use super::*;
mod portrait;
use matching::{album_matches, names_match, track_matches};
use providers::Provider;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn artist_lookup_identifies_with_itunes_then_verifies_deezer_recording_and_downloads_portrait() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("GINI");
    query.title = Some("Sukoon".into());
    query.album = "Sukoon".into();
    query.duration_ms = Some(186000);
    let script = Script::new(vec![
        (
            "itunes.apple.com".into(),
            Ok(
                json!({"results":[{"artistId":123,"artistName":"Gini","trackName":"Sukoon","collectionName":"Sukoon","trackTimeMillis":186000,"artworkUrl100":"https://wrong.test/album.jpg"}]}),
            ),
        ),
        (
            "api.deezer.com/search?".into(),
            Ok(json!({"data":[
                {"artist":{"id":777,"name":"Gini"},"title":"Sukoon","album":{"title":"Sukoon"},"duration":186},
                {"artist":{"id":888,"name":"Gini"},"title":"Sukoon","album":{"title":"Other album"},"duration":186}
            ]})),
        ),
        (
            "api.deezer.com/artist/777".into(),
            Ok(json!({"id":777,"name":"Gini","picture_xl":"https://image.test/portrait.png"})),
        ),
    ]);
    let stages = RefCell::new(Vec::new());
    assert!(lookup(&query, &f.0, LookupMode::Manual, |provider| {
        stages.borrow_mut().push(provider);
        script.clone()
    })
    .is_ok());
    assert_eq!(
        *stages.borrow(),
        [Provider::Itunes, Provider::AppleMusic, Provider::Deezer]
    );
    assert_eq!(
        script.calls.borrow().last().unwrap(),
        "https://image.test/portrait.png"
    );
    assert!(!script
        .calls
        .borrow()
        .iter()
        .any(|url| url.contains("wrong.test")));
}

#[test]
fn itunes_identity_timeout_still_allows_a_verified_portrait_and_cancellation_stops_it() {
    let f = Fixture::new();
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Sukoon".into());
    let script = Script::new(vec![
        ("itunes.apple.com".into(), Err(LookupError::TimedOut)),
        ("api.deezer.com/search?".into(), Ok(json!({"data":[]}))),
        (
            "api.deezer.com/search/artist".into(),
            Ok(
                json!({"data":[{"id":1,"name":"Gini","picture_xl":"https://image.test/portrait.png"}]}),
            ),
        ),
    ]);
    assert!(lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).is_ok());
    let mut cancelled = Script::new(vec![("itunes.apple.com".into(), Ok(json!({"results":[]})))]);
    cancelled.cancel_after_json = true;
    let other = Fixture::new();
    assert_eq!(
        lookup(&query, &other.0, LookupMode::Manual, |_| cancelled.clone()).unwrap_err(),
        LookupError::Cancelled
    );
    assert_eq!(cancelled.calls.borrow().len(), 1);
}

#[test]
fn musicbrainz_disambiguates_names_with_recording_evidence() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Sukoon".into());
    query.duration_ms = Some(186000);
    let script = Script::new(vec![
        (
            "musicbrainz.org/ws/2/artist?".into(),
            Ok(
                json!({"artists":[{"id":"wrong-id","name":"Gini"},{"id":"right-id","name":"Gini"}]}),
            ),
        ),
        (
            "musicbrainz.org/ws/2/recording?".into(),
            Ok(
                json!({"recordings":[{"title":"Sukoon","length":186000,"artist-credit":[{"artist":{"name":"Gini","id":"right-id"}}]}]}),
            ),
        ),
        (
            "musicbrainz.org/ws/2/artist/right-id?".into(),
            Ok(json!({"relations":[]})),
        ),
    ]);
    assert_eq!(
        providers::candidates(Provider::MusicBrainz, &query, &script).unwrap_err(),
        LookupError::PortraitNotFound
    );
    assert_eq!(script.calls.borrow().len(), 3);
}

#[test]
fn conflicting_deezer_recordings_never_choose_the_first_artist() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Sukoon".into());
    let script = Script::new(vec![(
        "api.deezer.com/search?".into(),
        Ok(json!({"data":[
            {"title":"Sukoon","artist":{"id":1,"name":"Gini"}},
            {"title":"Sukoon","artist":{"id":2,"name":"Gini"}}
        ]})),
    )]);
    assert_eq!(
        providers::candidates(Provider::Deezer, &query, &script).unwrap_err(),
        LookupError::AmbiguousArtist
    );
    assert_eq!(script.calls.borrow().len(), 1);
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "orca-artwork-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
type Reply = (String, Result<Value, LookupError>);
type PageReply = (String, Result<String, LookupError>);

#[test]
fn other_songs_resolve_a_shared_title_without_choosing_the_first_artist() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Sukoon".into());
    query.artist_recordings.push(ArtistRecording {
        title: "Ansuna".into(),
        album: String::new(),
        duration_ms: Some(224000),
    });
    let script = Script::new(vec![
        (
            "track%3A%22Sukoon%22".into(),
            Ok(json!({"data":[
                {"title":"Sukoon","artist":{"id":1,"name":"Gini"}},
                {"title":"Sukoon","artist":{"id":2,"name":"Gini"}}
            ]})),
        ),
        (
            "track%3A%22Ansuna%22".into(),
            Ok(json!({"data":[{"title":"Ansuna","duration":224,"artist":{"id":2,"name":"Gini"}}]})),
        ),
        (
            "artist/2".into(),
            Ok(json!({"id":2,"name":"Gini","picture_xl":"https://image.test/right.png"})),
        ),
    ]);
    assert_eq!(
        providers::candidates(Provider::Deezer, &query, &script).unwrap(),
        ["https://image.test/right.png"]
    );
    assert_eq!(script.calls.borrow().len(), 3);
}

#[test]
fn missing_first_song_still_uses_other_song_and_different_mastering_album_label() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("Unavailable".into());
    query.artist_recordings.push(ArtistRecording {
        title: "Sukoon".into(),
        album: "Sukoon (Dolby Atmos Version)".into(),
        duration_ms: Some(186000),
    });
    let script = Script::new(vec![
        ("api.deezer.com/search?".into(), Ok(json!({"data":[]}))),
        (
            "api.deezer.com/search?".into(),
            Ok(
                json!({"data":[{"title":"Sukoon","duration":186,"album":{"title":"Sukoon"},"artist":{"id":2,"name":"Gini"}}]}),
            ),
        ),
        (
            "artist/2".into(),
            Ok(json!({"id":2,"name":"Gini","picture_xl":"https://image.test/right.png"})),
        ),
    ]);
    assert_eq!(
        providers::candidates(Provider::Deezer, &query, &script).unwrap(),
        ["https://image.test/right.png"]
    );
}

#[test]
fn conflicting_songs_and_verified_artist_without_a_portrait_never_pick_another_name_match() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("First".into());
    query.artist_recordings.push(ArtistRecording {
        title: "Second".into(),
        album: String::new(),
        duration_ms: None,
    });
    let script = Script::new(vec![
        (
            "api.deezer.com/search?".into(),
            Ok(json!({"data":[{"title":"First","artist":{"id":1,"name":"Gini"}}]})),
        ),
        (
            "api.deezer.com/search?".into(),
            Ok(json!({"data":[{"title":"Second","artist":{"id":2,"name":"Gini"}}]})),
        ),
    ]);
    assert_eq!(
        providers::candidates(Provider::Deezer, &query, &script).unwrap_err(),
        LookupError::AmbiguousArtist
    );
    query.artist_recordings.clear();
    let script = Script::new(vec![
        (
            "api.deezer.com/search?".into(),
            Ok(json!({"data":[{"title":"First","artist":{"id":1,"name":"Gini"}}]})),
        ),
        ("artist/1".into(), Ok(json!({"id":1,"name":"Gini"}))),
    ]);
    assert!(providers::candidates(Provider::Deezer, &query, &script)
        .unwrap()
        .is_empty());
    assert_eq!(script.calls.borrow().len(), 2);
}

#[test]
fn recording_requests_are_bounded_and_itunes_ids_are_combined_across_songs() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("First".into());
    for title in ["Second", "Third", "Fourth", "Fifth"] {
        query.artist_recordings.push(ArtistRecording {
            title: title.into(),
            album: String::new(),
            duration_ms: None,
        });
    }
    let track = |title: &str, id: u64| json!({"trackName":title,"artistName":"Gini","artistId":id});
    let script = Script::new(vec![
        (
            "itunes.apple.com".into(),
            Ok(json!({"results":[track("First",1),track("First",2)]})),
        ),
        (
            "itunes.apple.com".into(),
            Ok(json!({"results":[track("Second",2)]})),
        ),
        (
            "itunes.apple.com".into(),
            Ok(json!({"results":[track("Third",2)]})),
        ),
    ]);
    assert!(artist::identify(&query, &script).unwrap().is_some());
    assert_eq!(script.calls.borrow().len(), 3);
}

#[test]
fn musicbrainz_combines_recording_credits_to_resolve_ambiguous_names() {
    let mut query = ArtworkQuery::artist("Gini");
    query.title = Some("First".into());
    query.artist_recordings.push(ArtistRecording {
        title: "Second".into(),
        album: String::new(),
        duration_ms: None,
    });
    let recording = |title: &str, id: &str| json!({"title":title,"artist-credit":[{"artist":{"name":"Gini","id":id}}]});
    let script = Script::new(vec![
        (
            "musicbrainz.org/ws/2/artist?".into(),
            Ok(json!({"artists":[{"name":"Gini","id":"one"},{"name":"Gini","id":"two"}]})),
        ),
        (
            "musicbrainz.org/ws/2/recording?".into(),
            Ok(json!({"recordings":[recording("First","one"),recording("First","two")]})),
        ),
        (
            "musicbrainz.org/ws/2/recording?".into(),
            Ok(json!({"recordings":[recording("Second","two")]})),
        ),
        (
            "musicbrainz.org/ws/2/artist/two?".into(),
            Ok(json!({"relations":[]})),
        ),
    ]);
    assert_eq!(
        providers::candidates(Provider::MusicBrainz, &query, &script).unwrap_err(),
        LookupError::PortraitNotFound
    );
    assert_eq!(script.calls.borrow().len(), 4);
}
#[derive(Clone)]
struct Script {
    pages: Rc<RefCell<VecDeque<PageReply>>>,
    replies: Rc<RefCell<VecDeque<Reply>>>,
    calls: Rc<RefCell<Vec<String>>>,
    bad_image: bool,
    cancelled: Rc<std::cell::Cell<bool>>,
    cancel_after_json: bool,
}
impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        Self {
            pages: Default::default(),
            replies: Rc::new(RefCell::new(replies.into())),
            calls: Default::default(),
            bad_image: false,
            cancelled: Default::default(),
            cancel_after_json: false,
        }
    }
}
impl Transport for Script {
    fn text(&self, url: &str) -> Result<String, LookupError> {
        let Some((expected, result)) = self.pages.borrow_mut().pop_front() else {
            return Err(LookupError::NotFound);
        };
        self.calls.borrow_mut().push(url.into());
        assert!(url.contains(&expected), "expected {expected}, got {url}");
        result
    }
    fn cancelled(&self) -> bool {
        self.cancelled.get()
    }
    fn json(&self, url: &str) -> Result<Value, LookupError> {
        self.calls.borrow_mut().push(url.into());
        let (expected, result) = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected network request");
        assert!(url.contains(&expected), "expected {expected}, got {url}");
        if self.cancel_after_json {
            self.cancelled.set(true);
        }
        result
    }
    fn image(&self, url: &str) -> Result<Vec<u8>, LookupError> {
        self.calls.borrow_mut().push(url.into());
        Ok(if self.bad_image {
            b"not an image".to_vec()
        } else {
            include_bytes!("../../tests/fixtures/cover.png").to_vec()
        })
    }
}
#[test]
fn cancellation_during_search_stops_fallbacks_and_does_not_cache_a_miss() {
    let f = Fixture::new();
    let mut script = Script::new(vec![("itunes".into(), Ok(json!({"results":[]})))]);
    script.cancel_after_json = true;
    assert_eq!(
        lookup(&query(), &f.0, LookupMode::Automatic, |_| script.clone()).unwrap_err(),
        LookupError::Cancelled
    );
    assert_eq!(script.calls.borrow().len(), 1);
    assert!(cache::read(&f.0, &query(), LookupMode::Automatic).is_none());
    let stopped = std::sync::atomic::AtomicBool::new(true);
    assert!(
        fetch_cancellable(&query(), &f.0, LookupMode::Manual, &stopped)
            .unwrap_err()
            .contains("cancelled")
    );
}
fn album_result() -> Value {
    json!({"data":[{"id":1,"artist":{"name":"Artist"},"title":"Album","cover_xl":"https://image.test/cover.png"}]})
}
fn query() -> ArtworkQuery {
    ArtworkQuery::album("Artist", "Album", None)
}

#[test]
fn version_and_album_identity_are_preserved_and_duration_rejects_other_recordings() {
    assert!(names_match("*NSYNC", "NSYNC"));
    assert!(!names_match("Love of my life", "Love of your life"));
    assert!(!album_matches(
        "Morning Glory",
        "Morning Glory (Remastered)"
    ));
    assert!(album_matches(
        "Two New Malcolm Todd Songs",
        "Two New Malcolm Todd Songs - Single"
    ));
    let mut q = ArtworkQuery::album("Artist", "Album", Some("Song"));
    q.duration_ms = Some(120000);
    let mut track = json!({"artistName":"Artist","trackName":"Song","collectionName":"Album","trackTimeMillis":121000});
    assert!(track_matches(&track, &q));
    track["collectionName"] = json!("Greatest Hits");
    assert!(!track_matches(&track, &q));
    track["collectionName"] = json!("Album");
    track["trackTimeMillis"] = json!(180000);
    assert!(!track_matches(&track, &q));
}
#[test]
fn stalled_first_provider_does_not_prevent_deezer_and_success_is_reused() {
    let f = Fixture::new();
    let script = Script::new(vec![
        ("itunes.apple.com".into(), Err(LookupError::TimedOut)),
        ("api.deezer.com".into(), Ok(album_result())),
    ]);
    let stages = RefCell::new(vec![]);
    let first = lookup(&query(), &f.0, LookupMode::Manual, |provider| {
        stages.borrow_mut().push(provider);
        script.clone()
    })
    .unwrap();
    assert_eq!(*stages.borrow(), [Provider::Itunes, Provider::Deezer]);
    assert!(Path::new(&first.full).is_file());
    let again = lookup(&query(), &f.0, LookupMode::Automatic, |_| -> Script {
        panic!("cached success must avoid HTTP")
    })
    .unwrap();
    assert_eq!(again.full, first.full);
    fs::remove_file(&first.full).unwrap();
    let retry = Script::new(vec![
        ("itunes.apple.com".into(), Ok(json!({"results":[]}))),
        ("api.deezer.com".into(), Ok(album_result())),
    ]);
    assert!(lookup(&query(), &f.0, LookupMode::Automatic, |_| retry.clone()).is_ok());
}
#[test]
fn automatic_misses_are_cached_but_manual_fetch_retries_and_network_errors_do_not_poison_cache() {
    let f = Fixture::new();
    let empty = || {
        Script::new(vec![
            ("itunes.apple.com".into(), Ok(json!({"results":[]}))),
            ("api.deezer.com".into(), Ok(json!({"data":[]}))),
            ("musicbrainz.org".into(), Ok(json!({"release-groups":[]}))),
        ])
    };
    let script = empty();
    assert!(matches!(
        lookup(&query(), &f.0, LookupMode::Automatic, |_| script.clone()),
        Err(LookupError::NotFound)
    ));
    assert!(matches!(
        lookup(&query(), &f.0, LookupMode::Automatic, |_| -> Script {
            panic!("automatic miss must avoid HTTP")
        }),
        Err(LookupError::NotFound)
    ));
    let script = empty();
    assert!(matches!(
        lookup(&query(), &f.0, LookupMode::Manual, |_| script.clone()),
        Err(LookupError::NotFound)
    ));
    assert_eq!(script.calls.borrow().len(), 3);
    let other = ArtworkQuery::album("Artist", "Different album", None);
    let offline = Script::new(vec![
        ("itunes".into(), Err(LookupError::Unavailable)),
        ("deezer".into(), Err(LookupError::Unavailable)),
        ("musicbrainz".into(), Err(LookupError::Unavailable)),
    ]);
    assert!(matches!(
        lookup(&other, &f.0, LookupMode::Automatic, |_| offline.clone()),
        Err(LookupError::Unavailable)
    ));
    let retry = empty();
    assert!(matches!(
        lookup(&other, &f.0, LookupMode::Automatic, |_| retry.clone()),
        Err(LookupError::NotFound)
    ));
    assert_eq!(retry.calls.borrow().len(), 3);
}
#[test]
fn invalid_provider_image_falls_through_to_musicbrainz_and_cannot_be_cached_as_success() {
    let f = Fixture::new();
    let mut bad = Script::new(vec![(
        "itunes".into(),
        Ok(
            json!({"results":[{"artistName":"Artist","collectionName":"Album","artworkUrl100":"https://bad.test/image"}]}),
        ),
    )]);
    bad.bad_image = true;
    let fallback = Script::new(vec![
        ("deezer".into(), Ok(json!({"data":[]}))),
        (
            "musicbrainz".into(),
            Ok(
                json!({"release-groups":[{"id":"mbid","title":"Album","artist-credit":[{"artist":{"name":"Artist"}}]}]}),
            ),
        ),
        (
            "coverartarchive".into(),
            Ok(
                json!({"images":[{"front":true,"thumbnails":{"1200":"https://good.test/1200.png"},"image":"https://good.test/full.png"}]}),
            ),
        ),
    ]);
    let paths = lookup(&query(), &f.0, LookupMode::Manual, |provider| {
        if provider == Provider::Itunes {
            bad.clone()
        } else {
            fallback.clone()
        }
    })
    .unwrap();
    assert!(image::load_from_memory(&fs::read(&paths.full).unwrap()).is_ok());
    assert!(fallback
        .calls
        .borrow()
        .iter()
        .any(|url| url.ends_with("1200.png")));
}
#[test]
fn cache_expires_and_different_song_identities_do_not_share_results() {
    let f = Fixture::new();
    cache::save(&f.0, &query(), None);
    let conn = rusqlite::Connection::open(f.0.join("lookup-cache.sqlite3")).unwrap();
    conn.execute("UPDATE lookups SET saved_at=0", []).unwrap();
    assert!(cache::read(&f.0, &query(), LookupMode::Automatic).is_none());
    let mut song = query();
    song.title = Some("Song A".into());
    cache::save(&f.0, &song, None);
    assert!(cache::read(&f.0, &song, LookupMode::Automatic).is_some());
    song.title = Some("Song B".into());
    assert!(cache::read(&f.0, &song, LookupMode::Automatic).is_none());
    conn.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2050) INSERT INTO lookups(key,saved_at) SELECT 'extra-'||x,0 FROM n;").unwrap();
    cache::save(&f.0, &song, None);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM lookups", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2048
    );
}
#[test]
fn ambiguous_artist_names_are_not_selected_arbitrarily() {
    let script = Script::new(vec![(
        "deezer.com".into(),
        Ok(
            json!({"data":[{"id":1,"name":"Artist","picture_xl":"https://one"},{"id":2,"name":"Artist","picture_xl":"https://two"}]}),
        ),
    )]);
    assert_eq!(
        providers::candidates(Provider::Deezer, &ArtworkQuery::artist("Artist"), &script)
            .unwrap_err(),
        LookupError::AmbiguousArtist
    );
}
#[test]
fn expired_http_budget_stops_without_contacting_the_network() {
    let client = Http::new(Duration::ZERO);
    assert_eq!(
        client.json("https://itunes.apple.com/search").unwrap_err(),
        LookupError::TimedOut
    );
}

#[test]
fn ambiguous_artist_lookup_explains_the_failure_and_automatic_retries_are_cached() {
    let f = Fixture::new();
    let query = ArtworkQuery::artist("Gini");
    let script = Script::new(vec![
        (
            "deezer.com".into(),
            Ok(
                json!({"data":[{"id":1,"name":"Gini","picture_xl":"https://one"},{"id":2,"name":"GINI","picture_xl":"https://two"}]}),
            ),
        ),
        ("musicbrainz.org".into(), Ok(json!({"artists":[]}))),
    ]);
    assert_eq!(
        lookup(&query, &f.0, LookupMode::Manual, |_| script.clone()).unwrap_err(),
        LookupError::AmbiguousArtist
    );
    assert_eq!(
        script.calls.borrow().len(),
        2,
        "ambiguous pictures must never be downloaded"
    );
    assert_eq!(
        lookup(&query, &f.0, LookupMode::Automatic, |_| script.clone()).unwrap_err(),
        LookupError::NotFound
    );
    assert_eq!(
        script.calls.borrow().len(),
        2,
        "automatic searches must not repeatedly retry an ambiguous name"
    );
    let retry = Script::new(vec![(
        "deezer.com".into(),
        Ok(json!({"data":[{"id":1,"name":"Gini","picture_xl":"https://verified"}]})),
    )]);
    assert!(
        lookup(&query, &f.0, LookupMode::Manual, |_| retry.clone()).is_ok(),
        "manual fetching must bypass the cached miss"
    );
}
#[test]
#[ignore = "contacts live artwork providers; uses disposable cache and no user files"]
fn live_artwork_sources_return_decodable_album_and_artist_images() {
    let f = Fixture::new();
    for query in [
        ArtworkQuery::album(
            "Oasis",
            "(What's The Story) Morning Glory?",
            Some("Don't Look Back in Anger"),
        ),
        ArtworkQuery::album(
            "Malcolm Todd",
            "Two New Malcolm Todd Songs",
            Some("You Owe Me"),
        ),
        ArtworkQuery::artist("Shawn Mendes"),
    ] {
        let paths =
            fetch(&query, &f.0, LookupMode::Manual).unwrap_or_else(|e| panic!("{query:?}: {e}"));
        let (w, h) = image::image_dimensions(paths.full).unwrap();
        assert!(w >= 256 && h >= 256);
    }
}
