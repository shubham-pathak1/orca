use super::*;
fn config(width: u32) -> Config {
    Config {
        blur_strength: 0,
        left_aligned: false,
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
fn romanization_sits_below_original_and_inherits_its_sweep_timing() {
    let original = "Original lyric";
    let mut settings = config(600);
    settings.left_aligned = true;
    let prepared = shape(
        &mut fonts(),
        &mut LayoutContext::new(),
        Line {
            text: original.into(),
            secondary: "Latin pronunciation".into(),
            words: vec![Word {
                start: 0,
                end: original.len(),
                time: 1000.0,
                end_time: 2000.0,
            }],
        },
        &settings,
    );
    let rows: Vec<_> = prepared.layout.lines().collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[1].metrics().baseline > rows[0].metrics().baseline);
    assert!(rows[1]
        .runs()
        .all(|run| (run.font_size() - 23.2).abs() < 0.1));
    assert_eq!(prepared.spans.len(), 2);
    assert!(!prepared.spans[0].caption);
    assert!(prepared.spans[1].caption);
    assert_eq!(prepared.spans[1].time, 1000.0);
    assert_eq!(prepared.spans[1].end_time, 2000.0);
    assert_eq!(prepared.spans[0].time, 1000.0);
    assert_eq!(prepared.spans[0].end_time, 2000.0);
    assert!(prepared.spans[1].y > prepared.spans[0].y);
    assert!(prepared.spans[0].y + prepared.spans[0].height < rows[1].metrics().baseline);
    if std::env::var_os("ORCA_TEST_SCREENSHOTS").is_some() {
        let bitmap = raster(
            &prepared.layout,
            settings.width,
            prepared.height,
            &mut swash::scale::ScaleContext::new(),
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/slint/test-artifacts/ui/romanized-lyric-row.png");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pixels: Vec<u8> = bitmap
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[3]; 3])
            .collect();
        image::save_buffer(
            path,
            &pixels,
            settings.width,
            prepared.height,
            image::ColorType::Rgb8,
        )
        .unwrap();
    }
}
#[test]
fn left_alignment_keeps_wrapped_highlights_on_their_glyphs() {
    let text = "Lovely day with enough words to wrap across several lines";
    let mut settings = config(320);
    settings.left_aligned = true;
    let prepared = shape(
        &mut fonts(),
        &mut LayoutContext::new(),
        Line {
            secondary: String::new(),
            text: text.into(),
            words: vec![Word {
                start: 0,
                end: text.len(),
                time: 1000.0,
                end_time: 3000.0,
            }],
        },
        &settings,
    );
    assert!(prepared.spans.len() > 1);
    let bitmap = raster(
        &prepared.layout,
        settings.width,
        prepared.height,
        &mut swash::scale::ScaleContext::new(),
    );
    for span in &prepared.spans {
        assert!(span.x.abs() < 0.5, "wrapped rows should start at the left");
        let has_ink = (span.y.max(0.0) as usize
            ..(span.y + span.height).ceil().min(prepared.height as f32) as usize)
            .any(|y| {
                (0..span.width.ceil().min(settings.width as f32) as usize)
                    .any(|x| bitmap.bytes[(y * settings.width as usize + x) * 4 + 3] > 0)
            });
        assert!(has_ink, "highlight must cover the corresponding glyphs");
    }
    assert_eq!(prepared.spans.first().unwrap().time, 1000.0);
    assert_eq!(prepared.spans.last().unwrap().end_time, 3000.0);
}

