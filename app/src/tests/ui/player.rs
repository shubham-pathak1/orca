use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let pixels = &mut case.pixels;
    // Hovering or dragging the popup must keep it open past its close delay.
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    window.set_size(slint::PhysicalSize::new(1280, 820));
    ui.preferences().set_blurred_background(false);
    let previous_song = ui.player().get_now();
    ui.player().set_full_player(false);
    ui.lyrics().set_lyrics_open(false);
    ui.player().set_queue_open(false);
    ui.shell().set_view("songs".into());
    ui.player().set_now(Song {
        id: 42,
        path: "playing.flac".into(),
        title: "Playing song".into(),
        quality: "FLAC | 192 KHZ | 6319 KBPS".into(),
        ..Default::default()
    });
    let old_quality = ui.preferences().get_quality_info();
    let mut cover_song = ui.player().get_now();
    cover_song.cover = slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
        &[234u8, 10, 200, 255].repeat(64 * 64),
        64,
        64,
    ));
    cover_song.cover_missing = false;
    ui.player().set_now(cover_song);
    for width in [1280u32, 560] {
        window.set_size(slint::PhysicalSize::new(width, 820));
        let mut bounds = Vec::new();
        for shown in [true, false] {
            ui.preferences().set_quality_info(shown);
            window.draw_if_needed(|renderer| {
                renderer.render(pixels, 1280);
            });
            let points: Vec<_> = (650..820)
                .flat_map(|y| (56..200).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let p = pixels[y * 1280 + x];
                    p.r > 200 && p.g < 30 && p.b > 170
                })
                .collect();
            assert!(!points.is_empty());
            bounds.push((
                points.iter().map(|p| p.0).min(),
                points.iter().map(|p| p.0).max(),
                points.iter().map(|p| p.1).min(),
                points.iter().map(|p| p.1).max(),
            ));
        }
        assert_eq!(
            bounds[0], bounds[1],
            "quality toggle must preserve player artwork at width {width}"
        );
    }
    ui.preferences().set_quality_info(old_quality);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let (swipe_tx, swipe_rx) = std::sync::mpsc::channel();
    controllers::input::install_player_swipe(ui, swipe_tx.clone());
    let (playlist_tx, playlist_rx) = std::sync::mpsc::channel();
    controllers::playlists::install(ui, playlist_tx);
    ui.shell().set_view("playlists".into());
    ui.library().set_detail_key("7".into());
    ui.library().set_detail_title("Test playlist".into());
    let previous_songs = ui.library().get_songs();
    ui.library().set_songs(
        Rc::new(VecModel::from(vec![
            Song {
                id: 1,
                path: "one.wav".into(),
                title: "One".into(),
                ..Default::default()
            },
            Song {
                id: 2,
                path: "two.wav".into(),
                title: "Two".into(),
                ..Default::default()
            },
        ]))
        .into(),
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 878.0, 323.0);
    assert!(ui.library().get_playlist_selection_mode());
    click(window, 300.0, 362.0);
    click(window, 300.0, 405.0);
    assert_eq!(ui.library().get_playlist_selection_count(), 2);
    assert!(ui
        .library()
        .invoke_playlist_song_selected(1, ui.library().get_playlist_selection_revision()));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "playlist-multiselect");
    ui.library().invoke_playlist_selection("remove".into(), 0);
    assert!(!ui.library().get_playlist_selection_mode());
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::Playlist(crate::protocol::PlaylistEdit::RemoveMany { id: Some(7), song_ids })
            if song_ids.len() == 2 && song_ids.contains(&1) && song_ids.contains(&2)
    ));
    ui.library().invoke_playlist_selection("start".into(), 0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(300.0, 362.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(300.0, 407.0),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(300.0, 407.0),
        button: PointerEventButton::Left,
    });
    assert_eq!(ui.library().get_playlist_selection_count(), 2);
    ui.library().invoke_playlist_selection("cancel".into(), 0);
    ui.library().set_detail_key("liked".into());
    ui.library().set_detail_title("Liked Songs".into());
    ui.library().set_detail_cover_missing(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "liked-songs-dark");
    ui.preferences().set_light_theme(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "liked-songs-light");
    ui.preferences().set_light_theme(false);
    ui.library().invoke_playlist_selection("start".into(), 0);
    ui.library().invoke_playlist_selection("toggle".into(), 42);
    ui.library().invoke_playlist_selection("remove".into(), 0);
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::Playlist(crate::protocol::PlaylistEdit::RemoveMany { id: None, song_ids })
            if song_ids == vec![42]
    ));
    ui.library().set_detail_key("".into());
    ui.shell().set_view("songs".into());
    ui.library().set_songs(previous_songs);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "player-controls-smoke");
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(110.0, 775.0),
        delta_x: 20.0,
        delta_y: -40.0,
    });
    assert!(swipe_rx.try_recv().is_err());
    for _ in 0..3 {
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(110.0, 775.0),
            delta_x: -55.0,
            delta_y: 0.0,
        });
    }
    assert!(matches!(
        swipe_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Next)
    ));
    assert!(swipe_rx.try_recv().is_err());
    click(window, 1104.0, 772.0);
    assert!(ui.dialogs().get_playlist_picker_open());
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::PlaylistChoices(42)
    ));
    ui.dialogs().set_playlist_choices(
        Rc::new(VecModel::from(vec![
            PlaylistChoice {
                key: "7".into(),
                title: "Favorites".into(),
                selected: false,
            },
            PlaylistChoice {
                key: "8".into(),
                title: "Road trip".into(),
                selected: true,
            },
        ]))
        .into(),
    );
    ui.dialogs().set_playlist_picker_loading(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "playlist-picker-toggle");
    let picker_x = (ui.dialogs().get_playlist_picker_x() - 130.0).clamp(16.0, 964.0);
    let picker_y = (ui.dialogs().get_playlist_picker_y() - 208.0).clamp(16.0, 608.0);
    click(window, picker_x + 40.0, picker_y + 82.0);
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::Playlist(crate::protocol::PlaylistEdit::Add { id: 7, song_id: 42 })
    ));
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::PlaylistChoices(42)
    ));
    assert!(ui.dialogs().get_playlist_picker_open());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, picker_x + 40.0, picker_y + 126.0);
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::Playlist(crate::protocol::PlaylistEdit::Remove { id: 8, song_id: 42 })
    ));
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::PlaylistChoices(42)
    ));
    click(window, 800.0, 400.0);
    assert!(!ui.dialogs().get_playlist_picker_open());
    click(window, 1064.0, 772.0);
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::ToggleLiked(42)
    ));
    ui.player().set_liked(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "player-liked-song");
    click(window, 1152.0, 772.0);
    assert!(ui.lyrics().get_lyrics_open());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "inline-lyrics-docked");
    assert!(ui.lyrics().get_lyrics_width() > 200.0);
    click(window, 1202.0, 772.0);
    assert!(ui.player().get_queue_open());
    assert!(ui.lyrics().get_lyrics_open());
    click(window, 1236.0, 38.0);
    assert!(!ui.player().get_queue_open());
    assert!(ui.lyrics().get_lyrics_open());
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1244.0, 764.0),
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1244.0, 666.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "volume-over-lyrics");
    click(window, 1232.0, 28.0);
    assert!(!ui.lyrics().get_lyrics_open());
    window.set_size(slint::PhysicalSize::new(560, 820));
    let mut narrow_pixels = vec![slint::Rgb8Pixel::default(); 560 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut narrow_pixels, 560);
    });
    let bytes: Vec<u8> = narrow_pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/player-controls-narrow.png"),
        &bytes,
        560,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    click(window, 420.0, 698.0);
    assert!(ui.dialogs().get_playlist_picker_open());
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::PlaylistChoices(42)
    ));
    click(window, 100.0, 500.0);
    assert!(!ui.dialogs().get_playlist_picker_open());
    click(window, 459.0, 698.0);
    assert!(ui.lyrics().get_lyrics_open());
    click(window, 512.0, 28.0);
    assert!(!ui.lyrics().get_lyrics_open());
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let mut playing = ui.player().get_now();
    playing.artist = "Artist".into();
    playing.album = "Album".into();
    playing.quality = "FLAC | 44.1 KHZ | 965 KBPS".into();
    ui.player().set_now(playing);
    ui.player().set_full_player(true);
    controllers::input::install_player_swipe(ui, swipe_tx);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    for _ in 0..3 {
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(640.0, 300.0),
            delta_x: 55.0,
            delta_y: 0.0,
        });
    }
    assert!(matches!(
        swipe_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Previous)
    ));
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(640.0, 300.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(570.0, 304.0),
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(500.0, 307.0),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(500.0, 307.0),
        button: PointerEventButton::Left,
    });
    assert!(matches!(
        swipe_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Next)
    ));
    assert!(swipe_rx.try_recv().is_err(), "one drag must skip only once");
    assert!(!ui.player().invoke_player_drag(20.0, 0.0));
    assert!(!ui.player().invoke_player_drag(110.0, 130.0));
    assert!(
        swipe_rx.try_recv().is_err(),
        "short and vertical drags must not skip"
    );
    screenshot(pixels, "full-player-like-and-playlist");
    ui.shell().set_windows_audio(true);
    ui.player().set_output_rate(44100);
    ui.player().set_output_source_rate(44100);
    ui.player().set_output_channels(2);
    ui.player().set_output_active_exclusive(true);
    ui.player().set_output_active_unprocessed(true);
    ui.player().set_output_unprocessed(true);
    ui.player().set_output_device_name("Test speakers".into());
    ui.player().set_output_sample_format("24-bit Int".into());
    ui.player()
        .set_output_notice("Use your device's volume control.".into());
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(640.0, 556.0),
    });
    std::thread::sleep(Duration::from_millis(850));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-output-tooltip");
    ui.dialogs().set_audio_info_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "audio-path-opened");
    ui.player().set_output_rate(0);
    ui.player()
        .set_output_error(crate::errors::friendly("unsupported-source-format: test").into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "audio-path-unsupported");
    ui.dialogs().set_audio_info_open(false);
    ui.player().set_output_error("".into());
    ui.player().set_output_notice("".into());
    ui.player().set_output_unprocessed(false);
    ui.player().set_output_active_unprocessed(false);
    ui.player().set_output_active_exclusive(false);
    ui.player().set_output_rate(44100);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(20.0, 400.0),
    });
    slint::platform::update_timers_and_animations();
    ui.lyrics().set_lyrics_open(true);
    click(window, 1212.0, 72.0);
    assert!(ui.player().get_queue_open());
    assert!(ui.lyrics().get_lyrics_open());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-queue-divider");
    let commands = Rc::new(RefCell::new(Vec::new()));
    let observed_commands = commands.clone();
    ui.player()
        .on_command(move |action, _| observed_commands.borrow_mut().push(action.to_string()));
    assert!(ui.shell().invoke_layer_enabled("full".into()));
    for x in [307.0, 367.0, 435.0, 503.0, 563.0] {
        click(window, x, 712.0);
        assert!(
            ui.player().get_queue_open(),
            "playback controls must keep the queue open"
        );
    }
    assert_eq!(
        *commands.borrow(),
        ["shuffle", "previous", "toggle", "next", "repeat"]
    );
    let old_duration = ui.player().get_duration();
    ui.player().set_duration(100000.0);
    let seek_target = Rc::new(Cell::new(-1.0));
    let observed_seek = seek_target.clone();
    ui.player().on_seek(move |value| observed_seek.set(value));
    click(window, 400.0, 620.0);
    assert!(seek_target.get() >= 0.0);
    assert!(
        ui.player().get_queue_open(),
        "seeking must keep the queue open"
    );
    ui.player().set_duration(old_duration);
    click(window, 614.0, 712.0);
    assert!(ui.dialogs().get_playlist_picker_open());
    assert!(
        ui.player().get_queue_open(),
        "playlist picker must keep the queue open"
    );
    ui.dialogs().set_playlist_picker_open(false);
    while playlist_rx.try_recv().is_ok() {}

    for width in [1280u32, 960] {
        window.set_size(slint::PhysicalSize::new(width, 820));
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let start = width as f32 * 0.4;
        window.dispatch_event(WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(start, 400.0),
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(start - 120.0, 406.0),
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(start - 120.0, 406.0),
            button: PointerEventButton::Left,
        });
        assert!(
            matches!(
                swipe_rx.try_recv().unwrap(),
                Request::Command(crate::protocol::PlayerAction::Next)
            ),
            "full-player swipe must work with the queue open at width {width}"
        );
        assert!(
            ui.player().get_queue_open(),
            "swiping must keep the queue open"
        );
        assert!(swipe_rx.try_recv().is_err(), "one swipe must skip once");
    }
    window.set_size(slint::PhysicalSize::new(1280, 820));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(600.0, 400.0),
        delta_x: -120.0,
        delta_y: 0.0,
    });
    assert!(
        matches!(
            swipe_rx.try_recv().unwrap(),
            Request::Command(crate::protocol::PlayerAction::Next)
        ),
        "horizontal trackpad scrolling must also work with the queue open"
    );
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(600.0, 400.0),
        delta_x: 0.0,
        delta_y: -120.0,
    });
    assert!(
        swipe_rx.try_recv().is_err(),
        "vertical scrolling must not skip songs"
    );
    assert!(ui.player().get_queue_open());
    click(window, 600.0, 400.0);
    assert!(!ui.player().get_queue_open());
    assert!(ui.lyrics().get_lyrics_open());
    let lyric_refreshes = Rc::new(Cell::new(0));
    let observed_refreshes = lyric_refreshes.clone();
    ui.lyrics()
        .on_refresh_lyrics(move || observed_refreshes.set(observed_refreshes.get() + 1));
    ui.lyrics().set_lyrics(Default::default());
    ui.lyrics().set_lyrics_status("Lyrics not found.".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-lyrics-retry");
    click(window, 880.0, 474.0);
    assert_eq!(lyric_refreshes.get(), 1);
    assert!(ui.lyrics().get_lyrics_refreshing());
    click(window, 880.0, 474.0);
    assert_eq!(
        lyric_refreshes.get(),
        1,
        "retry button disappears while the lookup is pending"
    );
    ui.lyrics().set_lyrics_refreshing(false);
    screenshot(pixels, "full-player-lyrics-controls");
    ui.lyrics().set_lyrics(
        Rc::new(VecModel::from(crate::lyrics::parse(
            "[00:01.00]First line\n[00:03.00]Second line",
        )))
        .into(),
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(1000.0, 400.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(880.0, 406.0),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(880.0, 406.0),
        button: PointerEventButton::Left,
    });
    assert!(matches!(
        swipe_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Next)
    ));
    ui.lyrics().set_lyrics(Default::default());
    window.set_size(slint::PhysicalSize::new(960, 820));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "compact-full-player-lyrics");
    window.set_size(slint::PhysicalSize::new(1280, 820));
    ui.lyrics().set_lyrics_open(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 474.0, 712.0);
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::ToggleLiked(42)
    ));
    click(window, 806.0, 712.0);
    assert!(ui.dialogs().get_playlist_picker_open());
    assert!((770.0..830.0).contains(&ui.dialogs().get_playlist_picker_x()));
    assert!((680.0..730.0).contains(&ui.dialogs().get_playlist_picker_y()));
    assert!(matches!(
        playlist_rx.try_recv().unwrap(),
        Request::PlaylistChoices(42)
    ));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-playlist-picker");
    click(window, 800.0, 400.0);
    ui.player().set_full_player(false);
    ui.player().set_liked(false);
    ui.library().set_playlists(Default::default());
    ui.player().set_now(previous_song);
    ui.player().set_volume(0.5);
    let weak_volume = ui.as_weak();
    let volume_changes = Rc::new(Cell::new(0));
    let observed_volume = volume_changes.clone();
    ui.player().on_volume_changed(move |value| {
        observed_volume.set(observed_volume.get() + 1);
        if let Some(ui) = weak_volume.upgrade() {
            ui.player().set_volume(value);
        }
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1244.0, 764.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1244.0, 666.0),
    });
    std::thread::sleep(Duration::from_millis(650));
    slint::platform::update_timers_and_animations();
    click(window, 1244.0, 622.0);
    assert!(
        ui.player().get_volume() > 0.8,
        "volume popup must remain interactive while hovered"
    );
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(1244.0, 666.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1244.0, 712.0),
    });
    assert!(
        ui.player().get_volume() < 0.4,
        "dragging the popup must adjust volume"
    );
    std::thread::sleep(Duration::from_millis(650));
    slint::platform::update_timers_and_animations();
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(1244.0, 712.0),
        button: PointerEventButton::Left,
    });
    assert!(volume_changes.get() >= 3);
    ui.player().set_output_unprocessed(true);
    let before = volume_changes.get();
    click(window, 1244.0, 764.0);
    assert_eq!(
        volume_changes.get(),
        before,
        "unprocessed output must not apply software volume"
    );
    ui.player().set_output_unprocessed(false);
    ui.shell().set_tray_available(false);
    set_background_playback(ui, true);
    assert!(
        !ui.shell().get_window_hidden(),
        "hiding is unavailable without a recovery tray"
    );
    ui.shell().set_tray_available(true);
    let was_full = ui.player().get_full_player();
    set_background_playback(ui, true);
    assert!(ui.shell().get_window_hidden());
    assert_eq!(
        ui.player().get_full_player(),
        was_full,
        "Phantom must preserve the current page"
    );
    let (tray_tx, tray_rx) = std::sync::mpsc::channel();
    for action in ["toggle", "previous", "next"] {
        dispatch_tray_action(ui, &tray_tx, action);
        assert!(
            matches!(tray_rx.try_recv().unwrap(),Request::Command(actual) if actual==crate::protocol::PlayerAction::parse(action).unwrap())
        );
    }
    dispatch_tray_action(ui, &tray_tx, "restore");
    assert!(!ui.shell().get_window_hidden());
    assert_eq!(ui.player().get_full_player(), was_full);
    toggle_background_playback(ui);
    assert!(ui.shell().get_window_hidden());
    toggle_background_playback(ui);
    assert!(!ui.shell().get_window_hidden());
    assert_eq!(ui.player().get_full_player(), was_full);
    ui.player().set_now(Song {
        title: "Stale song".into(),
        artist: "Stale artist".into(),
        album: "Stale album".into(),
        path: "removed.wav".into(),
        ..Default::default()
    });
    ui.player().set_position(12000.0);
    ui.player().set_duration(20000.0);
    ui.player().set_playing(true);
    ui.dialogs().set_collection_dialog(true);
    let original = serde_json::json!({"action":"collection-edit","kind":"artists","key":"Artist","secondary":"","draft_id":"1"});
    ui.dialogs()
        .set_collection_request(original.to_string().into());
    let mut staged = original.clone();
    staged["cover"] = serde_json::json!("");
    assert!(stage_collection_draft(ui, &staged));
    assert!(ui.dialogs().get_collection_cover_missing());
    ui.player().set_full_player(false);
    ui.dialogs().set_collection_kind("artists".into());
    ui.dialogs().set_collection_name("Preview artist".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "artist-editor-preview-smoke");
    ui.dialogs().set_collection_kind("albums".into());
    ui.dialogs().set_collection_name("Preview album".into());
    ui.preferences().set_scale(1.2);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "album-editor-preview-smoke");
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(500.0, 330.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "album-editor-cover-actions");
    ui.preferences().set_scale(1.0);
    staged["draft_id"] = serde_json::json!("old-editor");
    assert!(!stage_collection_draft(ui, &staged));
    ui.dialogs().set_collection_dialog(false);
    assert!(!stage_collection_draft(ui, &original));
    ui.dialogs().set_metadata_open(true);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(200.0, 120.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "metadata-editor-cover-actions");
    ui.dialogs().set_metadata_open(false);
    ui.player().set_full_player(false);
    ui.library().set_detail_key("".into());
    ui.shell().set_view("artists".into());
    ui.preferences().set_browse_artwork(true);
    let group = CatalogGroup {
        key: "Right-click artist".into(),
        title: "Right-click artist".into(),
        cover_missing: true,
        ..Default::default()
    };
    ui.library().set_group_grid(
        Rc::new(VecModel::from(vec![
            Rc::new(VecModel::from(vec![group])).into()
        ]))
        .into(),
    );
    let selected = Rc::new(RefCell::new(String::new()));
    let observed = selected.clone();
    ui.dialogs()
        .on_collection_menu(move |group, _, _| *observed.borrow_mut() = group.key.to_string());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    ui.shell().set_keyboard_navigation(true);
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(200.0, 130.0),
        button: PointerEventButton::Right,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(200.0, 130.0),
        button: PointerEventButton::Right,
    });
    assert_eq!(selected.borrow().as_str(), "Right-click artist");
    assert!(
        !ui.shell().get_keyboard_navigation(),
        "right-click menus must hide keyboard-only focus outlines"
    );
    reset_empty_player(ui);
    assert!(ui.player().get_now().path.is_empty());
    assert!(ui.player().get_now().title.is_empty());
    assert!(ui.player().get_now().artist.is_empty());
    assert!(ui.player().get_now().album.is_empty());
    assert!(ui.player().get_now().cover_missing);
    assert_eq!(ui.player().get_position(), 0.0);
    assert_eq!(ui.player().get_duration(), 0.0);
    assert!(!ui.player().get_playing());
    assert!(ui.player().get_original_cover_missing());
}

