//! Exercise real UI completion handlers with an isolated service event stream.
use super::*;
use crate::protocol::{Event, MetadataAssetKind, OperationError, OperationRequest};
use slint::ModelNotify;

pub(crate) fn verify_completions(ui: &OrcaWindow) {
    let (service, events, requests) = Service::test_channels();
    let service = Rc::new(service);
    let cache = Rc::new(RefCell::new(artwork::ArtworkCache::new(4 * 1024 * 1024)));
    let tracks = Rc::new(models::Tracks::new(service.tx.clone(), cache.clone()));
    let groups = || {
        Rc::new(models::GroupGrid {
            landscape: Cell::new(false),
            visible_rows: RefCell::new(Default::default()),
            artwork: Cell::new(true),
            circular: Cell::new(false),
            groups: RefCell::new(vec![]),
            notify: ModelNotify::default(),
            columns: Cell::new(1),
            edge: Cell::new(64),
            cache: cache.clone(),
        })
    };
    let models = RuntimeModels {
        service,
        cache: cache.clone(),
        lyric_renderer: Rc::new(RefCell::new(lyric_render::Renderer::new())),
        playback_clock: Rc::new(RefCell::new(playback_clock::PlaybackClock::new())),
        list: Rc::new(models::SongList {
            store: tracks.clone(),
            notify: ModelNotify::default(),
        }),
        grid: Rc::new(models::SongGrid {
            visible_rows: RefCell::new(Default::default()),
            store: tracks.clone(),
            notify: ModelNotify::default(),
            columns: Cell::new(1),
            edge: Cell::new(64),
        }),
        tracks,
        groups: groups(),
        related: groups(),
        neighbors: Rc::new(RefCell::new(vec![])),
        detail_art: Rc::new(RefCell::new(Default::default())),
        queue: Rc::new(VecModel::from(vec![])),
        queue_tracks: Rc::new(RefCell::new(vec![])),
        now: Rc::new(RefCell::new(Track::default())),
        lines: Rc::new(RefCell::new(vec![])),
        preview: Rc::new(RefCell::new(None)),
    };
    let state = ui.global::<AppState>();
    let browses = Rc::new(Cell::new(0));
    let counter = browses.clone();
    state.on_browse(move || counter.set(counter.get() + 1));
    // Test completion ownership, without timing a large asset decode in debug.
    let fixture = std::env::temp_dir().join(format!(
        "orca-runtime-completion-{}.png",
        std::process::id()
    ));
    image::RgbaImage::from_pixel(32, 32, image::Rgba([50, 100, 150, 255]))
        .save(&fixture)
        .unwrap();
    let path = fixture.to_string_lossy().into_owned();
    cache.borrow_mut().get(&path, 64, false);
    let deadline = Instant::now() + Duration::from_secs(4);
    while cache.borrow_mut().cached_cover(&[&path]).size().width == 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
        cache.borrow_mut().drain();
    }
    let before = cache.borrow_mut().cached_cover(&[&path]).size();
    events.send(Event::AutomaticArtworkUpdated).unwrap();
    events::apply(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(
        cache.borrow_mut().cached_cover(&[&path]).size(),
        before,
        "automatic completion must retain decoded covers"
    );
    assert_eq!(browses.get(), 1);
    assert!(matches!(
        requests.try_recv().unwrap(),
        crate::protocol::Request::Reconcile
    ));

    // Collection saves and artwork reconciliation update the same song. They
    // must not clear its loaded lyrics or invalidate unchanged decoded artwork.
    let playing = Track {
        path: "playing.flac".into(),
        title: "Song".into(),
        album: "Old album".into(),
        artwork: path.clone(),
        ..Default::default()
    };
    *models.now.borrow_mut() = playing.clone();
    let loaded = crate::lyrics::parse("[00:01.00]Keep these lyrics");
    *models.lines.borrow_mut() = loaded.clone();
    state.set_lyrics(Rc::new(VecModel::from(loaded)).into());
    state.set_active_line(0);
    state.set_lyrics_status("Loaded".into());
    let mut refreshed = playing;
    refreshed.album = "New album".into();
    events.send(Event::Now(refreshed.clone())).unwrap();
    assert!(events::apply(
        ui,
        &models,
        &None,
        &Cell::new(0),
        &Cell::new(0)
    ));
    assert_eq!(models.now.borrow().album, "New album");
    assert_eq!(models.lines.borrow().len(), 1);
    assert_eq!(state.get_lyrics().row_count(), 1);
    assert_eq!(state.get_active_line(), 0);
    assert_eq!(state.get_lyrics_status(), "Loaded");
    assert_eq!(cache.borrow_mut().cached_cover(&[&path]).size(), before);
    events.send(Event::Now(refreshed)).unwrap();
    assert!(
        !events::apply(ui, &models, &None, &Cell::new(0), &Cell::new(0)),
        "unchanged track metadata must not request another image update"
    );
    events
        .send(Event::Now(Track {
            path: "next.flac".into(),
            ..Default::default()
        }))
        .unwrap();
    events::apply(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(models.lines.borrow().is_empty());
    assert_eq!(state.get_lyrics().row_count(), 0);

    let draft=crate::metadata::parse(r#"{"path":"same.flac","title":"Current","artist":"Artist","album":"Album","album_artist":"Artist","lyrics":"Current lyrics"}"#).unwrap();
    state.set_draft(draft);
    state.set_editor_generation(2);
    state.set_metadata_open(true);
    state.set_editor_busy(true);
    events
        .send(Event::MetadataAsset(
            MetadataAssetKind::Lyrics,
            "same.flac".into(),
            "Old lyrics".into(),
            1,
        ))
        .unwrap();
    events
        .send(Event::MetadataSaved("same.flac".into(), 1))
        .unwrap();
    events
        .send(Event::MetadataFailed(1, "Old failure".into()))
        .unwrap();
    events
        .send(Event::Error("Unrelated playback failure".into()))
        .unwrap();
    let old=OperationRequest::from_value(serde_json::json!({"action":"fetch-lyrics","key":"same.flac","editor_path":"same.flac","editor_generation":1})).unwrap();
    events
        .send(Event::OperationFailed(
            old,
            OperationError::Failed("Old fetch failure".into()),
        ))
        .unwrap();
    events::apply(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(state.get_draft().lyrics, "Current lyrics");
    assert!(
        state.get_metadata_open() && state.get_editor_busy(),
        "stale and unrelated completions must preserve the current editor"
    );
    assert_eq!(cache.borrow_mut().cached_cover(&[&path]).size(), before);
    state.set_metadata_open(false);
    state.set_editor_busy(false);
    state.set_error("".into());
    std::fs::remove_file(fixture).unwrap();
}