#[test]
fn cached_glyphs_and_highlight_rectangles_share_the_same_coordinates() {
    let line = Line {
        secondary: String::new(),
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
    let rasterized = raster(
        &prepared.layout,
        600,
        prepared.height,
        &mut swash::scale::ScaleContext::new(),
    );
    let pixels = &rasterized.bytes;
    assert!(rasterized.ink_top < rasterized.ink_bottom);
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
    assert_eq!(renderer.offset(2), 136.0);
}
#[test]
fn wrapped_word_rectangles_progress_in_sequence() {
    let line = Line {
        secondary: String::new(),
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

#[test]
fn lyrics_blur_strength_softens_alpha_without_changing_sharp_source_or_ink_color() {
    let mut source = vec![0; 31 * 31 * 4];
    let center = (15 * 31 + 15) * 4;
    source[center..center + 4].copy_from_slice(&[255; 4]);
    let before = source.clone();
    let light = raster::blur_alpha(&source, 31, 31, 1.0);
    let strong = raster::blur_alpha(&source, 31, 31, 3.0);
    assert_eq!(source, before);
    assert!(light[center + 3] < 255);
    assert!(strong[center + 3] < light[center + 3]);
    assert!(light.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count() > 1);
    assert!(
        strong
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count()
            > light.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count()
    );
    assert!(strong.as_chunks::<4>().0.iter().all(|p| p[..3] == [255; 3]));
    assert_eq!(raster::blur_alpha(&source, 31, 31, 0.0), source);
}

#[test]
fn lyrics_blur_caches_both_variants_within_the_existing_memory_budget() {
    let mut renderer = Renderer::new();
    let lines = crate::lyrics::parse("[00:01.00]Original lyric");
    renderer.set_romanization(vec!["Latin pronunciation".into()]);
    let mut settings = config(320);
    settings.blur_strength = 70;
    renderer.configure(settings, &lines);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        renderer.drain();
        let row = renderer.get(0);
        if row.bitmap.size().width > 0 {
            assert_eq!(row.bitmap.size(), row.blurred.size());
            assert!(row.padding > 0.0);
            assert_eq!(row.bitmap.size().width, 320 + (row.padding * 2.0) as u32);
            assert_eq!(
                row.bitmap.size().height as f32,
                row.height + row.padding * 2.0
            );
            assert_eq!(
                renderer.bytes,
                row.bitmap.size().width as usize * row.bitmap.size().height as usize * 8
            );
            assert!(renderer.bytes <= BUDGET);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn lyrics_blur_padding_preserves_glyph_positions_and_keeps_soft_edges_in_the_canvas() {
    let mut settings = config(160);
    settings.left_aligned = true;
    let prepared = shape(
        &mut fonts(),
        &mut LayoutContext::new(),
        Line {
            text: "Left edge".into(),
            secondary: String::new(),
            words: Vec::new(),
        },
        &settings,
    );
    let sharp = raster(
        &prepared.layout,
        settings.width,
        prepared.height,
        &mut swash::scale::ScaleContext::new(),
    );
    let padding = 18;
    let padded = raster::raster_padded(
        &prepared.layout,
        settings.width,
        prepared.height,
        padding,
        &mut swash::scale::ScaleContext::new(),
    );
    assert!(
        padded
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| { pixel[3] == 0 || pixel[..3] == [255, 255, 255] }),
        "glyph masks must remain white so explicit tinting preserves their pixels"
    );
    let width = settings.width + padding * 2;
    let height = prepared.height + padding * 2;
    for y in 0..prepared.height {
        for x in 0..settings.width {
            assert_eq!(
                sharp.bytes[((y * settings.width + x) * 4 + 3) as usize],
                padded.bytes[(((y + padding) * width + x + padding) * 4 + 3) as usize]
            );
        }
    }
    let blurred = raster::blur_alpha(&padded.bytes, width, height, 6.0);
    assert!(
        (0..height).any(|y| (0..padding).any(|x| blurred[((y * width + x) * 4 + 3) as usize] > 0)),
        "soft edges must extend into the transparent gutter"
    );
    assert!(
        (0..height).all(|y| blurred[(y * width * 4 + 3) as usize] == 0),
        "blur must fade before the outer clipping edge"
    );
}

#[test]
fn romanization_projection_preserves_holds_pauses_and_wrapped_reading_order() {
    let prepared = shape(
        &mut fonts(),
        &mut LayoutContext::new(),
        Line {
            text: "ab cd".into(),
            secondary: "A longer pronunciation wraps onto several rows".into(),
            words: vec![
                Word {
                    start: 0,
                    end: 2,
                    time: 1000.0,
                    end_time: 2000.0,
                },
                Word {
                    start: 3,
                    end: 5,
                    time: 4000.0,
                    end_time: 4500.0,
                },
            ],
        },
        &config(140),
    );
    let captions: Vec<_> = prepared.spans.iter().filter(|span| span.caption).collect();
    assert!(captions.len() > 2);
    assert!(captions.last().unwrap().y > captions[0].y);
    assert_eq!(captions[0].time, 1000.0);
    assert!((captions.last().unwrap().end_time - 4500.0).abs() < 0.1);
    assert!(captions
        .windows(2)
        .all(|pair| pair[1].time >= pair[0].end_time - 0.1));
    assert!(captions
        .iter()
        .any(|span| span.time >= 2000.0 && span.end_time <= 4000.0));
    let progress = |position: f32| {
        captions
            .iter()
            .map(|span| {
                span.width
                    * ((position - span.time) / (span.end_time - span.time).max(1.0))
                        .clamp(0.0, 1.0)
            })
            .sum::<f32>()
    };
    assert_eq!(progress(900.0), 0.0);
    assert!(progress(1500.0) < progress(2000.0));
    assert!(progress(2000.0) < progress(3000.0));
    assert!(progress(3000.0) < progress(4000.0));
    assert!(progress(4250.0) < progress(4501.0));
}
#[test]
fn line_timed_romanization_has_no_synthetic_word_sweep() {
    let prepared = shape(
        &mut fonts(),
        &mut LayoutContext::new(),
        Line {
            text: "Original".into(),
            secondary: "Pronunciation".into(),
            words: vec![],
        },
        &config(600),
    );
    assert!(prepared.spans.is_empty());
}

#[test]
fn romanization_projection_counts_source_characters_instead_of_utf8_bytes() {
    let line = Line {
        text: "\u{3042}B".into(),
        secondary: "abcdef".into(),
        words: vec![
            Word {
                start: 0,
                end: 3,
                time: 1000.0,
                end_time: 2000.0,
            },
            Word {
                start: 3,
                end: 4,
                time: 3000.0,
                end_time: 4000.0,
            },
        ],
    };
    let clusters = vec![super::projection::Cluster {
        start: 0.0,
        end: 1.0,
        span: Span {
            caption: true,
            x: 0.0,
            y: 0.0,
            width: 90.0,
            height: 20.0,
            time: 0.0,
            end_time: 0.0,
            rtl: false,
        },
    }];
    let spans = super::projection::project(&line, &clusters);
    assert_eq!(spans.len(), 2);
    assert_eq!(
        (spans[0].x, spans[0].width, spans[0].time, spans[0].end_time),
        (0.0, 45.0, 1000.0, 2000.0)
    );
    assert_eq!(
        (spans[1].x, spans[1].width, spans[1].time, spans[1].end_time),
        (45.0, 45.0, 3000.0, 4000.0)
    );
}

#[test]
fn unused_renderer_shuts_down_without_waiting_for_a_render_job() {
    let (done, completed) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        drop(Renderer::new());
        done.send(()).unwrap();
    });
    completed
        .recv_timeout(Duration::from_secs(2))
        .expect("idle renderer must wake for shutdown");
    worker.join().unwrap();
}
