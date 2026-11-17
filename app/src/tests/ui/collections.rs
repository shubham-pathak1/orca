use super::*;

pub(super) fn verify_narrow_details(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    window.set_size(slint::PhysicalSize::new(800, 820));
    ui.preferences().set_scale(1.0);
    ui.shell().set_display_dpr(1.0);
    ui.player().set_full_player(false);
    ui.dialogs().set_metadata_open(false);
    ui.dialogs().set_collection_dialog(false);
    ui.dialogs().set_playlist_dialog(false);
    ui.player().set_queue_open(false);
    ui.lyrics().set_lyrics_open(false);
    ui.dialogs().set_menu_open(false);
    ui.dialogs().set_collection_menu_open(false);
    ui.preferences().set_light_theme(false);
    ui.preferences().set_blurred_background(false);
    ui.preferences().set_browse_artwork(true);
    ui.preferences().set_compact_artwork(true);
    ui.preferences().set_quality_info(true);
    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("Artist".into());
    ui.library().set_songs(
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
    let albums = (0..6)
        .map(|index| CatalogGroup {
            key: if index == 0 {
                "Compilation".into()
            } else {
                format!("Album {index}").into()
            },
            title: if index == 0 {
                "Compilation".into()
            } else {
                format!("Album {index}").into()
            },
            secondary: "Various Artists".into(),
            count: 2,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let album_rows = |columns: usize| {
        Rc::new(VecModel::from(
            albums
                .chunks(columns)
                .map(|chunk| {
                    let mut row = chunk.to_vec();
                    row.resize(columns, CatalogGroup::default());
                    Rc::new(VecModel::from(row)).into()
                })
                .collect::<Vec<_>>(),
        ))
        .into()
    };
    ui.library().set_related_columns(4);
    ui.library().set_related_albums(album_rows(4));
    let opened = Rc::new(RefCell::new(Vec::new()));
    let observed = opened.clone();
    ui.library().on_open_group(move |key, secondary, _| {
        observed
            .borrow_mut()
            .push((key.to_string(), secondary.to_string()))
    });
    let mut pixels = vec![slint::Rgb8Pixel::default(); 800 * 820];
    let mut now = ui.player().get_now();
    now.quality = "".into();
    ui.player().set_now(now.clone());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    let without_quality = pixels.clone();
    now.quality = "FLAC | 44.1 KHZ | 777 KBPS".into();
    ui.player().set_now(now);
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
        Some(&("Album 4".into(), "Various Artists".into())),
        "the multi-column grid must expose albums past the songs"
    );
    ui.shell().set_view("artists".into());
    let mut now = ui.player().get_now();
    now.path = "".into();
    ui.player().set_now(now);
    ui.lyrics().set_lyrics_open(true);
    ui.library().set_related_columns(2);
    ui.library().set_related_albums(album_rows(2));
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(350.0, 480.0),
        delta_x: 0.0,
        delta_y: -9999.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    click(window, 760.0, 429.0);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 800);
    });
    click(window, 650.0, 500.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&("Album 5".into(), "Various Artists".into())),
        "the horizontal album shelf must expose albums beyond the visible width"
    );
    for delta in [9999.0, -9999.0] {
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(650.0, 500.0),
            delta_x: delta,
            delta_y: 0.0,
        });
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 800);
        });
    }
    click(window, 650.0, 500.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&("Album 5".into(), "Various Artists".into())),
        "native horizontal scrolling must expose and select recycled shelf cards"
    );
    window.set_size(slint::PhysicalSize::new(1400, 820));
    pixels.resize(1400 * 820, slint::Rgb8Pixel::default());
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(900.0, 450.0),
        delta_x: 0.0,
        delta_y: -9999.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1400);
    });
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/wide-related.png"),
        &bytes,
        1400,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    ui.lyrics().set_lyrics_open(false);
    ui.library().set_related_columns(2);
    ui.library().set_related_albums(album_rows(2));
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1400);
    });
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/wide-related-sidebar.png"),
        &bytes,
        1400,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    click(window, 1120.0, 460.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&(String::from("Compilation"), String::from("Various Artists"))),
        "related albums must be selectable in the right sidebar when lyrics are closed"
    );
    window.set_size(slint::PhysicalSize::new(960, 820));
    pixels.resize(960 * 820, slint::Rgb8Pixel::default());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 960);
    });
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/medium-related-sidebar.png"),
        &bytes,
        960,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    click(window, 810.0, 460.0);
    assert_eq!(
        opened.borrow().last(),
        Some(&(String::from("Album 1"), String::from("Various Artists"))),
        "related albums must remain in the sidebar at a medium window width"
    );
    window.set_size(slint::PhysicalSize::new(1400, 820));
    pixels.resize(1400 * 820, slint::Rgb8Pixel::default());
    ui.shell().set_view("folders".into());
    ui.library().set_detail_key("Music".into());
    ui.library().set_related_albums(Default::default());
    ui.library().set_group_grid(
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
    ui.library().set_detail_key("Music/Next".into());
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

    let pixels = &mut case.pixels;
    ui.player().set_full_player(false);
    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("Artist".into());
    ui.library().set_detail_title("Artist".into());
    ui.library()
        .set_detail_summary("2 albums · 14 songs · 38 mins".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "artist-smoke");
    let edits = Rc::new(RefCell::new(Vec::new()));
    let observed = edits.clone();
    ui.dialogs()
        .on_collection_action(move |action| observed.borrow_mut().push(action.to_string()));
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
    ui.player().on_command(move |action, _| {
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
        ui.player().get_queue_open(),
        "artist queue button should show its result"
    );
    ui.player().set_queue_open(false);

    let (catalog_query, _) = query(ui);
    assert_eq!(catalog_query.kind, "artists");
    assert_eq!(catalog_query.key, "Artist");
    ui.library().set_detail_key("".into());
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = changes.clone();
    ui.player().on_queue_action(move |action, source, target| {
        observed
            .borrow_mut()
            .push((action.to_string(), source.to_string(), target.to_string()))
    });
    ui.player().set_queue(
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
    ui.player().set_queue_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "queue-docked");
    window.dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(1000.0, 180.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1000.0, 247.0),
    });
    assert!(
        changes.borrow().is_empty(),
        "dragging must not send queue updates before release"
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "queue-live-drag");
    window.dispatch_event(WindowEvent::PointerReleased {
        position: slint::LogicalPosition::new(1000.0, 247.0),
        button: PointerEventButton::Left,
    });
    assert!(
        changes
            .borrow()
            .iter()
            .any(|(action, source, target)| action == "move" && source == "b" && target == "c"),
        "queue drag should request a move without resetting playback"
    );
    ui.player().set_queue_open(false);
    ui.player().set_full_player(false);
    // Leaving any detail page must clear its context and destroy its controls.
    let detail_commands = Rc::new(Cell::new(0));
    let observed = detail_commands.clone();
    ui.player().on_command(move |action, _| {
        if action == "play-context" {
            observed.set(observed.get() + 1);
        }
    });
    for kind in ["artists", "albums", "genres", "playlists"] {
        ui.shell().set_view(kind.into());
        ui.library().set_detail_key("Old detail".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        ui.shell().set_view("settings".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(ui.library().get_detail_key().is_empty());
        assert!(query(ui).0.kind.is_empty());
        click(window, 286.0, 230.0);
    }
    assert_eq!(
        detail_commands.get(),
        0,
        "detail actions must not survive on Settings"
    );
}
