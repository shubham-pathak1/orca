use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
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
