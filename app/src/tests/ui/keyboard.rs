use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let songs = &case.songs;
    let shortcuts = &case.shortcuts;
    state.set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 1160.0, 72.0);
    assert!(
        !state.get_keyboard_navigation(),
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
    state.set_full_player(false);
    let (key_tx, key_rx) = std::sync::mpsc::channel();
    install_keyboard(ui, key_tx);
    state.set_menu_open(true);
    assert!(state.invoke_keyboard(slint::platform::Key::Escape.into(), false));
    assert!(!state.get_menu_open());

    state.set_full_player(true);
    state.set_lyrics_open(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 1160.0, 72.0);
    for (key, alt, command) in [
        (" ", false, "toggle"),
        ("n", true, "next"),
        ("p", true, "previous"),
    ] {
        assert!(state.invoke_keyboard(key.into(), alt));
        assert!(
            matches!(key_rx.try_recv().unwrap(),Request::Command(actual) if actual==crate::protocol::PlayerAction::parse(command).unwrap())
        );
    }
    state.set_volume(0.7);
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(state.get_volume(), 0.0);
    assert!(matches!(key_rx.try_recv().unwrap(),Request::Volume(v) if v==0.0));
    assert!(state.invoke_keyboard("M".into(), false));
    assert!((state.get_volume() - 0.7).abs() < 0.001);
    assert!(matches!(key_rx.try_recv().unwrap(),Request::Volume(v) if (v-0.7).abs()<0.001));
    let lyrics_open = state.get_lyrics_open();
    window.dispatch_event(WindowEvent::KeyPressed { text: "l".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "l".into() });
    assert_ne!(state.get_lyrics_open(), lyrics_open);
    // New shortcuts work from every browsing page, and searching restores that page.
    for view in [
        "songs",
        "artists",
        "albums",
        "genres",
        "playlists",
        "settings",
    ] {
        state.set_full_player(false);
        state.set_view(view.into());
        state.set_detail_key("".into());
        assert!(state.invoke_keyboard("l".into(), false));
        assert!(state.get_full_player() && state.get_lyrics_open());
        assert!(state.invoke_keyboard("q".into(), false));
        assert!(state.get_queue_open());
        assert!(
            matches!(
                key_rx.try_recv().unwrap(),
                Request::Command(crate::protocol::PlayerAction::QueuePreview)
            ),
            "Q must request the same artwork refresh as the queue button"
        );
        assert!(state.invoke_keyboard("Q".into(), false));
        assert!(!state.get_queue_open());
        assert!(state.invoke_keyboard("focus-search".into(), false));
        std::thread::sleep(Duration::from_millis(3));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(!state.get_full_player());
        assert_eq!(state.get_view().as_str(), view);
        assert!(
            state.get_text_input_focused(),
            "search must receive focus on {view}"
        );
        window.dispatch_event(WindowEvent::KeyPressed { text: "q".into() });
        window.dispatch_event(WindowEvent::KeyReleased { text: "q".into() });
        assert!(
            !state.get_queue_open(),
            "typing q in search must not open the queue"
        );
        assert!(state.get_search().ends_with('q'));
        let query = state.get_search();
        assert!(state.invoke_keyboard("focus-search".into(), false));
        assert!(!state.get_text_input_focused());
        assert_eq!(
            state.get_search(),
            query,
            "leaving search must preserve the query"
        );
        assert!(state.invoke_keyboard("q".into(), false));
        assert!(state.get_queue_open());
        assert!(matches!(
            key_rx.try_recv().unwrap(),
            Request::Command(crate::protocol::PlayerAction::QueuePreview)
        ));
        assert!(state.invoke_keyboard("q".into(), false));
        state.set_search("".into());
        ui.invoke_focus_shell();
    }
    // Two rapid Ctrl+K presses must cancel the pending focus handoff.
    assert!(state.invoke_keyboard("focus-search".into(), false));
    assert!(state.invoke_keyboard("focus-search".into(), false));
    std::thread::sleep(Duration::from_millis(3));
    slint::platform::update_timers_and_animations();
    assert!(!state.get_search_focus_pending());
    assert!(!state.get_text_input_focused());
    // Playback captures the clicked page independently of the 100ms browse debounce.
    state.set_view("albums".into());
    state.set_detail_key("Chosen album".into());
    state.set_detail_secondary("Chosen artist".into());
    state.set_search("new filter".into());
    let play = play_request(ui, "chosen.flac".into());
    let collection = command_request(ui, "play-context".into(), 0.0).unwrap();
    state.set_search("later filter".into());
    state.set_detail_key("Later album".into());
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
    state.set_view("artists".into());
    state.set_detail_key("Artist".into());
    state.set_search("old query".into());
    state.set_full_player(true);
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
        state.get_search(),
        "new",
        "focusing search must select the old query"
    );
    state.set_search("".into());
    state.set_detail_key("".into());
    state.set_view("songs".into());
    state.set_full_player(true);
    ui.invoke_focus_shell();
    assert!(state.invoke_keyboard(slint::platform::Key::F11.into(), false));
    assert!(state.invoke_keyboard(slint::platform::Key::F11.into(), false));
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(!state.get_full_player());
    window.dispatch_event(WindowEvent::KeyPressed { text: " ".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: " ".into() });
    assert!(matches!(
        key_rx.try_recv().unwrap(),
        Request::Command(crate::protocol::PlayerAction::Toggle)
    ));
    window.set_size(slint::PhysicalSize::new(500, 600));
    state.set_columns(2);
    state.set_song_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            songs.row_data(0).unwrap(),
            Song::default(),
        ]))
        .into()]))
        .into(),
    );
    let mut narrow = vec![slint::Rgb8Pixel::default(); 500 * 600];
    for view in ["songs", "settings"] {
        state.set_view(view.into());
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
    state.set_scale(1.2);
    for editor in ["metadata", "collection"] {
        state.set_metadata_open(editor == "metadata");
        state.set_collection_dialog(editor == "collection");
        state.set_collection_kind("artists".into());
        state.set_collection_name("Narrow artist".into());
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
    state.set_metadata_open(false);
    state.set_collection_dialog(false);
    state.set_scale(1.0);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    state.set_view("settings".into());
    state.set_draft(metadata::parse(r#"{"path":"fixture.flac","title":"Title","artist":"Artist","album":"Album","album_artist":"Artist"}"#).unwrap());
    state.set_metadata_open(true);
}
