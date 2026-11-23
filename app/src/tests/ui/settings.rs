use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let pixels = &mut case.pixels;
    let (font_tx, _font_rx) = std::sync::mpsc::channel();
    controllers::preferences::install(ui, font_tx, Rc::new(Timer::default()));
    ui.preferences().set_font_path("".into());
    ui.preferences().set_font_family("".into());
    ui.shell().set_view("settings".into());
    ui.player().set_output_choices(
        Rc::new(slint::VecModel::from(vec![
            crate::OutputChoice {
                id: "".into(),
                name: "System default".into(),
            },
            crate::OutputChoice {
                id: "test-endpoint".into(),
                name: "Test headphones".into(),
            },
        ]))
        .into(),
    );
    ui.player().invoke_select_output(1);
    assert_eq!(
        controllers::preferences::settings_from_ui(ui)
            .output
            .device_id,
        "test-endpoint"
    );
    ui.player().set_output_exclusive(true);
    let output_settings = controllers::preferences::settings_from_ui(ui);
    ui.player().set_output_exclusive(false);
    controllers::preferences::apply_settings(ui, output_settings);
    assert!(ui.player().get_output_exclusive());
    ui.library().set_search("Output".into());
    ui.player().set_output_device_name("Test speakers".into());
    ui.player().set_output_active_exclusive(true);
    ui.player().set_output_active_unprocessed(false);
    ui.player().set_output_rate(48000);
    ui.player().set_output_source_rate(192000);
    ui.player().set_output_channels(2);
    ui.player().set_output_sample_format("24-bit Int".into());
    ui.player().set_output_notice(
        "The device does not support 192000 Hz in exclusive mode. Using 48000 Hz with resampling."
            .into(),
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "audio-output-settings");
    ui.player().set_output_unprocessed(true);
    let unprocessed_settings = controllers::preferences::settings_from_ui(ui);
    ui.player().set_output_unprocessed(false);
    controllers::preferences::apply_settings(ui, unprocessed_settings);
    assert!(ui.player().get_output_unprocessed());
    ui.player().set_output_active_unprocessed(true);
    ui.player().set_output_rate(44100);
    ui.player().set_output_source_rate(44100);
    ui.player().set_output_sample_format("32-bit Int".into());
    ui.player()
        .set_output_notice("Use your device's volume control.".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "unprocessed-output-settings");
    ui.player().set_output_unprocessed(false);
    ui.player().set_output_notice("".into());
    ui.player().set_output_exclusive(false);
    ui.player().set_output_device_id("".into());
    ui.player().set_output_active_exclusive(false);
    ui.player().set_output_active_unprocessed(false);
    ui.library().set_search("tooltips".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(ui.preferences().get_show_tooltips());
    screenshot(pixels, "tooltips-setting");
    click(window, 875.0, 192.0);
    assert!(!ui.preferences().get_show_tooltips());
    let saved_tooltips = controllers::preferences::settings_from_ui(ui);
    assert!(!saved_tooltips.show_tooltips);
    ui.preferences().set_show_tooltips(true);
    controllers::preferences::apply_settings(ui, saved_tooltips);
    assert!(!ui.preferences().get_show_tooltips());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 875.0, 192.0);
    assert!(ui.preferences().get_show_tooltips());
    ui.library().set_search("Lyrics alignment".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "lyrics-alignment-setting");
    ui.library().set_search("Romanize lyrics".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "romanization-setting");
    assert_eq!(ui.preferences().get_romanization_mode(), 0);
    ui.preferences().set_romanization_mode(2);
    ui.preferences()
        .invoke_romanization_language("ja".into(), true);
    ui.preferences()
        .invoke_romanization_language("ko".into(), true);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 2);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "romanization-language-picker");
    // Exercise the native checkbox and the picker's own search field, rather
    // than relying only on the controller callback and saved model.
    click(window, 84.0, 355.0);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 3);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 84.0, 355.0);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 2);
    click(window, 200.0, 310.0);
    for letter in "Japanese".chars() {
        let text: slint::SharedString = letter.to_string().into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }
    assert_eq!(ui.library().get_search(), "Romanize lyrics");
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 84.0, 355.0);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 1);
    assert_eq!(
        controllers::preferences::settings_from_ui(ui).romanization_languages,
        ["ko"]
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 84.0, 355.0);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 2);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "romanization-language-filtered");

    let saved = controllers::preferences::settings_from_ui(ui);
    assert_eq!(saved.romanization_languages, ["ja", "ko"]);
    ui.preferences().set_romanization_mode(0);
    ui.preferences()
        .invoke_romanization_language("ja".into(), false);
    controllers::preferences::apply_settings(ui, saved);
    assert_eq!(ui.preferences().get_romanization_mode(), 2);
    assert_eq!(ui.preferences().get_romanization_selected_count(), 2);
    ui.preferences().set_romanization_mode(0);
    ui.preferences()
        .invoke_romanization_language("ja".into(), false);
    ui.preferences()
        .invoke_romanization_language("ko".into(), false);
    let bulk_actions = Rc::new(RefCell::new(Vec::new()));
    let observed_bulk = bulk_actions.clone();
    ui.library()
        .on_artwork_bulk(move |action| observed_bulk.borrow_mut().push(action.to_string()));
    ui.library().set_search("bulk".into());
    ui.library().set_artwork_bulk_started(true);
    ui.library().set_artwork_bulk_running(true);
    ui.library()
        .set_artwork_bulk_progress("Fetching · 12 lookups · 4 images found".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 800.0, 188.0);
    assert!(
        bulk_actions.borrow().is_empty(),
        "whole-library artwork controls must remain hidden"
    );
    ui.library().set_artwork_bulk_started(false);
    ui.library().set_artwork_bulk_running(false);
    ui.library().set_search("Font preset".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    ui.preferences()
        .set_font_path("C:/Fonts/My Font.ttf".into());
    ui.preferences().set_custom_font_family("My Font".into());
    ui.preferences().set_font_family("My Font".into());
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
    assert_eq!(ui.preferences().get_font_family(), "My Font");
    assert_eq!(ui.preferences().get_font_path(), "C:/Fonts/My Font.ttf");
    ui.preferences().invoke_font_action("system".into());
    assert_eq!(ui.preferences().get_font_family(), "");
    assert_eq!(ui.preferences().get_custom_font_family(), "My Font");
    assert_eq!(ui.preferences().get_font_path(), "C:/Fonts/My Font.ttf");
    let saved = controllers::preferences::settings_from_ui(ui);
    assert_eq!(saved.font_family, "");
    assert_eq!(saved.font_path, "C:/Fonts/My Font.ttf");
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "system-font-with-custom-option");
    ui.preferences().invoke_font_action("custom".into());
    assert_eq!(ui.preferences().get_font_family(), "My Font");
    ui.preferences().invoke_font_action("preset".into());
    assert_eq!(ui.preferences().get_font_family(), "Plus Jakarta Sans");
    assert_eq!(ui.preferences().get_font_path(), "C:/Fonts/My Font.ttf");
    ui.preferences().invoke_font_action("remove".into());
    assert_eq!(ui.preferences().get_font_family(), "Plus Jakarta Sans");
    assert_eq!(ui.preferences().get_font_path(), "");
    assert_eq!(ui.preferences().get_custom_font_family(), "");
    let font_file =
        std::env::temp_dir().join(format!("orca-font-setting-{}.ttf", std::process::id()));
    std::fs::write(
        &font_file,
        include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf"),
    )
    .unwrap();
    let mut persisted = controllers::preferences::settings_from_ui(ui);
    persisted.font_family = "".into();
    persisted.font_path = font_file.to_string_lossy().into_owned();
    controllers::preferences::apply_settings(ui, persisted);
    assert_eq!(ui.preferences().get_font_family(), "");
    assert_eq!(
        ui.preferences().get_custom_font_family(),
        "Plus Jakarta Sans"
    );
    assert_eq!(
        ui.preferences().get_font_path(),
        font_file.to_string_lossy()
    );
    ui.preferences().invoke_font_action("remove".into());
    std::fs::remove_file(font_file).unwrap();
    ui.preferences().invoke_font_action("preset".into());
    ui.library().set_search("".into());
    let current = controllers::preferences::settings_from_ui(ui);
    let mut old_blur = current.clone();
    old_blur.full_player_background = "artwork".into();
    controllers::preferences::apply_settings(ui, old_blur);
    assert_eq!(
        ui.preferences().get_full_player_background(),
        "soft-artwork"
    );
    let older = serde_json::from_str(r#"{"blurred_background":false}"#).unwrap();
    controllers::preferences::apply_settings(ui, older);
    assert_eq!(ui.preferences().get_full_player_background(), "off");
    assert!(!ui.preferences().get_lyrics_left_aligned());
    assert!(
        !ui.preferences().get_replaygain(),
        "older settings should keep ReplayGain off"
    );
    controllers::preferences::apply_settings(ui, current.clone());
    controllers::preferences::apply_settings(ui, current.clone());
    ui.preferences().set_replaygain(true);
    assert!(controllers::preferences::settings_from_ui(ui).replaygain);
    ui.preferences().set_lyrics_left_aligned(true);
    let saved = controllers::preferences::settings_from_ui(ui);
    assert!(saved.lyrics_left_aligned);
    ui.preferences().set_lyrics_left_aligned(false);
    controllers::preferences::apply_settings(ui, saved);
    assert!(ui.preferences().get_lyrics_left_aligned());
    controllers::preferences::apply_settings(ui, current);
    ui.preferences().set_light_theme(false);
    ui.shell().set_view("settings".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "settings-smoke");
    ui.library().set_search("Playback gapless".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let heading_pixels = (156..230)
        .flat_map(|y| (76..260).map(move |x| y * 1280 + x))
        .filter(|&i| pixels[i].r > 200 && pixels[i].g > 200 && pixels[i].b > 200)
        .count();
    assert!(
        heading_pixels > 25,
        "settings section headings must align with the left edge of their rows"
    );
    screenshot(pixels, "settings-playback-section-spacing");

    ui.library().set_search("Dynamic cover accent".into());
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
    ui.library().set_search("Full player background".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "full-player-background-setting");
    ui.library().set_search("Visuals".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 230.0, 228.0);
    assert!(
        !ui.preferences().get_browse_artwork()
            && !ui.preferences().get_compact_artwork()
            && !ui.preferences().get_full_artwork()
            && !ui.preferences().get_blurred_background()
            && ui.preferences().get_full_player_background() == "off"
    );
    assert!(!ui.preferences().get_grid());
    assert_eq!(
        ui.preferences().get_font_family(),
        "Plus Jakarta Sans",
        "Minimal must preserve the user's font"
    );
    ui.shell().set_view("songs".into());
    ui.library().set_search("".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "minimal-library-smoke");
    ui.shell().set_view("settings".into());
    ui.library().set_search("Visuals".into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 120.0, 228.0);
    assert!(
        ui.preferences().get_browse_artwork()
            && ui.preferences().get_compact_artwork()
            && ui.preferences().get_full_artwork()
            && !ui.preferences().get_blurred_background()
            && ui.preferences().get_full_player_background() == "soft-artwork"
    );
    ui.library().set_search("Keyboard shortcuts".into());
    click(window, 210.0, 102.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "shortcuts-smoke");
    ui.library().set_search("".into());
    ui.library().set_group_grid(
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
        ui.shell().set_view(view.into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        screenshot(pixels, &format!("{view}-catalog-smoke"));
    }
    // Folder pages share indexed song rows, but have their own directory navigation.
    ui.shell().set_view("folders".into());
    ui.library().set_group_grid(
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
    ui.preferences().set_folder_grid(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-list-smoke");
    ui.preferences().set_folder_grid(true);
    let saved_groups = ui.library().get_group_grid();
    ui.library().set_search("missing".into());
    ui.library()
        .set_group_grid(Rc::new(VecModel::<slint::ModelRc<CatalogGroup>>::default()).into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "folders-search-empty-smoke");
    ui.library().set_search("".into());
    ui.library().set_group_grid(saved_groups);

    ui.library().set_detail_key("C:/Music".into());
    ui.library().set_detail_secondary("".into());
    ui.library().set_detail_title("Music".into());
    ui.library().set_detail_cover_missing(true);
    ui.library()
        .set_detail_summary("3 songs, including subfolders".into());
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
    ui.library().set_detail_key("".into());
    ui.preferences().set_scale(1.2);
    ui.shell().set_view("songs".into());
    ui.player().set_now(Song {
        title: "hours".into(),
        artist: "again&again".into(),
        album: "hours".into(),
        quality: "FLAC | 44.1 KHZ | 905 KBPS".into(),
        ..Default::default()
    });
    ui.preferences().set_blurred_background(true);
    let gradient = image::RgbaImage::from_fn(1280, 820, |x, _| {
        image::Rgba([(x * 200 / 1279) as u8, 30, 80, 255])
    });
    ui.player()
        .set_backdrop(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
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
        ui.shell().set_view(view.into());
        ui.player().set_now(Song {
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

pub(super) fn verify_optional_sidebar(case: &mut UiCase) {
    let saved = controllers::preferences::settings_from_ui(&case.ui);
    case.ui.shell().set_view("songs".into());
    case.ui.player().set_full_player(false);
    case.ui.library().set_detail_key("".into());
    case.ui.library().set_search("".into());
    case.ui.preferences().set_sidebar_composers(true);
    case.ui.preferences().set_sidebar_years(true);
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(&case.window, 28.0, 348.0);
    assert_eq!(case.ui.shell().get_view(), "composers");
    click(&case.window, 28.0, 392.0);
    assert_eq!(case.ui.shell().get_view(), "years");
    case.ui.preferences().set_sidebar_songs(false);
    case.ui.preferences().set_sidebar_artists(false);
    case.ui.preferences().set_sidebar_albums(false);
    case.ui.preferences().set_sidebar_genres(false);
    case.ui.preferences().set_sidebar_playlists(false);
    case.ui.preferences().set_sidebar_folders(false);
    case.ui.preferences().set_sidebar_composers(false);
    case.ui.preferences().set_sidebar_years(false);
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(&case.window, 28.0, 84.0);
    assert_eq!(
        case.ui.shell().get_view(),
        "settings",
        "Settings must remain accessible when every optional shortcut is hidden"
    );
    controllers::preferences::apply_settings(&case.ui, saved);
    case.ui.shell().set_view("songs".into());
}

#[test]
fn wide_settings_categories_stay_at_the_left_edge() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_blurred_background(false);
    ui.shell().set_view("settings".into());
    ui.show().unwrap();
    for dpr in [1.0_f32, 1.25, 1.5, 2.0] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
        ui.shell().set_display_dpr(dpr);
        let width = (1280.0 * dpr) as usize;
        let height = (820.0 * dpr) as usize;
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        let mut pixels = vec![slint::Rgb8Pixel::default(); width * height];
        // Select Appearance, then Interface, from the fixed left category column.
        click(&window, 110.0, 104.0);
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, width);
        });
        let index = (140.0 * dpr) as usize * width + (84.0 * dpr) as usize;
        let before = pixels[index];
        click(&window, 110.0, 148.0);
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, width);
        });
        assert_ne!(
            pixels[index], before,
            "Interface category must be selectable at the left edge at {dpr}"
        );
    }
    ui.hide().unwrap();
}
