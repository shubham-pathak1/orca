use super::*;

#[test]
fn browse_width_tracks_docked_panels_at_each_display_scale() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    models::install_grid_window(&ui);
    ui.player().set_now(Song {
        path: "geometry.wav".into(),
        ..Default::default()
    });
    ui.show().unwrap();
    for dpr in [1.0, 1.25, 1.5, 2.0] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
        for icon_sidebar in [true, false] {
            ui.preferences().set_icon_sidebar(icon_sidebar);
            for (width, lyrics, queue, expected) in [
                (1000, false, false, 904.0),
                (1000, true, false, 904.0),
                (1100, true, false, 684.0),
                (1120, true, false, 704.0),
                (1280, true, false, 825.6),
                (1280, false, true, 825.6),
                (1280, true, true, 825.6),
                (1920, false, true, 1404.0),
                (1920, true, false, 1404.0),
                (1920, false, false, 1824.0),
            ] {
                window.set_size(slint::PhysicalSize::new(
                    (width as f32 * dpr) as u32,
                    (820.0 * dpr) as u32,
                ));
                ui.lyrics().set_lyrics_open(lyrics);
                ui.player().set_queue_open(queue);
                let expected = expected - if icon_sidebar { 0.0 } else { 76.0 };
                assert!((ui.get_browse_width()-expected).abs()<0.01,
                    "wrong browse width at width={width}, dpr={dpr}, icons={icon_sidebar}, lyrics={lyrics}, queue={queue}: {} vs {expected}",ui.get_browse_width());
                ui.player().set_full_player(true);
                assert!(
                    (ui.get_browse_width() - expected).abs() < 0.01,
                    "covering the library must not reflow its docked grid"
                );
                ui.player().set_full_player(false);
                assert!((ui.get_browse_width() - expected).abs() < 0.01);
            }
        }
    }
    ui.hide().unwrap();
}

#[test]
fn incomplete_catalog_rows_keep_the_same_cover_size_and_column_position() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_browse_artwork(true);
    ui.preferences().set_blurred_background(false);
    ui.library().set_group_columns(6);
    let cover = slint::Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
        &[240u8, 0, 0].repeat(144 * 144),
        144,
        144,
    ));
    let rows: Vec<slint::ModelRc<CatalogGroup>> = [6, 1]
        .into_iter()
        .enumerate()
        .map(|(row, count)| {
            Rc::new(VecModel::from(
                (0..count)
                    .map(|column| CatalogGroup {
                        key: format!("{row}-{column}").into(),
                        title: "Collection".into(),
                        cover: cover.clone(),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
            ))
            .into()
        })
        .collect();
    ui.library()
        .set_group_grid(Rc::new(VecModel::from(rows)).into());
    ui.show().unwrap();
    for page in ["albums", "artists"] {
        ui.shell().set_view(page.into());
        for dpr in [1.0_f32, 1.25, 1.5, 2.0] {
            window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
            ui.shell().set_display_dpr(dpr);
            let width = (1280.0 * dpr) as usize;
            let height = (820.0 * dpr) as usize;
            window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
            let mut pixels = vec![slint::Rgb8Pixel::default(); width * height];
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width);
            });
            let mut bands = Vec::new();
            let mut in_band = false;
            for row in pixels.chunks_exact(width) {
                let red = |p: &slint::Rgb8Pixel| p.r > 200 && p.g < 20 && p.b < 20;
                if let Some(start) = row.iter().position(red) {
                    let length = row[start..].iter().take_while(|p| red(p)).count();
                    if !in_band {
                        bands.push((start, length));
                    }
                    in_band = true;
                } else {
                    in_band = false;
                }
            }
            assert_eq!(bands.len(), 2, "expected two cover rows on {page} at {dpr}");
            assert!(
                bands[0].0.abs_diff(bands[1].0) <= 1,
                "last row shifted columns on {page} at {dpr}: {bands:?}"
            );
            assert!(
                bands[0].1.abs_diff(bands[1].1) <= 1,
                "last row enlarged covers on {page} at {dpr}: {bands:?}"
            );
        }
    }
    ui.hide().unwrap();
}

#[test]
fn full_player_return_preserves_song_anchor_until_docked_layout_is_ready() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    models::install_grid_window(&ui);
    window.set_size(slint::PhysicalSize::new(1280, 820));
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_grid(true);
    ui.preferences().set_full_artwork(false);
    ui.shell().set_view("songs".into());
    ui.player().set_now(Song {
        path: "anchor.wav".into(),
        ..Default::default()
    });
    ui.lyrics().set_lyrics_open(false);
    let songs: Vec<Song> = (0..600)
        .map(|index| Song {
            path: format!("anchor-{index}").into(),
            title: format!("Song {index}").into(),
            ..Default::default()
        })
        .collect();
    let set_columns = |columns: usize| {
        ui.library().set_columns(columns as i32);
        ui.library().set_song_grid(
            Rc::new(VecModel::from(
                songs
                    .chunks(columns)
                    .map(|row| Rc::new(VecModel::from(row.to_vec())).into())
                    .collect::<Vec<slint::ModelRc<Song>>>(),
            ))
            .into(),
        );
    };
    set_columns(((ui.get_browse_width() - 28.0) / 148.0).floor().max(1.0) as usize);
    ui.show().unwrap();
    let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 820];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    ui.library().set_jump_index(180);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision() + 1);
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    let anchor = ui.library().get_library_scroll_index();
    assert!(
        anchor >= 174,
        "initial anchor={anchor}, position={}, columns={}, width={}, restoring={}",
        ui.library().get_artwork_scroll_position(),
        ui.library().get_columns(),
        ui.get_browse_width(),
        ui.library().get_library_view_restore()
    );
    ui.player().set_full_player(true);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    ui.lyrics().set_lyrics_open(true);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    assert_eq!(ui.library().get_library_scroll_index(), anchor);
    ui.player().set_full_player(false);
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    assert!(
        ui.library().get_library_view_restore(),
        "restoration must wait for the new column layout"
    );
    assert_eq!(ui.library().get_library_scroll_index(), anchor);
    let columns = ((ui.get_browse_width() - 28.0) / 148.0).floor().max(1.0) as usize;
    set_columns(columns);
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, 1280);
    });
    assert!(!ui.library().get_library_view_restore());
    assert!((ui.library().get_library_scroll_index() - anchor).abs() < columns as i32);
    assert!(ui.library().get_artwork_scroll_position() < -1000.0);
    ui.hide().unwrap();
}
