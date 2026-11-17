use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let pixels = &mut case.pixels;
    let played = &case.played;
    let shortcuts = &case.shortcuts;
    let observed = shortcuts.clone();
    ui.shell().on_keyboard(move |key, _| {
        if key == "m" {
            observed.set(observed.get() + 1);
            true
        } else {
            false
        }
    });
    click(window, 800.0, 40.0);
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(shortcuts.get(), 0, "typing in search must not trigger mute");
    assert!(ui.library().get_search().contains('m'));
    click(window, 600.0, 680.0);
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(
        shortcuts.get(),
        1,
        "empty-space click must release search focus"
    );
    ui.library().set_search("".into());
    let custom =
        register_font(include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf").to_vec())
            .unwrap();
    assert_eq!(custom, "Plus Jakarta Sans");
    assert!(register_font(vec![0, 1, 2, 3]).is_err());
    screenshot(pixels, "library-smoke");
    click(window, 100.0, 120.0);
    assert_eq!(played.get(), 1, "library click should play one song");
    ui.player().set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 100.0, 120.0);
    assert_eq!(
        played.get(),
        1,
        "full player must block underlying library input"
    );
    ui.lyrics().set_lyrics_open(true);
    ui.lyrics().set_lyrics(
        Rc::new(VecModel::from(crate::lyrics::parse(
            "[00:01.00]First line\n[00:03.00]Second line",
        )))
        .into(),
    );
    ui.lyrics().set_active_line(1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "lyrics-smoke");
    let long_line = "A long example lyric that must wrap naturally inside the available lyric column instead of being cropped at either edge";
    ui.lyrics().set_lyrics(
        Rc::new(VecModel::from(crate::lyrics::parse(&format!(
            "[00:01.00]{long_line}"
        ))))
        .into(),
    );
    ui.lyrics().set_active_line(0);
    ui.lyrics()
        .on_lyric_offset(|index, _| index.max(0) as f32 * 240.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let bands: Vec<_> = (104..350)
        .filter(|&y| {
            (550..1220).any(|x| {
                let p = pixels[y * 1280 + x];
                p.r > 220 && p.g > 220 && p.b > 220
            })
        })
        .collect();
    let starts = bands
        .iter()
        .enumerate()
        .filter(|(index, y)| *index == 0 || **y > bands[*index - 1] + 1)
        .count();
    assert!(
        starts >= 2,
        "long lyrics must wrap onto multiple text lines"
    );
    screenshot(pixels, "wrapped-lyrics-smoke");
    ui.preferences().set_lyrics_left_aligned(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "left-aligned-lyrics-full-player");
    // Check the width cap in both alignments at compact and desktop widths.
    for (left_aligned, width, height) in [
        (false, 1000, 820),
        (false, 1100, 820),
        (false, 1920, 1080),
        (true, 1000, 820),
        (true, 1100, 820),
        (true, 1920, 1080),
    ] {
        ui.preferences().set_lyrics_left_aligned(left_aligned);
        window.set_size(slint::PhysicalSize::new(width, height));
        let mut frame = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut frame, width as usize);
        });
        assert!(ui.lyrics().get_lyrics_width() <= 760.0);
        assert!(ui.lyrics().get_lyrics_width() > 400.0);
        let bytes: Vec<u8> = frame.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        save_screenshot(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../target/slint/test-artifacts/ui/lyrics-left-{left_aligned}-{width}.png"
            )),
            &bytes,
            width,
            height,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
    window.set_size(slint::PhysicalSize::new(1280, 820));
    let previous_track = ui.player().get_now();
    let mut preview_track = previous_track.clone();
    preview_track.path = "test.flac".into();
    ui.player().set_now(preview_track);
    ui.player().set_full_player(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "left-aligned-lyrics-side-panel");
    ui.player().set_full_player(true);
    ui.player().set_now(previous_track);
    ui.preferences().set_lyrics_left_aligned(false);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let enhanced = crate::lyrics::parse("[00:01.00]Lovely <00:01.50>day\n[00:03.00]Next line");
    let rendered = Rc::new(RefCell::new(lyric_render::Renderer::new()));
    rendered.borrow_mut().set_romanization(vec![
        "Latin pronunciation follows this line".into(),
        String::new(),
    ]);
    rendered.borrow_mut().configure(
        lyric_render::Config {
            blur_strength: 0,
            left_aligned: false,
            width: ui.lyrics().get_lyrics_width().round() as u32,
            size: ui.lyrics().get_lyric_font_size().round() as u32,
            dpr: 1.0,
            family: "Plus Jakarta Sans".into(),
            font_path: String::new(),
        },
        &enhanced,
    );
    let observer = rendered.clone();
    ui.lyrics()
        .on_lyric_render(move |index, _| observer.borrow_mut().get_signed(index));
    let observer = rendered.clone();
    ui.lyrics()
        .on_lyric_offset(move |index, _| observer.borrow().offset(index.max(0) as usize));
    ui.lyrics()
        .set_lyrics(Rc::new(VecModel::from(enhanced)).into());
    ui.lyrics().set_active_line(0);
    ui.player().set_position(1100.0);
    ui.lyrics().set_lyric_position(1100.0);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if rendered.borrow_mut().drain() {
            ui.lyrics()
                .set_lyric_render_revision(ui.lyrics().get_lyric_render_revision() + 1);
            ui.lyrics()
                .set_lyric_layout_revision(ui.lyrics().get_lyric_layout_revision() + 1);
        }
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        if rendered.borrow_mut().get(0).bitmap.size().width > 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "enhanced lyric rendering did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let row = rendered.borrow_mut().get(0);
    let captions: Vec<_> = (0..row.spans.row_count())
        .filter_map(|i| row.spans.row_data(i))
        .filter(|span| span.caption)
        .collect();
    assert!(!captions.is_empty());
    let caption_top = captions.iter().map(|s| s.y).fold(f32::INFINITY, f32::min);
    let caption_bottom = captions.iter().map(|s| s.y + s.height).fold(0.0, f32::max);
    let bright_caption_pixels = |pixels: &[slint::Rgb8Pixel]| {
        ((104.0 + caption_top).ceil() as usize + 2..(104.0 + caption_bottom).floor() as usize)
            .flat_map(|y| (550..1220).map(move |x| y * 1280 + x))
            .filter(|&i| pixels[i].r > 220 && pixels[i].g > 220 && pixels[i].b > 220)
            .count()
    };
    let early_caption = bright_caption_pixels(pixels);
    screenshot(pixels, "enhanced-lyrics-smoke");
    let before = pixels.clone();
    ui.player().set_position(2000.0);
    ui.lyrics().set_lyric_position(2000.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let difference = pixels
        .iter()
        .zip(&before)
        .filter(|(a, b)| a.r as u16 > b.r as u16 + 10)
        .count();
    assert!(
        difference > 100,
        "word highlight should brighten cached glyphs as playback advances"
    );
    assert!(
        bright_caption_pixels(pixels) > early_caption + 100,
        "pronunciation must sweep with the enhanced original rather than start fully bright"
    );
    ui.lyrics().set_lyric_position(900.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        bright_caption_pixels(pixels) < early_caption,
        "seeking backward must rewind pronunciation highlighting immediately"
    );
    ui.lyrics().set_lyric_position(2000.0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "enhanced-lyrics-complete-smoke");
    // Cache pressure can evict a row while the UI is still displaying it.
    // Revisions for other completed rows must not erase/re-request that bitmap.
    let available = Rc::new(Cell::new(true));
    let requests = Rc::new(Cell::new(0));
    let observed_available = available.clone();
    let observed_requests = requests.clone();
    ui.lyrics()
        .on_lyric_offset(|index, _| index.max(0) as f32 * 100.0);
    ui.lyrics().on_lyric_render(move |index, _| {
        if index < 0 {
            return LyricRender::default();
        }
        observed_requests.set(observed_requests.get() + 1);
        if !observed_available.get() {
            return LyricRender::default();
        }
        let image = slint::Image::from_rgba8(
            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &[255u8; 4].repeat(32 * 100),
                32,
                100,
            ),
        );
        LyricRender {
            bitmap: image.clone(),
            blurred: image,
            height: 100.0,
            ink_top: 10.0,
            ink_bottom: 90.0,
            ..Default::default()
        }
    });
    ui.preferences().set_blur_unfocused_lyrics(true);
    ui.lyrics().set_lyrics(
        Rc::new(VecModel::from(
            (0..200)
                .map(|i| LyricLine {
                    text: format!("Cached lyric {i}").into(),
                    time: i as f32 * 1000.0,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    ui.lyrics().set_active_line(8);
    ui.lyrics()
        .set_lyric_layout_revision(ui.lyrics().get_lyric_layout_revision() + 1);
    ui.lyrics()
        .set_lyric_render_revision(ui.lyrics().get_lyric_render_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[130 * 1280 + 700].r > pixels[106 * 1280 + 700].r + 15,
        "partially visible edge rows must fade toward the boundary without leaving a blank band"
    );
    assert!(
        pixels[760 * 1280 + 700].r > pixels[786 * 1280 + 700].r + 15,
        "the bottom boundary must fade cached glyphs too"
    );
    let before_eviction = pixels.clone();
    let before_requests = requests.get();
    assert!(before_requests > 0 && before_requests < 20);
    available.set(false);
    for _ in 0..12 {
        ui.lyrics()
            .set_lyric_render_revision(ui.lyrics().get_lyric_render_revision() + 1);
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(
            pixels
                .iter()
                .zip(&before_eviction)
                .enumerate()
                .filter(|(i, _)| i % 1280 >= 550)
                .all(|(_, (a, b))| a == b),
            "cache eviction must not make visible lyrics flicker"
        );
    }
    assert_eq!(
        requests.get(),
        before_requests,
        "displayed rows must not repeatedly request evicted cache entries"
    );
    // A new song/layout must release old images and request the new rows.
    available.set(true);
    ui.lyrics()
        .set_lyric_layout_revision(ui.lyrics().get_lyric_layout_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(requests.get() > before_requests);
    ui.preferences().set_blur_unfocused_lyrics(false);
    ui.lyrics().set_active_line(0);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[118 * 1280 + 700].r > 220,
        "a fully visible first line must retain its brightness even near the viewport edge"
    );
    // A lyric taller than the viewport must not destabilize automatic following.
    // Render requests remain confined to rows near the visible viewport.
    let requested = Rc::new(RefCell::new(std::collections::HashSet::new()));
    let observed = requested.clone();
    ui.lyrics().on_lyric_offset(|index, _| {
        if index <= 0 {
            0.0
        } else if index == 1 {
            84.0
        } else {
            984.0 + (index - 2) as f32 * 84.0
        }
    });
    ui.lyrics().on_lyric_render(move |index, _| {
        if index < 0 {
            return LyricRender::default();
        }
        observed.borrow_mut().insert(index);
        let height = if index == 1 { 900 } else { 84 };
        let color = [255, 255, 255, if index == 2 { 255 } else { 0 }];
        LyricRender {
            bitmap: slint::Image::from_rgba8(
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &color.repeat(640 * height),
                    640,
                    height as u32,
                ),
            ),
            height: height as f32,
            ..Default::default()
        }
    });
    ui.lyrics().set_lyrics(
        Rc::new(VecModel::from(
            (0..200)
                .map(|i| LyricLine {
                    text: format!("Line {i}").into(),
                    time: i as f32 * 1000.0,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    ui.lyrics().set_active_line(1);
    ui.lyrics()
        .set_lyric_render_revision(ui.lyrics().get_lyric_render_revision() + 1);
    ui.lyrics()
        .set_lyric_layout_revision(ui.lyrics().get_lyric_layout_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    ui.lyrics().set_active_line(2);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[420 * 1280 + 700].b > 150,
        "following must immediately advance past the oversized lyric row"
    );
    assert!(
        pixels[420 * 1280 + 1160].b > 150,
        "lyrics viewport must have no scrollbar overlay"
    );
    assert!(
        requested.borrow().len() < 20,
        "offscreen lyrics must not retain rasterized rows"
    );
    let old_dynamic_accent = ui.preferences().get_dynamic_accent();
    ui.preferences().set_dynamic_accent(false);
    let marker_top = |pixels: &[slint::Rgb8Pixel]| {
        (104..684)
            .find(|&y| {
                let p = pixels[y * 1280 + 700];
                p.r > 60 && p.g > 60 && p.b > 60 && p.r.abs_diff(p.b) < 4
            })
            .expect("the white alpha-mask marker row must remain visible")
    };
    slint::platform::update_timers_and_animations();
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(700.0, 420.0),
        delta_x: 0.0,
        delta_y: -100.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let manual_top = marker_top(pixels);
    ui.lyrics().set_active_line(3);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_eq!(
        marker_top(pixels),
        manual_top,
        "manual scrolling must pause lyric auto-follow"
    );
    std::thread::sleep(Duration::from_millis(5200));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_ne!(
        marker_top(pixels),
        manual_top,
        "auto-follow must resume after manual scrolling becomes idle"
    );
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(700.0, 420.0),
        delta_x: 0.0,
        delta_y: -100.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let manual_top = marker_top(pixels);
    let seek = Rc::new(Cell::new(-1.0));
    let observed = seek.clone();
    ui.player().on_seek(move |position| observed.set(position));
    click(window, 700.0, (manual_top + 20) as f32);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_eq!(
        seek.get(),
        2000.0,
        "a scrolled lyric must still seek to its own timestamp"
    );
    ui.lyrics().set_active_line(4);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_ne!(
        marker_top(pixels),
        manual_top,
        "clicking a lyric must restore auto-follow"
    );
    ui.preferences().set_dynamic_accent(old_dynamic_accent);
}
