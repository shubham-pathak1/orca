use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let played = &case.played;
    let shortcuts = &case.shortcuts;
    let observed = shortcuts.clone();
    state.on_keyboard(move |key, _| {
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
    assert!(state.get_search().contains('m'));
    click(window, 600.0, 680.0);
    window.dispatch_event(WindowEvent::KeyPressed { text: "m".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "m".into() });
    assert_eq!(
        shortcuts.get(),
        1,
        "empty-space click must release search focus"
    );
    state.set_search("".into());
    let custom =
        register_font(include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf").to_vec())
            .unwrap();
    assert_eq!(custom, "Plus Jakarta Sans");
    assert!(register_font(vec![0, 1, 2, 3]).is_err());
    screenshot(pixels, "library-smoke");
    click(window, 100.0, 120.0);
    assert_eq!(played.get(), 1, "library click should play one song");
    state.set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    click(window, 100.0, 120.0);
    assert_eq!(
        played.get(),
        1,
        "full player must block underlying library input"
    );
    state.set_lyrics_open(true);
    state.set_lyrics(
        Rc::new(VecModel::from(crate::lyrics::parse(
            "[00:01.00]First line\n[00:03.00]Second line",
        )))
        .into(),
    );
    state.set_active_line(1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    screenshot(pixels, "lyrics-smoke");
    let long_line = "A long example lyric that must wrap naturally inside the available lyric column instead of being cropped at either edge";
    state.set_lyrics(
        Rc::new(VecModel::from(crate::lyrics::parse(&format!(
            "[00:01.00]{long_line}"
        ))))
        .into(),
    );
    state.set_active_line(0);
    state.on_lyric_offset(|index, _| index.max(0) as f32 * 240.0);
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
    let enhanced = crate::lyrics::parse("[00:01.00]Lovely <00:01.50>day\n[00:03.00]Next line");
    let rendered = Rc::new(RefCell::new(lyric_render::Renderer::new()));
    rendered.borrow_mut().configure(
        lyric_render::Config {
            width: state.get_lyrics_width().round() as u32,
            size: state.get_lyric_font_size().round() as u32,
            dpr: 1.0,
            family: "Plus Jakarta Sans".into(),
            font_path: String::new(),
        },
        &enhanced,
    );
    let observer = rendered.clone();
    state.on_lyric_render(move |index, _| observer.borrow_mut().get_signed(index));
    let observer = rendered.clone();
    state.on_lyric_offset(move |index, _| observer.borrow().offset(index.max(0) as usize));
    state.set_lyrics(Rc::new(VecModel::from(enhanced)).into());
    state.set_active_line(0);
    state.set_position(1100.0);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if rendered.borrow_mut().drain() {
            state.set_lyric_render_revision(state.get_lyric_render_revision() + 1);
            state.set_lyric_layout_revision(state.get_lyric_layout_revision() + 1);
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
    screenshot(pixels, "enhanced-lyrics-smoke");
    let before = pixels.clone();
    state.set_position(2000.0);
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
    screenshot(pixels, "enhanced-lyrics-complete-smoke");
    // A lyric taller than the viewport must not destabilize automatic following.
    // Render requests remain confined to rows near the visible viewport.
    let requested = Rc::new(RefCell::new(std::collections::HashSet::new()));
    let observed = requested.clone();
    state.on_lyric_offset(|index, _| {
        if index <= 0 {
            0.0
        } else if index == 1 {
            84.0
        } else {
            984.0 + (index - 2) as f32 * 84.0
        }
    });
    state.on_lyric_render(move |index, _| {
        if index < 0 {
            return LyricRender::default();
        }
        observed.borrow_mut().insert(index);
        let height = if index == 1 { 900 } else { 84 };
        let color = if index == 2 {
            [0, 0, 220, 255]
        } else {
            [220, 0, 0, 255]
        };
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
    state.set_lyrics(
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
    state.set_active_line(1);
    state.set_lyric_render_revision(state.get_lyric_render_revision() + 1);
    state.set_lyric_layout_revision(state.get_lyric_layout_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    state.set_active_line(2);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[350 * 1280 + 700].b > 150,
        "following must immediately advance past the oversized lyric row"
    );
    assert!(
        pixels[350 * 1280 + 1215].b > 150,
        "lyrics viewport must have no scrollbar overlay"
    );
    assert!(
        requested.borrow().len() < 20,
        "offscreen lyrics must not retain rasterized rows"
    );
}
