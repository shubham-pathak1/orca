use super::*;
fn config(width: u32) -> Config {
    Config {
        width,
        size: 40,
        dpr: 1.0,
        family: "Plus Jakarta Sans".into(),
        font_path: String::new(),
    }
}
fn fonts() -> FontContext {
    let mut fonts = FontContext::new();
    fonts.collection.register_fonts(
        parley::fontique::Blob::new(Arc::new(
            include_bytes!("../../../ui/assets/fonts/PlusJakartaSans-Bold.ttf").to_vec(),
        )),
        None,
    );
    fonts
}
#[test]
fn cached_glyphs_and_highlight_rectangles_share_the_same_coordinates() {
    let line = Line {
        text: "Lovely day".into(),
        words: vec![
            Word {
                start: 0,
                end: 7,
                time: 1000.0,
                end_time: 1500.0,
            },
            Word {
                start: 7,
                end: 10,
                time: 1500.0,
                end_time: 2000.0,
            },
        ],
    };
    let prepared = shape(&mut fonts(), &mut LayoutContext::new(), line, &config(600));
    let pixels = raster(
        &prepared.layout,
        600,
        prepared.height,
        &mut swash::scale::ScaleContext::new(),
    );
    assert_eq!(prepared.spans.len(), 2);
    assert!(prepared.spans[0].x > 100.0, "paragraph should be centered");
    for span in &prepared.spans {
        let count = (span.y.max(0.0) as usize
            ..(span.y + span.height).ceil().min(prepared.height as f32) as usize)
            .flat_map(|y| {
                (span.x.max(0.0) as usize..(span.x + span.width).ceil().min(600.0) as usize)
                    .map(move |x| (y * 600 + x) * 4 + 3)
            })
            .filter(|index| pixels[*index] > 0)
            .count();
        assert!(
            count > 30,
            "highlight rectangle must cover its word's glyphs"
        );
    }
    assert_eq!(prepared.spans[0].time, 1000.0);
    assert_eq!(prepared.spans[1].end_time, 2000.0);
}
#[test]
fn bitmap_completion_does_not_invalidate_lyric_layout() {
    let mut renderer = Renderer::new();
    let lines = crate::lyrics::parse("[00:01.00]First line\n[00:02.00]Second line");
    renderer.configure(config(320), &lines);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while renderer.heights.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        renderer.drain();
        thread::sleep(Duration::from_millis(5));
    }
    let revision = renderer.layout_revision();
    renderer.get(0);
    while renderer.get(0).bitmap.size().width == 0 {
        assert!(std::time::Instant::now() < deadline);
        renderer.drain();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(renderer.layout_revision(), revision);
    renderer.get(1);
    while renderer.get(1).bitmap.size().width == 0 {
        assert!(std::time::Instant::now() < deadline);
        renderer.drain();
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(renderer.layout_revision(), revision);
}
#[test]
fn long_plain_lyrics_are_cached_with_matching_wrapped_row_offsets() {
    let mut renderer = Renderer::new();
    let lines = crate::lyrics::parse("[00:01.00]A long plain lyric with enough words to wrap repeatedly inside a narrow column without clipping or reshaping on every animation frame
[00:03.00]Next line");
    renderer.configure(config(320), &lines);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        renderer.drain();
        let row = renderer.get_signed(0);
        assert_eq!(renderer.get_signed(-1).bitmap.size().width, 0);
        if row.bitmap.size().width > 0 {
            assert_eq!(row.bitmap.size().width, 320);
            assert!(row.height > 160.0);
            assert_eq!(row.spans.row_count(), 0);
            assert_eq!(renderer.offset(1), row.height);
            assert_eq!(row.bitmap.size().height as f32, row.height);
            let clock = renderer.clock;
            for _ in 0..60 {
                renderer.get(0);
            }
            assert_eq!(renderer.entries.len(), 1);
            assert!(renderer.pending.is_empty());
            assert_eq!(renderer.clock, clock + 60);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    renderer.invalidate();
    assert_eq!(renderer.offset(2), 168.0);
}
#[test]
fn wrapped_word_rectangles_progress_in_sequence() {
    let line = Line {
        text: "extraordinary".into(),
        words: vec![Word {
            start: 0,
            end: 13,
            time: 1000.0,
            end_time: 2000.0,
        }],
    };
    let prepared = shape(&mut fonts(), &mut LayoutContext::new(), line, &config(110));
    assert!(prepared.spans.len() > 1);
    assert!(prepared.height > 84);
    assert_eq!(prepared.spans.first().unwrap().time, 1000.0);
    assert!((prepared.spans.last().unwrap().end_time - 2000.0).abs() < 0.1);
    for pair in prepared.spans.windows(2) {
        assert!((pair[0].end_time - pair[1].time).abs() < 0.1);
    }
}
#[test]
fn latest_layout_and_pending_rows_survive_rapid_configuration_changes() {
    let mut renderer = Renderer::new();
    let lines = crate::lyrics::parse("[00:01.00]Lovely <00:01.50>day");
    for width in 300..310 {
        renderer.configure(config(width), &lines);
        renderer.get(0);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        renderer.drain();
        let render = renderer.get(0);
        if render.bitmap.size().width > 0 {
            assert_eq!(render.bitmap.size().width, 309);
            assert_eq!(render.spans.row_count(), 2);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "latest lyric bitmap was lost"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(renderer.bytes <= BUDGET);
    assert!(renderer.pending.len() <= 12);
    let shutdown = std::time::Instant::now();
    drop(renderer);
    assert!(shutdown.elapsed() < Duration::from_secs(2));
}
