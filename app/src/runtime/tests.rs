//! Exercise real UI completion handlers with an isolated service event stream.
use super::*;
use crate::protocol::{Event, MetadataAssetKind, OperationError, OperationRequest};
use orca_services::types::Track;
use slint::{Model, VecModel};
use std::sync::atomic::Ordering;

pub(crate) fn verify_completions(ui: &OrcaWindow) {
    let (service, events, requests) = AppWorker::test_channels();
    let service = Rc::new(service);
    let cache = Rc::new(RefCell::new(crate::artwork::ArtworkCache::new(
        4 * 1024 * 1024,
    )));
    let catalog = Rc::new(models::CatalogModels::new(
        service.tx.clone(),
        cache.clone(),
    ));
    catalog.grid.columns.set(1);
    catalog.grid.edge.set(64);
    for group in [&catalog.groups, &catalog.related] {
        group.columns.set(1);
        group.edge.set(64);
    }
    let models = RuntimeModels {
        worker: service,
        cache: cache.clone(),
        catalog,
        player: Rc::new(crate::playback::state::PlayerModels::new()),
    };

    for count in 1..=200 {
        events
            .send(Event::Statistics(orca_services::types::Statistics {
                songs: count,
                ..Default::default()
            }))
            .unwrap();
    }
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    let first = ui
        .library()
        .get_summary()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!(
        (1..=64).contains(&first),
        "a UI frame must not drain an unlimited event burst"
    );
    for _ in 0..200 {
        event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    }
    assert_eq!(
        ui.library().get_summary(),
        "200 songs",
        "deferred events must be delivered in order without loss"
    );

    ui.player().set_output_device_id("saved-endpoint".into());
    events
        .send(Event::OutputDevices(Ok(vec![
            orca_services::types::OutputDevice {
                id: "another-endpoint".into(),
                name: "Speakers".into(),
                is_default: true,
            },
        ])))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(ui.player().get_output_choices().row_count(), 3);
    assert_eq!(ui.player().get_output_device_index(), 2);
    assert_eq!(ui.player().get_output_device_id(), "saved-endpoint");
    events
        .send(Event::OutputDevices(Ok(vec![
            orca_services::types::OutputDevice {
                id: "saved-endpoint".into(),
                name: "Headphones".into(),
                is_default: false,
            },
        ])))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(ui.player().get_output_device_index(), 1);
    ui.player().set_output_device_id("".into());
    let mut output = orca_services::types::OutputStatus {
        device_name: "Test speakers".into(),
        exclusive: true,
        sample_rate: 48000,
        source_sample_rate: 192000,
        channels: 2,
        sample_format: "24-bit Int".into(),
        notice: "Unsupported source rate; using resampling.".into(),
        ..Default::default()
    };
    events
        .send(Event::Playback(orca_services::types::Snapshot {
            output: output.clone(),
            ..Default::default()
        }))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(ui.player().get_output_source_rate(), 192000);
    assert_eq!(ui.player().get_output_rate(), 48000);
    assert!(ui.player().get_output_rate_path().contains("192 kHz"));
    assert!(!ui.player().get_output_notice().is_empty());
    output.sample_rate = 192000;
    output.notice.clear();
    events
        .send(Event::Playback(orca_services::types::Snapshot {
            output,
            ..Default::default()
        }))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(ui.player().get_output_rate_path(), "Native sample rate");
    assert!(ui.player().get_output_notice().is_empty());

    events
        .send(Event::Error("output:device busy".into()))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(!ui.shell().get_error().is_empty());
    let recovered = || orca_services::types::Snapshot {
        path: "recovered.flac".into(),
        playing: true,
        output: orca_services::types::OutputStatus {
            sample_rate: 48000,
            source_sample_rate: 44100,
            channels: 2,
            sample_format: "24-bit Int".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    events.send(Event::Playback(recovered())).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(
        ui.shell().get_error().is_empty(),
        "healthy playback must clear its old output banner"
    );
    assert!(ui.player().get_output_format_label().contains("48 kHz"));
    assert!(ui.player().get_output_rate_path().contains("44.1 kHz"));
    ui.shell().set_windows_audio(true);
    ui.player().set_output_unprocessed(true);
    assert_eq!(
        ui.player().get_output_requested_mode_label(),
        "Unprocessed exclusive"
    );
    assert_eq!(ui.player().get_output_mode_label(), "WASAPI shared");
    assert!(ui.player().get_output_open());
    let mut failed = recovered();
    failed.output.sample_rate = 0;
    failed.output.channels = 0;
    failed.output.error = "unsupported-source-format: private driver detail".into();
    events.send(Event::Playback(failed)).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(!ui.player().get_output_open());
    assert_eq!(ui.player().get_output_mode_label(), "No active output");
    assert_eq!(ui.player().get_processing_summary(), "No active processing");
    assert!(ui
        .player()
        .get_output_error()
        .starts_with("Device doesn't support"));
    assert!(!ui.player().get_output_error().contains("private"));
    let mut switching = recovered();
    switching.output.switching = true;
    events.send(Event::Playback(switching)).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(!ui.player().get_output_open());
    assert_eq!(ui.player().get_output_mode_label(), "Switching output...");
    assert!(!ui.player().get_audio_tooltip().contains("48 kHz"));
    ui.player().set_output_unprocessed(false);
    events.send(Event::Playback(recovered())).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    events
        .send(Event::Error("permission denied".into()))
        .unwrap();
    events.send(Event::Playback(recovered())).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(
        !ui.shell().get_error().is_empty(),
        "audio recovery must preserve unrelated errors"
    );
    ui.shell().set_error("".into());
    ui.player().set_playing(false);
    let browses = Rc::new(Cell::new(0));
    let counter = browses.clone();
    ui.library()
        .on_browse(move || counter.set(counter.get() + 1));
    ui.shell().set_view("songs".into());
    events
        .send(Event::PlaylistChanged(
            crate::protocol::PlaylistChange::Membership,
        ))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(
        browses.get(),
        0,
        "liking a song must not reload the library"
    );
    ui.shell().set_view("playlists".into());
    events
        .send(Event::PlaylistChanged(
            crate::protocol::PlaylistChange::Membership,
        ))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(browses.get(), 1, "liked playlists must still update");
    ui.shell().set_view("songs".into());
    // Test completion ownership, without timing a large asset decode in debug.
    models.worker.latest_query.store(2, Ordering::Relaxed);
    ui.library().set_catalog_loading(true);
    events.send(Event::Page(1, 0, 0, vec![])).unwrap();
    events
        .send(Event::CatalogError(1, "Stale search".into()))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(
        ui.library().get_catalog_loading(),
        "stale searches must not finish the current search"
    );
    events.send(Event::Page(2, 0, 0, vec![])).unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(
        !ui.library().get_catalog_loading(),
        "empty current results must finish the search"
    );
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
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(
        cache.borrow_mut().cached_cover(&[&path]).size(),
        before,
        "automatic completion must retain decoded covers"
    );
    assert_eq!(
        browses.get(),
        1,
        "artwork completion must not restart the catalog query"
    );
    assert!(matches!(
        requests.try_recv().unwrap(),
        crate::protocol::Request::ArtworkRefresh(2, _)
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
    *models.player.now.borrow_mut() = playing.clone();
    let loaded = crate::lyrics::parse("[00:01.00]Keep these lyrics");
    *models.player.lines.borrow_mut() = loaded.clone();
    ui.lyrics()
        .set_lyrics(Rc::new(VecModel::from(loaded)).into());
    ui.lyrics().set_active_line(0);
    ui.lyrics().set_lyrics_status("Loaded".into());
    ui.preferences().set_romanization_mode(1);
    models
        .worker
        .romanization_generation
        .store(7, Ordering::Relaxed);
    let revision = models.player.lyric_renderer.borrow().layout_revision();
    events
        .send(Event::Romanized(6, vec!["Stale pronunciation".into()]))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(
        models.player.lyric_renderer.borrow().layout_revision(),
        revision
    );
    events
        .send(Event::Romanized(7, vec!["Current pronunciation".into()]))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(models.player.lyric_renderer.borrow().layout_revision() > revision);
    assert_eq!(models.player.lines.borrow()[0].text, "Keep these lyrics");
    assert_eq!(
        ui.lyrics().get_lyrics().row_data(0).unwrap().text,
        "Keep these lyrics"
    );
    ui.preferences().set_romanization_mode(0);
    let revision = models.player.lyric_renderer.borrow().layout_revision();
    events
        .send(Event::Romanized(7, vec!["Disabled pronunciation".into()]))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(
        models.player.lyric_renderer.borrow().layout_revision(),
        revision
    );
    models
        .player
        .lyric_renderer
        .borrow_mut()
        .set_romanization(Vec::new());

    let mut refreshed = playing;
    refreshed.album = "New album".into();
    events.send(Event::Now(refreshed.clone())).unwrap();
    assert!(event_dispatch::drain_events(
        ui,
        &models,
        &None,
        &Cell::new(0),
        &Cell::new(0)
    ));
    assert_eq!(models.player.now.borrow().album, "New album");
    assert_eq!(models.player.lines.borrow().len(), 1);
    assert_eq!(ui.lyrics().get_lyrics().row_count(), 1);
    assert_eq!(ui.lyrics().get_active_line(), 0);
    assert_eq!(ui.lyrics().get_lyrics_status(), "Loaded");
    assert_eq!(cache.borrow_mut().cached_cover(&[&path]).size(), before);
    events.send(Event::Now(refreshed)).unwrap();
    assert!(
        !event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0)),
        "unchanged track metadata must not request another image update"
    );
    events
        .send(Event::Now(Track {
            path: "next.flac".into(),
            ..Default::default()
        }))
        .unwrap();
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert!(models.player.lines.borrow().is_empty());
    assert_eq!(ui.lyrics().get_lyrics().row_count(), 0);

    let draft=crate::metadata::parse(r#"{"path":"same.flac","title":"Current","artist":"Artist","album":"Album","album_artist":"Artist","lyrics":"Current lyrics"}"#).unwrap();
    ui.dialogs().set_draft(draft);
    ui.dialogs().set_editor_generation(2);
    ui.dialogs().set_metadata_open(true);
    ui.dialogs().set_editor_busy(true);
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
    event_dispatch::drain_events(ui, &models, &None, &Cell::new(0), &Cell::new(0));
    assert_eq!(ui.dialogs().get_draft().lyrics, "Current lyrics");
    assert!(
        ui.dialogs().get_metadata_open() && ui.dialogs().get_editor_busy(),
        "stale and unrelated completions must preserve the current editor"
    );
    assert_eq!(cache.borrow_mut().cached_cover(&[&path]).size(), before);
    ui.dialogs().set_metadata_open(false);
    ui.dialogs().set_editor_busy(false);
    ui.shell().set_error("".into());
    // One runtime batch publishes ready cover pixels without rebuilding the catalog.
    *models.player.now.borrow_mut() = Track {
        path: "completion.flac".into(),
        artwork: path.clone(),
        artwork_thumb: path.clone(),
        ..Default::default()
    };
    ui.preferences().set_compact_artwork(true);
    ui.player().set_full_player(false);
    ui.shell().set_window_hidden(false);
    cache.borrow_mut().clear();
    artwork_updates::refresh_artwork(ui, &models, true);
    let deadline = Instant::now() + Duration::from_secs(3);
    while ui.player().get_now().cover.size().width == 0 {
        assert!(
            Instant::now() < deadline,
            "the runtime batch must publish the ready cover"
        );
        std::thread::sleep(Duration::from_millis(5));
        artwork_updates::refresh_artwork(ui, &models, false);
    }
    let previous_view = ui.shell().get_view();
    let previous_detail = ui.library().get_detail_key();
    let previous_artwork = ui.preferences().get_browse_artwork();
    let previous_background = ui.preferences().get_blurred_background();
    ui.shell().set_view("albums".into());
    ui.library().set_detail_key("Fixture album".into());
    ui.library().set_detail_backdrop(slint::Image::default());
    ui.preferences().set_browse_artwork(true);
    ui.preferences().set_blurred_background(false);
    *models.catalog.detail_art.borrow_mut() = (path.clone(), path.clone());
    artwork_updates::refresh_artwork(ui, &models, true);
    let deadline = Instant::now() + Duration::from_secs(3);
    while ui.library().get_detail_backdrop().size().width == 0 {
        assert!(
            Instant::now() < deadline,
            "detail headers must retain artwork blur without library backgrounds"
        );
        std::thread::sleep(Duration::from_millis(5));
        artwork_updates::refresh_artwork(ui, &models, false);
    }
    assert_eq!(ui.player().get_backdrop().size().width, 0);
    ui.preferences().set_browse_artwork(false);
    artwork_updates::refresh_artwork(ui, &models, true);
    assert_eq!(ui.library().get_detail_backdrop().size().width, 0);
    ui.shell().set_view(previous_view);
    ui.library().set_detail_key(previous_detail);
    ui.preferences().set_browse_artwork(previous_artwork);
    ui.preferences().set_blurred_background(previous_background);
    std::fs::remove_file(fixture).unwrap();
    let runtime = start(ui, None, models);
    ui.invoke_artwork_ready();
    assert!(runtime.artwork_timer.running());
    runtime.stop();
    ui.invoke_artwork_ready();
    assert!(!runtime.update_timer.running());
    assert!(
        !runtime.artwork_timer.running(),
        "late artwork notifications must not restart a stopped runtime"
    );
}