pub(super) fn verify_auto_hide(case: &mut UiCase) {
    case.ui.preferences().set_full_artwork(true);
    case.ui.player().set_full_player(true);
    case.ui.player().set_playing(true);
    case.ui.preferences().set_auto_hide_player_controls(true);
    case.ui.shell().set_keyboard_navigation(false);
    case.ui.lyrics().set_lyrics_open(false);
    case.ui.player().set_queue_open(false);
    case.ui.dialogs().set_playlist_picker_open(false);
    case.ui.player().set_player_interacting(false);
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    let visible_cover_size = case.ui.player().get_player_cover_size();
    case.window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(30.0, 400.0),
    });
    assert!(!case.ui.player().get_player_idle());
    std::thread::sleep(Duration::from_millis(3100));
    slint::platform::update_timers_and_animations();
    assert!(
        case.ui.player().get_player_idle(),
        "full player must become idle after three seconds"
    );
    std::thread::sleep(Duration::from_millis(180));
    slint::platform::update_timers_and_animations();
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    assert!(
        case.ui.player().get_player_cover_size() > visible_cover_size,
        "idle artwork should grow when controls release their layout space"
    );
    case.window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(35.0, 401.0),
    });
    assert!(
        !case.ui.player().get_player_idle(),
        "pointer movement must reveal controls"
    );
    case.ui.preferences().set_auto_hide_player_controls(false);
    case.ui.player().set_playing(false);
    case.ui.player().set_full_player(false);
}

