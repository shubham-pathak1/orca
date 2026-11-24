use super::*;

#[test]
fn playback_shortcuts_route_shuffle_repeat_and_like() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    install_keyboard(&ui, tx.clone());
    controllers::playlists::install(&ui, tx);
    ui.preferences().set_show_liked_songs(true);
    ui.player().set_now(Song {
        id: 42,
        path: "fixture.flac".into(),
        ..Default::default()
    });
    ui.show().unwrap();
    ui.invoke_focus_shell();

    for key in ["s", "r", "l"] {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Alt.into(),
        });
        window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        window.dispatch_event(WindowEvent::KeyReleased {
            text: slint::platform::Key::Alt.into(),
        });
    }
    assert!(matches!(
        rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Shuffle)
    ));
    assert!(matches!(
        rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Repeat)
    ));
    assert!(matches!(rx.try_recv().unwrap(), Request::ToggleLiked(42)));
    assert!(!ui.lyrics().get_lyrics_open());
    assert!(ui.shell().invoke_keyboard("l".into(), false));
    assert!(ui.lyrics().get_lyrics_open());
    assert!(rx.try_recv().is_err());
}

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let mut current = ui.player().get_now();
    current.path = "fixture.flac".into();
    ui.player().set_now(current);
    let pixels = &mut case.pixels;
    let songs = &case.songs;
    let shortcuts = &case.shortcuts;
    ui.player().set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 1160.0, 72.0);
    assert!(
        !ui.shell().get_keyboard_navigation(),
        "mouse buttons hide keyboard-only focus rings"
    );
    let before = shortcuts.get();
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(
        shortcuts.get(),
        before + 1,
        "full player buttons must preserve keyboard focus"
    );
    ui.player().set_full_player(false);
    let (key_tx, key_rx) = std::sync::mpsc::channel();
    install_keyboard(ui, key_tx);
    ui.dialogs().set_menu_open(true);
    assert!(ui
        .shell()
        .invoke_keyboard(slint::platform::Key::Escape.into(), false));
    assert!(!ui.dialogs().get_menu_open());

    ui.player().set_full_player(true);
    ui.lyrics().set_lyrics_open(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 1160.0, 72.0);
    for (key, alt, command) in [
        (" ", false, "toggle"),
        ("n", true, "next"),
        ("p", true, "previous"),
    ] {
        assert!(ui.shell().invoke_keyboard(key.into(), alt));
        assert!(
            matches!(key_rx.try_recv().unwrap(),Request::Command(actual) if actual==crate::protocol::PlayerAction::parse(command).unwrap())
        );
    }
    ui.player().set_volume(0.7);
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(ui.player().get_volume(), 0.0);
    assert!(
        !ui.shell().get_keyboard_navigation(),
        "a playback shortcut after a mouse click must not show a focus ring"
    );
    assert!(matches!(key_rx.try_recv().unwrap(),Request::Volume(v) if v==0.0));
    assert!(ui.shell().invoke_keyboard("M".into(), false));
    assert!((ui.player().get_volume() - 0.7).abs() < 0.001);
    assert!(matches!(key_rx.try_recv().unwrap(),Request::Volume(v) if (v-0.7).abs()<0.001));
    let lyrics_open = ui.lyrics().get_lyrics_open();
    window.dispatch_event(WindowEvent::KeyPressed { text: "l".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "l".into() });
    assert_ne!(ui.lyrics().get_lyrics_open(), lyrics_open);
    // New shortcuts work from every browsing page, and searching restores that page.
    for view in [
        "songs",
        "artists",
        "albums",
        "genres",
        "playlists",
        "settings",
    ] {
        ui.player().set_full_player(false);
        ui.lyrics().set_lyrics_open(false);
        ui.shell().set_view(view.into());
        ui.library().set_detail_key("".into());
        assert!(ui.shell().invoke_keyboard("l".into(), false));
        assert!(!ui.player().get_full_player() && ui.lyrics().get_lyrics_open());
        assert!(ui.shell().invoke_keyboard("L".into(), false));
        assert!(!ui.lyrics().get_lyrics_open());
        assert!(ui.shell().invoke_keyboard("f".into(), false));
        assert!(ui.player().get_full_player());
        assert!(ui.shell().invoke_keyboard("F".into(), false));
        assert!(!ui.player().get_full_player());
        ui.lyrics().set_lyrics_open(true);
        ui.dialogs().set_playlist_picker_open(true);
        assert!(ui.shell().invoke_keyboard("q".into(), false));
        assert!(ui.player().get_queue_open());
        assert!(ui.lyrics().get_lyrics_open() && !ui.dialogs().get_playlist_picker_open());
        assert!(
            matches!(
                key_rx.try_recv().unwrap(),
                Request::Command(crate::protocol::PlayerAction::QueuePreview)
            ),
            "Q must request the same artwork refresh as the queue button"
        );
        assert!(ui.shell().invoke_keyboard("Q".into(), false));
        assert!(!ui.player().get_queue_open());
        assert!(ui.lyrics().get_lyrics_open());
        assert!(ui.shell().invoke_keyboard("focus-search".into(), false));
        std::thread::sleep(Duration::from_millis(3));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(!ui.player().get_full_player());
        assert_eq!(ui.shell().get_view().as_str(), view);
        assert!(
            ui.shell().get_text_input_focused(),
            "search must receive focus on {view}"
        );
        window.dispatch_event(WindowEvent::KeyPressed { text: "q".into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: "q".into() });
        assert!(
            !ui.player().get_queue_open(),
            "typing q in search must not open the queue"
        );
        assert!(ui.library().get_search().ends_with('q'));
        let query = ui.library().get_search();
        assert!(ui.shell().invoke_keyboard("focus-search".into(), false));
        assert!(!ui.shell().get_text_input_focused());
        assert_eq!(
            ui.library().get_search(),
            query,
            "leaving search must preserve the query"
        );
        assert!(ui.shell().invoke_keyboard("q".into(), false));
        assert!(ui.player().get_queue_open());
        assert!(matches!(
            key_rx.try_recv().unwrap(),
            Request::Command(crate::protocol::PlayerAction::QueuePreview)
        ));
        assert!(ui.shell().invoke_keyboard("q".into(), false));
        ui.library().set_search("".into());
        ui.invoke_focus_shell();
    }
    // Two rapid Ctrl+K presses must cancel the pending focus handoff.
    assert!(ui.shell().invoke_keyboard("focus-search".into(), false));
    assert!(ui.shell().invoke_keyboard("focus-search".into(), false));
    std::thread::sleep(Duration::from_millis(3));
    slint::platform::update_timers_and_animations();
    assert!(!ui.shell().get_search_focus_pending());
    assert!(!ui.shell().get_text_input_focused());
    // Playback captures the clicked page independently of the 100ms browse debounce.
    ui.shell().set_view("albums".into());
    ui.library().set_detail_key("Chosen album".into());
    ui.library().set_detail_secondary("Chosen artist".into());
    ui.library().set_search("new filter".into());
    let play = play_request(ui, "chosen.flac".into());
    let collection = command_request(ui, "play-context".into(), 0.0).unwrap();
    ui.library().set_search("later filter".into());
    ui.library().set_detail_key("Later album".into());
    match play {
        Request::Play(path, false, captured) => {
            assert_eq!(path, "chosen.flac");
            assert_eq!(captured.kind, "albums");
            assert_eq!(captured.key, "Chosen album");
            assert_eq!(captured.secondary, "Chosen artist");
            assert_eq!(captured.search, "new filter");
        }
        _ => panic!("expected captured playback request"),
    }
    match collection {
        Request::Collection(action, captured) => {
            assert_eq!(action, crate::protocol::CollectionAction::Play);
            assert_eq!(captured.key, "Chosen album");
            assert_eq!(captured.search, "new filter");
        }
        _ => panic!("expected captured collection request"),
    }
    for full_player in [false, true] {
        ui.player().set_full_player(full_player);
        ui.preferences().set_scale(1.2);
        ui.invoke_focus_shell();
        window.dispatch_event(WindowEvent::KeyPressed {
            text: slint::platform::Key::Control.into(),
        });
        window.dispatch_event(WindowEvent::KeyPressed { text: "0".into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: "0".into() });
        window.dispatch_event(WindowEvent::KeyReleased {
            text: slint::platform::Key::Control.into(),
        });
        assert_eq!(
            ui.preferences().get_scale(),
            1.0,
            "Ctrl+0 must reset scale in browsing and the full player"
        );
    }
    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("Artist".into());
    ui.library().set_search("old query".into());
    ui.player().set_full_player(true);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed { text: "k".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "k".into() });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Control.into(),
    });
    std::thread::sleep(Duration::from_millis(3));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::KeyPressed { text: "new".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "new".into() });
    assert_eq!(
        ui.library().get_search(),
        "new",
        "focusing search must select the old query"
    );
    ui.player().set_full_player(true);
    ui.invoke_focus_shell();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed { text: ",".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: ",".into() });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Control.into(),
    });
    assert_eq!(ui.shell().get_view().as_str(), "settings");
    assert!(!ui.player().get_full_player());
    assert!(ui.library().get_search().is_empty());
    ui.library().set_search("".into());
    ui.library().set_detail_key("".into());
    ui.shell().set_view("songs".into());
    ui.player().set_full_player(true);
    ui.invoke_focus_shell();
    assert!(ui
        .shell()
        .invoke_keyboard(slint::platform::Key::F11.into(), false));
    assert!(ui
        .shell()
        .invoke_keyboard(slint::platform::Key::F11.into(), false));
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(!ui.player().get_full_player());
    window.dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
    assert!(matches!(
        key_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Toggle)
    ));
    window.set_size(slint::PhysicalSize::new(500, 600));
    ui.library().set_columns(2);
    ui.library().set_song_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            songs.row_data(0).unwrap(),
            Song::default(),
        ]))
        .into()]))
        .into(),
    );
    let mut narrow = vec![slint::Rgb8Pixel::default(); 500 * 600];
    for view in ["songs", "settings"] {
        ui.shell().set_view(view.into());
        window.draw_if_needed(|renderer| {
            renderer.render(&mut narrow, 500);
        });
        assert!(
            narrow.iter().any(|p| p.r > 100),
            "narrow {view} should render"
        );
        let bytes: Vec<u8> = narrow.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        save_screenshot(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../target/slint/test-artifacts/ui/narrow-{view}-smoke.png"
            )),
            &bytes,
            500,
            600,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
    ui.preferences().set_scale(1.2);
    for editor in ["metadata", "collection"] {
        ui.dialogs().set_metadata_open(editor == "metadata");
        ui.dialogs().set_collection_dialog(editor == "collection");
        ui.dialogs().set_collection_kind("artists".into());
        ui.dialogs().set_collection_name("Narrow artist".into());
        window.draw_if_needed(|renderer| {
            renderer.render(&mut narrow, 500);
        });
        let bytes: Vec<u8> = narrow.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        save_screenshot(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../target/slint/test-artifacts/ui/narrow-{editor}-smoke.png"
            )),
            &bytes,
            500,
            600,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
    ui.dialogs().set_metadata_open(false);
    ui.dialogs().set_collection_dialog(false);
    ui.preferences().set_scale(1.0);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    ui.shell().set_view("settings".into());
    ui.dialogs().set_draft(metadata::parse(r#"{"path":"fixture.flac","title":"Title","artist":"Artist","album":"Album","album_artist":"Artist"}"#).unwrap());
    ui.dialogs().set_metadata_open(true);
}
