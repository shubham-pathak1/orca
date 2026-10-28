use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let (font_tx, _font_rx) = std::sync::mpsc::channel();
    controllers::preferences::install(ui, font_tx, Rc::new(Timer::default()));
    state.set_font_path("".into());
    state.set_font_family("".into());
    state.set_view("settings".into());
    state.set_search("tooltips".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(state.get_show_tooltips());
    screenshot(pixels, "tooltips-setting");
    click(window, 875.0, 192.0);
    assert!(!state.get_show_tooltips());
    let saved_tooltips = controllers::preferences::settings(ui);
    assert!(!saved_tooltips.show_tooltips);
    state.set_show_tooltips(true);
    controllers::preferences::apply_settings(ui, saved_tooltips);
    assert!(!state.get_show_tooltips());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 875.0, 192.0);
    assert!(state.get_show_tooltips());
    state.set_search("Lyrics alignment".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "lyrics-alignment-setting");
    let bulk_actions = Rc::new(RefCell::new(Vec::new()));
    let observed_bulk = bulk_actions.clone();
    state.on_artwork_bulk(move |action| observed_bulk.borrow_mut().push(action.to_string()));
    state.set_search("bulk".into());
    state.set_artwork_bulk_started(true);
    state.set_artwork_bulk_running(true);
    state.set_artwork_bulk_progress("Fetching · 12 lookups · 4 images found".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "bulk-artwork-setting");
    click(window, 800.0, 212.0);
    assert_eq!(
        bulk_actions.borrow().last().map(String::as_str),
        Some("pause")
    );
    state.set_artwork_bulk_running(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 800.0, 212.0);
    assert_eq!(
        bulk_actions.borrow().last().map(String::as_str),
        Some("resume")
    );
    state.set_artwork_bulk_started(false);
    state.set_search("Font preset".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    state.set_font_path("C:/Fonts/My Font.ttf".into());
    state.set_custom_font_family("My Font".into());
    state.set_font_family("My Font".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "selected-custom-font-setting");
    window.set_size(slint::PhysicalSize::new(500, 820));
    let mut narrow = vec![slint::Rgb8Pixel::default(); 500 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut narrow, 500);
    });
    let bytes: Vec<u8> = narrow.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/selected-custom-font-setting-narrow.png"),
        &bytes,
        500,
        820,
        image::ColorType::Rgb8,
    )
    .unwrap();
    window.set_size(slint::PhysicalSize::new(1280, 820));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_eq!(state.get_font_family(), "My Font");
    assert_eq!(state.get_font_path(), "C:/Fonts/My Font.ttf");
    state.invoke_font_action("system".into());
    assert_eq!(state.get_font_family(), "");
    assert_eq!(state.get_custom_font_family(), "My Font");
    assert_eq!(state.get_font_path(), "C:/Fonts/My Font.ttf");
    let saved = controllers::preferences::settings(ui);
    assert_eq!(saved.font_family, "");
    assert_eq!(saved.font_path, "C:/Fonts/My Font.ttf");
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "system-font-with-custom-option");
    state.invoke_font_action("custom".into());
    assert_eq!(state.get_font_family(), "My Font");
    state.invoke_font_action("preset".into());
    assert_eq!(state.get_font_family(), "Plus Jakarta Sans");
    assert_eq!(state.get_font_path(), "C:/Fonts/My Font.ttf");
    state.invoke_font_action("remove".into());
    assert_eq!(state.get_font_family(), "Plus Jakarta Sans");
    assert_eq!(state.get_font_path(), "");
    assert_eq!(state.get_custom_font_family(), "");
    let font_file =
        std::env::temp_dir().join(format!("orca-font-setting-{}.ttf", std::process::id()));
    std::fs::write(
        &font_file,
        include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf"),
    )
    .unwrap();
    let mut persisted = controllers::preferences::settings(ui);
    persisted.font_family = "".into();
    persisted.font_path = font_file.to_string_lossy().into_owned();
    controllers::preferences::apply_settings(ui, persisted);
    assert_eq!(state.get_font_family(), "");
    assert_eq!(state.get_custom_font_family(), "Plus Jakarta Sans");
    assert_eq!(state.get_font_path(), font_file.to_string_lossy());
    state.invoke_font_action("remove".into());
    std::fs::remove_file(font_file).unwrap();
    state.invoke_font_action("preset".into());
    state.set_search("".into());
    let current = controllers::preferences::settings(ui);
    let older = serde_json::from_str(r#"{"blurred_background":false}"#).unwrap();
    controllers::preferences::apply_settings(ui, older);
    assert_eq!(state.get_full_player_background(), "off");
    assert!(!state.get_lyrics_left_aligned());
    assert!(
        !state.get_replaygain(),
        "older settings should keep ReplayGain off"
    );
    controllers::preferences::apply_settings(ui, current.clone());
    state.set_replaygain(true);
    assert!(controllers::preferences::settings(ui).replaygain);
    state.set_lyrics_left_aligned(true);
    let saved = controllers::preferences::settings(ui);
    assert!(saved.lyrics_left_aligned);
    state.set_lyrics_left_aligned(false);
    controllers::preferences::apply_settings(ui, saved);
    assert!(state.get_lyrics_left_aligned());
    controllers::preferences::apply_settings(ui, current);
    state.set_light_theme(false);
    state.set_view("settings".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "settings-smoke");
    state.set_search("Dynamic cover accent".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let toggle_pixels = (150..225)
        .flat_map(|y| (850..900).map(move |x| y * 1280 + x))
        .filter(|&index| pixels[index].r > 200 && pixels[index].g > 200 && pixels[index].b > 200)
        .count();
    assert!(
        toggle_pixels > 100,
        "settings toggle must remain inside its control column"
    );
    state.set_search("Full player background".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-background-setting");
    state.set_search("Visuals".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 230.0, 228.0);
    assert!(
        !state.get_browse_artwork()
            && !state.get_compact_artwork()
            && !state.get_full_artwork()
            && !state.get_blurred_background()
            && state.get_full_player_background() == "off"
    );
    assert!(!state.get_grid());
    assert_eq!(
        state.get_font_family(),
        "Plus Jakarta Sans",
        "Minimal must preserve the user's font"
    );
    state.set_view("songs".into());
    state.set_search("".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "minimal-library-smoke");
    state.set_view("settings".into());
    state.set_search("Visuals".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 120.0, 228.0);
    assert!(
        state.get_browse_artwork()
            && state.get_compact_artwork()
            && state.get_full_artwork()
            && state.get_blurred_background()
            && state.get_full_player_background() == "artwork"
    );
    state.set_search("Keyboard shortcuts".into());
    click(window, 210.0, 102.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "shortcuts-smoke");
    state.set_search("".into());
    state.set_group_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            CatalogGroup {
                key: "fixture".into(),
                title: "Example collection".into(),
                subtitle: "14 songs".into(),
                ..Default::default()
            },
            CatalogGroup {
                key: "second".into(),
                title: "Another collection".into(),
                subtitle: "7 songs".into(),
                ..Default::default()
            },
        ]))
        .into()]))
        .into(),
    );
    for view in ["artists", "albums", "genres", "playlists"] {
        state.set_view(view.into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        screenshot(pixels, &format!("{view}-catalog-smoke"));
    }
    // Folder pages share indexed song rows, but have their own directory navigation.
    state.set_view("folders".into());
    state.set_group_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            CatalogGroup {
                key: "C:/Music/Artist".into(),
                secondary: "C:/Music".into(),
                title: "Artist".into(),
                subtitle: "2 songs".into(),
                cover_missing: true,
                ..Default::default()
            },
            CatalogGroup::default(),
            CatalogGroup::default(),
            CatalogGroup::default(),
        ]))
        .into()]))
        .into(),
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-root-smoke");
    state.set_folder_grid(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-list-smoke");
    state.set_folder_grid(true);
    let saved_groups = state.get_group_grid();
    state.set_search("missing".into());
    state.set_group_grid(Rc::new(VecModel::<slint::ModelRc<CatalogGroup>>::default()).into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-search-empty-smoke");
    state.set_search("".into());
    state.set_group_grid(saved_groups);

    state.set_detail_key("C:/Music".into());
    state.set_detail_secondary("".into());
    state.set_detail_title("Music".into());
    state.set_detail_cover_missing(true);
    state.set_detail_summary("3 songs, including subfolders".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-detail-smoke");
    let Request::Collection(_, folder_query) =
        command_request(ui, "play-context".into(), 0.0).unwrap()
    else {
        panic!("expected folder playback context")
    };
    assert_eq!(folder_query.kind, "folders");
    assert_eq!(folder_query.key, "C:/Music");
    state.set_detail_key("".into());
    state.set_scale(1.2);
    state.set_view("songs".into());
    state.set_now(Song {
        title: "hours".into(),
        artist: "again&again".into(),
        album: "hours".into(),
        quality: "FLAC | 44.1 KHZ | 905 KBPS".into(),
        ..Default::default()
    });
    state.set_blurred_background(true);
    let gradient = image::RgbaImage::from_fn(1280, 820, |x, _| {
        image::Rgba([(x * 200 / 1279) as u8, 30, 80, 255])
    });
    state.set_backdrop(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
        slint::Rgba8Pixel,
    >::clone_from_slice(
        gradient.as_raw(), 1280, 820
    )));
    for _ in 0..14 {
        std::thread::sleep(Duration::from_millis(33));
        slint::platform::update_timers_and_animations();
    }
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "max-font-library-smoke");
    // A smooth source wash must not gain a vertical split near the window center.
    for y in [180usize, 300, 600] {
        let left = pixels[y * 1280 + 639];
        let right = pixels[y * 1280 + 640];
        assert!(
            (left.r as i16 - right.r as i16).abs() < 3,
            "background wash has a seam"
        );
    }
    // Compare dirty-region repainting with a fresh complete repaint after
    // navigation and playback metadata changes. The desktop backend reuses buffers.
    for (view, title) in [
        ("artists", "ANIMAL"),
        ("albums", "A very different title"),
        ("genres", "Any Second"),
    ] {
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            renderer.render(pixels, 1280);
        });
        state.set_view(view.into());
        state.set_now(Song {
            title: title.into(),
            artist: "Another artist".into(),
            album: "Another album".into(),
            quality: "FLAC | 44.1 KHZ".into(),
            ..Default::default()
        });
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let reused = pixels.clone();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            renderer.render(pixels, 1280);
        });
        let differences = reused
            .iter()
            .zip(pixels.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differences, 0,
            "dirty repaint must match a complete repaint after {view} navigation"
        );
    }
}