pub(super) fn verify_controller(ui: &OrcaWindow) {
    let (tx, rx) = std::sync::mpsc::channel();
    let player = Rc::new(crate::playback::state::PlayerModels::new());
    *player.lines.borrow_mut() =
        crate::lyrics::parse("[00:01.00]First line\n[00:02.00]Second line");
    controllers::player::install(ui, tx, player.clone());

    ui.player().set_duration(3000.0);
    ui.player().invoke_seek(f32::NAN);
    assert!(rx.try_recv().is_err());
    assert!(player.preview.borrow().is_none());
    ui.player().invoke_seek(9000.0);
    assert!(matches!(rx.try_recv().unwrap(),Request::Seek(value) if value==3000.0));
    assert_eq!(ui.player().get_position(), 3000.0);
    assert_eq!(ui.lyrics().get_active_line(), 1);
    ui.player().invoke_seek(-50.0);
    assert!(matches!(rx.try_recv().unwrap(),Request::Seek(value) if value==0.0));
    assert_eq!(ui.player().get_position(), 0.0);
    ui.player().set_output_unprocessed(false);
    ui.player().invoke_volume_changed(f32::INFINITY);
    assert!(rx.try_recv().is_err());
    ui.player().invoke_volume_changed(2.0);
    assert!(matches!(rx.try_recv().unwrap(),Request::Volume(value) if value==1.0));
    ui.player().set_output_unprocessed(true);
    ui.player().invoke_volume_changed(0.5);
    assert!(rx.try_recv().is_err());
    ui.player().set_output_unprocessed(false);
}

