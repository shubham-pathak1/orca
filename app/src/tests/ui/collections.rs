use super::*;

pub(super) fn verify_narrow_details(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    window.set_size(slint::PhysicalSize::new(800, 820));
    state.set_scale(1.0);
    state.set_display_dpr(1.0);
    state.set_full_player(false);
    state.set_metadata_open(false);
    state.set_collection_dialog(false);
    state.set_playlist_dialog(false);
    state.set_queue_open(false);
    state.set_menu_open(false);
    state.set_collection_menu_open(false);
    state.set_light_theme(false);
    state.set_blurred_background(false);
    state.set_browse_artwork(true);
    state.set_compact_artwork(true);
    state.set_quality_info(true);
    state.set_view("artists".into());
    state.set_detail_key("Artist".into());
    state.set_songs(
        Rc::new(VecModel::from(
            (0..20)
                .map(|index| Song {
                    path: format!("track-{index}").into(),
                    title: format!("Track {index}").into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    state.set_related_albums(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            CatalogGroup {
                key: "Compilation".into(),
                title: "Compilation".into(),
                secondary: "Various Artists".into(),
                count: 2,
                ..Default::default()
            },
            CatalogGroup::default(),
        ]))
        .into()]))
        .into(),
    );
    let opened = Rc::new(RefCell::new(Vec::new()));
    let observed = opened.clone();
    state.on_open_group(move |key, secondary, _| {
        observed
            .borrow_mut()
            .push((key.to_string(), secondary.to_string()))
    });
    let mut pixels = vec![slint::Rgb8Pixel::default(); 800 * 820];
    let mut now = state.get_now();
    now.quality = "".into();
    state.set_now(now.clone());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    let without_quality = pixels.clone();
    now.quality = "FLAC | 44.1 KHZ | 777 KBPS".into();
    state.set_now(now);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    let changed = (682..728)
        .flat_map(|y| (130..500).map(move |x| y * 800 + x))
        .filter(|&index| pixels[index] != without_quality[index])
        .count();
    assert!(
        changed > 30,
        "narrow player must render the enabled quality label"
    );
    for _ in 0..3 {
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(350.0, 480.0),
            delta_x: 0.0,
            delta_y: -9999.0,
        });
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 800);
        });
    }
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/narrow.png"),
        &bytes,
        800,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    click(window, 150.0, 500.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&("Compilation".into(), "Various Artists".into())),
        "scrolling past songs must expose clickable albums at narrow widths"
    );
    window.set_size(slint::PhysicalSize::new(1400, 820));
    state.set_view("folders".into());
    state.set_detail_key("Music".into());
    state.set_related_albums(Default::default());
    state.set_group_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            CatalogGroup {
                key: "Music/Child".into(),
                title: "Child".into(),
                ..Default::default()
            },
        ]))
        .into()]))
        .into(),
    );
    pixels.resize(1400 * 820, slint::Rgb8Pixel::default());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1400);
    });
    click(window, 1220.0, 410.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&("Music/Child".into(), "".into())),
        "subfolders must be reachable from the right-hand panel"
    );
    window.set_size(slint::PhysicalSize::new(800, 820));
    pixels.resize(800 * 820, slint::Rgb8Pixel::default());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    let before = opened.borrow().len();
    click(window, 280.0, 320.0);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    click(window, 160.0, 400.0);
    assert_eq!(
        opened.borrow().len(),
        before + 1,
        "narrow folder tab must expose children without scrolling past songs"
    );
    state.set_detail_key("Music/Next".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    click(window, 160.0, 400.0);
    assert_eq!(
        opened.borrow().len(),
        before + 2,
        "folder navigation must preserve the selected Folders tab"
    );
    click(window, 150.0, 320.0);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    click(window, 160.0, 400.0);
    assert_eq!(
        opened.borrow().len(),
        before + 2,
        "returning to Songs must hide folder actions"
    );
}

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    state.set_full_player(false);
    state.set_view("artists".into());
    state.set_detail_key("Artist".into());
    state.set_detail_title("Artist".into());
    state.set_detail_summary("2 albums · 14 songs · 38 mins".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "artist-smoke");
    let edits = Rc::new(RefCell::new(Vec::new()));
    let observed = edits.clone();
    state.on_collection_action(move |action| observed.borrow_mut().push(action.to_string()));
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(120.0, 150.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(170.0, 220.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 170.0, 220.0);
    assert_eq!(
        edits.borrow().last().map(String::as_str),
        Some("cover"),
        "hover control must stay clickable"
    );
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(210.0, 220.0),
    });
    click(window, 210.0, 220.0);
    assert_eq!(
        edits.borrow().last().map(String::as_str),
        Some("remove-cover")
    );
    click(window, 388.0, 230.0);
    assert_eq!(edits.borrow().last().map(String::as_str), Some("name"));

    let queued = Rc::new(Cell::new(false));
    let observed = queued.clone();
    state.on_command(move |action, _| {
        if action == "queue-context" {
            observed.set(true);
        }
    });
    click(window, 330.0, 230.0);
    assert!(
        queued.get(),
        "artist queue button must issue an add-context command"
    );
    assert!(
        state.get_queue_open(),
        "artist queue button should show its result"
    );
    state.set_queue_open(false);

    let (catalog_query, _) = query(ui);
    assert_eq!(catalog_query.kind, "artists");
    assert_eq!(catalog_query.key, "Artist");
    state.set_detail_key("".into());
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = changes.clone();
    state.on_queue_action(move |action, source, target| {
        observed
            .borrow_mut()
            .push((action.to_string(), source.to_string(), target.to_string()))
    });
    state.set_queue(
        Rc::new(VecModel::from(
            ["a", "b", "c"]
                .into_iter()
                .map(|path| Song {
                    path: path.into(),
                    title: path.into(),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    state.set_queue_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(920.0, 695.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(920.0, 762.0),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(920.0, 762.0),
        button: PointerEventButton::Left,
    });
    assert!(
        changes
            .borrow()
            .iter()
            .any(|(action, source, target)| action == "move" && source == "b" && target == "c"),
        "queue drag should request a move without resetting playback"
    );
    state.set_queue_open(false);
    state.set_full_player(false);
    // Leaving any detail page must clear its context and destroy its controls.
    let detail_commands = Rc::new(Cell::new(0));
    let observed = detail_commands.clone();
    state.on_command(move |action, _| {
        if action == "play-context" {
            observed.set(observed.get() + 1);
        }
    });
    for kind in ["artists", "albums", "genres", "playlists"] {
        state.set_view(kind.into());
        state.set_detail_key("Old detail".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        state.set_view("settings".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(state.get_detail_key().is_empty());
        assert!(query(ui).0.kind.is_empty());
        click(window, 286.0, 230.0);
    }
    assert_eq!(
        detail_commands.get(),
        0,
        "detail actions must not survive on Settings"
    );
}
