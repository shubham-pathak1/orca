use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "ui-smoke");
    // Nonempty high-DPI fixtures catch intrinsic image sizing: a bitmap's
    // physical dimensions must never become the delegate's logical geometry.
    state.set_metadata_open(false);
    state.set_collection_dialog(true);
    assert!(!state.invoke_keyboard("q".into(), false));
    assert!(!state.invoke_keyboard("l".into(), false));
    assert!(!state.invoke_keyboard("focus-search".into(), false));
    assert!(state.invoke_keyboard(slint::platform::Key::Escape.into(), false));
    assert!(!state.get_collection_dialog());
    state.set_full_player(true);
    state.set_lyrics_open(false);
    state.set_blurred_background(true);
    state.set_light_theme(false);
    let solid = |width, height, rgba: [u8; 4]| {
        let bytes = rgba.repeat((width * height) as usize);
        slint::Image::from_rgba8(
            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&bytes, width, height),
        )
    };
    state.set_backdrop(solid(160, 160, [200, 0, 0, 255]));
    state.set_full_backdrop(solid(1024, 820, [200, 0, 0, 255]));
    state.set_original_cover(solid(500, 500, [0, 200, 0, 255]));
    state.set_waveform(solid(720, 48, [255, 255, 255, 255]));
    state.set_accent(slint::Color::from_rgb_u8(0, 200, 255));
    state.set_player_accent(slint::Color::from_rgb_u8(0, 200, 255));
    state.set_duration(100.0);
    state.set_position(50.0);
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.25 });
    window.set_size(slint::PhysicalSize::new(1600, 1025));
    let mut hidpi = vec![slint::Rgb8Pixel::default(); 1600 * 1025];
    for _ in 0..14 {
        std::thread::sleep(Duration::from_millis(33));
        slint::platform::update_timers_and_animations();
    }
    window.draw_if_needed(|renderer| {
        renderer.render(&mut hidpi, 1600);
    });
    for (x, y) in [(20, 20), (1580, 20), (20, 500), (1580, 1000)] {
        let pixel = hidpi[y * 1600 + x];
        assert!(
            pixel.r > pixel.g && pixel.g < 20,
            "backdrop must fill the full window at high DPI"
        );
    }
    let green: Vec<_> = hidpi
        .iter()
        .enumerate()
        .filter(|(_, p)| p.g > 150 && p.r < 20 && p.b < 20)
        .map(|(i, _)| (i % 1600, i / 1600))
        .collect();
    let cover_width =
        green.iter().map(|p| p.0).max().unwrap() - green.iter().map(|p| p.0).min().unwrap() + 1;
    let cover_center = (green.iter().map(|p| p.0).max().unwrap()
        + green.iter().map(|p| p.0).min().unwrap()) as f32
        / 2.0;
    assert!(
        (cover_center - 800.0).abs() < 2.0,
        "cover must be centered over the controls"
    );
    assert!(
        (400..=496).contains(&cover_width),
        "cover must fit its logical square, got {cover_width}"
    );
    let cyan: Vec<_> = hidpi
        .iter()
        .enumerate()
        .filter(|(_, p)| p.g > 140 && p.b > 180 && p.r < 30)
        .map(|(i, _)| i % 1600)
        .collect();
    assert!(!cyan.is_empty());
    assert!(
        cyan.iter().all(|&x| (449..=1151).contains(&x)),
        "waveform must stay inside the centered player column"
    );
    let bytes: Vec<u8> = hidpi.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    save_screenshot(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/hidpi-player-smoke.png"),
        &bytes,
        1600,
        1025,
        image::ColorType::Rgb8,
    )
    .unwrap();
    // A padded second grid cell is spacing, never an empty album card.
    window.set_size(slint::PhysicalSize::new(1920, 1080));
    state.set_full_player(false);
    state.set_blurred_background(false);
    state.set_view("artists".into());
    state.set_detail_key("Artist".into());
    state.set_related_albums(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            CatalogGroup {
                key: "One album".into(),
                title: "One album".into(),
                count: 1,
                cover_missing: true,
                ..Default::default()
            },
            CatalogGroup::default(),
        ]))
        .into()]))
        .into(),
    );
    let mut wide = vec![slint::Rgb8Pixel::default(); 1920 * 1080];
    window.draw_if_needed(|renderer| {
        renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        renderer.render(&mut wide, 1920);
    });
    let blank = wide[550 * 1920 + 1800];
    assert_eq!(
        (blank.r, blank.g, blank.b),
        (9, 10, 12),
        "padded album cells must draw neither artwork nor a card background"
    );
    // Full-player lyric updates must preserve a nonuniform backdrop in reused buffers.
    state.set_full_player(true);
    state.set_lyrics_open(true);
    state.set_blurred_background(true);
    let wash = image::RgbaImage::from_fn(1920, 1080, |x, _| {
        image::Rgba([(20 + x * 40 / 1919) as u8, 24, 28, 255])
    });
    state.set_full_backdrop(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
        slint::Rgba8Pixel,
    >::clone_from_slice(
        wash.as_raw(), 1920, 1080
    )));
    for _ in 0..14 {
        std::thread::sleep(Duration::from_millis(33));
        slint::platform::update_timers_and_animations();
    }
    window.draw_if_needed(|renderer| {
        renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
        renderer.render(&mut wide, 1920);
    });
    for line in [1, 2, 3] {
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            renderer.render(&mut wide, 1920);
        });
        state.set_active_line(line);
        window.draw_if_needed(|renderer| {
            renderer.render(&mut wide, 1920);
        });
        let reused = wide.clone();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            renderer.render(&mut wide, 1920);
        });
        assert_eq!(
            reused.iter().zip(&wide).filter(|(a, b)| a != b).count(),
            0,
            "lyric repaint must not create backdrop seams"
        );
    }
}