#[test]
fn active_player_buttons_share_size_and_vertical_center() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    ui.preferences().set_full_artwork(false);
    ui.preferences().set_full_player_background("off".into());
    ui.preferences().set_show_liked_songs(true);
    ui.preferences().set_auto_hide_player_controls(false);
    ui.player().set_now(Song {
        id: 1,
        path: "controls.wav".into(),
        title: "Controls".into(),
        ..Default::default()
    });
    ui.player()
        .set_player_accent(slint::Color::from_rgb_u8(20, 220, 80));
    ui.player().set_full_player(true);
    ui.player().set_liked(true);
    ui.player().set_shuffle(true);
    ui.player().set_repeat(1);
    ui.dialogs().set_playlist_picker_open(true);
    ui.show().unwrap();
    for dpr in [1.0_f32, 1.25, 1.5] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
        ui.shell().set_display_dpr(dpr);
        for (width, height) in [(1000, 640), (1280, 820), (1920, 1080)] {
            let w = (width as f32 * dpr) as usize;
            let h = (height as f32 * dpr) as usize;
            window.set_size(slint::PhysicalSize::new(w as u32, h as u32));
            for lyrics in [false, true] {
                ui.lyrics().set_lyrics_open(lyrics);
                let mut pixels = vec![slint::Rgb8Pixel::default(); w * h];
                window.request_redraw();
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, w);
                });
                let green = |p: slint::Rgb8Pixel| p.r == 20 && p.g == 220 && p.b == 80;
                let mut boxes = Vec::new();
                let mut start = None;
                for x in 0..=w {
                    let marked =
                        x < w && (h / 3..h).filter(|&y| green(pixels[y * w + x])).count() > 5;
                    if marked {
                        start.get_or_insert(x);
                    } else if let Some(left) = start.take() {
                        if x - left < (14.0 * dpr) as usize {
                            continue;
                        }
                        let ys: Vec<_> = (h / 3..h)
                            .filter(|&y| (left..x).any(|column| green(pixels[y * w + column])))
                            .collect();
                        boxes.push((x - left, *ys.first().unwrap(), *ys.last().unwrap()));
                    }
                }
                assert_eq!(boxes.len(),4,"four active controls at {width}x{height}, dpr={dpr}, lyrics={lyrics}: {boxes:?}");
                let reference = boxes[0];
                for item in &boxes[1..] {
                    assert!(
                        item.0.abs_diff(reference.0) <= 1,
                        "active tile widths differ: {boxes:?}"
                    );
                    assert!(
                        (item.1 + item.2).abs_diff(reference.1 + reference.2) <= 2,
                        "active controls are vertically offset: {boxes:?}"
                    );
                }
            }
        }
    }
    ui.hide().unwrap();
}
