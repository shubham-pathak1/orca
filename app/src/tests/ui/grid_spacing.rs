use super::*;

#[test]
fn partial_grid_pixels_match_full_rendering_with_an_artwork_background() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    let reference_window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    struct Windows(RefCell<Vec<Rc<MinimalSoftwareWindow>>>);
    impl Platform for Windows {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.borrow_mut().pop().unwrap())
        }
    }
    slint::platform::set_platform(Box::new(Windows(RefCell::new(vec![
        reference_window.clone(),
        window.clone(),
    ]))))
    .unwrap();
    let ui = OrcaWindow::new().unwrap();
    let reference_ui = OrcaWindow::new().unwrap();
    let backdrop = slint::Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
        &[30_u8, 15, 40, 70, 25, 10, 10, 50, 35, 15, 20, 60],
        2,
        2,
    ));
    for ui in [&ui, &reference_ui] {
        models::install_grid_window(ui);
        ui.preferences().set_icon_sidebar(true);
        ui.preferences().set_grid(true);
        ui.preferences().set_browse_artwork(true);
        ui.preferences().set_blurred_background(false);
        ui.player().set_backdrop(backdrop.clone());
        ui.preferences().set_blurred_background(true);
        ui.library().set_columns(6);
        ui.show().unwrap();
    }

    for page in ["songs", "albums", "artists"] {
        for ui in [&ui, &reference_ui] {
            ui.shell().set_view(page.into());
            ui.library().set_group_columns(6);
        }
        for dpr in [1.0_f32, 1.25, 1.5, 2.0] {
            let width = (1280.0 * dpr) as usize;
            let height = (820.0 * dpr) as usize;
            for (window, ui) in [(&window, &ui), (&reference_window, &reference_ui)] {
                window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
                ui.shell().set_display_dpr(dpr);
                window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
            }
            let logical_edge = match page {
                "artists" => 144.0,
                "albums" => (1184.0 - 44.0 - 60.0) / 6.0,
                _ => (1184.0 - 44.0 - 80.0) / 6.0,
            };
            let edge = (logical_edge * dpr).floor() as u32;
            let rows: Vec<slint::ModelRc<Song>> = (0..16)
                .map(|row| {
                    let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(edge, edge);
                    for (index, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
                        let x = index as u32 % edge;
                        let y = index as u32 / edge;
                        *pixel = slint::Rgba8Pixel {
                            r: 40 + row * 11,
                            g: (x * 7) as u8,
                            b: (y * 13) as u8,
                            a: if (x < 4 && y < 4)
                                || (row == 3 && x > edge / 3 && x < edge * 2 / 3)
                                || (page == "artists" && {
                                    let radius = edge as f32 / 2.0;
                                    (x as f32 + 0.5 - radius).powi(2)
                                        + (y as f32 + 0.5 - radius).powi(2)
                                        > radius.powi(2)
                                })
                            {
                                0
                            } else {
                                255
                            },
                        };
                    }
                    let cover = slint::Image::from_rgba8(pixels);
                    Rc::new(VecModel::from(
                        (0..6)
                            .map(|column| Song {
                                path: format!("retained-{row}-{column}").into(),
                                title: format!("Track {row}-{column}").into(),
                                artist: "Artist".into(),
                                cover: cover.clone(),
                                ..Default::default()
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into()
                })
                .collect();
            if page == "songs" {
                let source: slint::ModelRc<_> = Rc::new(VecModel::from(rows)).into();
                ui.library().set_song_grid(source.clone());
                reference_ui.library().set_song_grid(source);
            } else {
                let groups: Vec<slint::ModelRc<CatalogGroup>> = rows
                    .iter()
                    .map(|row| {
                        Rc::new(VecModel::from(
                            (0..row.row_count())
                                .map(|index| {
                                    let song = row.row_data(index).unwrap();
                                    CatalogGroup {
                                        key: song.path,
                                        title: song.title,
                                        cover: song.cover,
                                        count: 2,
                                        ..Default::default()
                                    }
                                })
                                .collect::<Vec<_>>(),
                        ))
                        .into()
                    })
                    .collect();
                let source: slint::ModelRc<_> = Rc::new(VecModel::from(groups)).into();
                ui.library().set_group_grid(source.clone());
                reference_ui.library().set_group_grid(source);
            }
            // Repeating timers coalesce missed ticks; advance all eight crossfade steps.
            let mut warm = vec![slint::Rgb8Pixel::default(); width * height];
            for window in [&window, &reference_window] {
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut warm, width);
                });
            }
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(20));
                slint::platform::update_timers_and_animations();
            }
            let mut pixels = vec![slint::Rgb8Pixel::default(); width * height];
            let mut reference = pixels.clone();
            for (frame, delta_y) in [
                0.0, 0.0, 0.0, -13.0, -19.0, 11.0, -360.0, -260.0, 26.0, 310.0, -6.0, 4.0,
            ]
            .into_iter()
            .enumerate()
            {
                if page == "songs" && frame == 2 {
                    let model = ui.library().get_song_grid().row_data(0).unwrap();
                    let rows = model.as_any().downcast_ref::<VecModel<Song>>().unwrap();
                    let mut song = rows.row_data(0).unwrap();
                    song.cover =
                        slint::Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
                            &[230u8, 120, 40].repeat(edge as usize * edge as usize),
                            edge,
                            edge,
                        ));
                    rows.set_row_data(0, song);
                }
                for window in [&window, &reference_window] {
                    window.dispatch_event(WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(100.0, 150.0),
                        delta_x: 0.0,
                        delta_y,
                    });
                }
                slint::platform::update_timers_and_animations();
                ui.window().request_redraw();
                window.draw_if_needed(|renderer| {
                    if frame == 0 {
                        renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
                    }
                    renderer.render(&mut pixels, width);
                    if frame == 0 {
                        renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
                    }
                });
                reference_ui.window().request_redraw();
                reference_window.draw_if_needed(|renderer| {
                    renderer.render(&mut reference, width);
                });
                let mismatch = pixels.iter().zip(&reference).position(|(a, b)| a != b);
                assert!(
                mismatch.is_none(),
                "grid differs from full rendering: page={page} dpr={dpr} frame={frame} pixel={:?}",
                mismatch.map(|index| (
                    index % width,
                    index / width,
                    pixels[index],
                    reference[index]
                ))
            );
            }
        }
    }
}

#[test]
fn fractional_scaling_keeps_grid_row_spacing_constant_while_scrolling() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    models::install_grid_window(&ui);
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_grid(true);
    ui.preferences().set_browse_artwork(true);
    ui.preferences().set_blurred_background(false);
    ui.library().set_columns(6);
    let rows: Vec<slint::ModelRc<Song>> = (0..12)
        .map(|row| {
            let cover = slint::Image::from_rgb8(slint::SharedPixelBuffer::clone_from_slice(
                &[40 + row as u8 * 15, 50, 160],
                1,
                1,
            ));
            Rc::new(VecModel::from(
                (0..6)
                    .map(|column| Song {
                        path: format!("spacing-{row}-{column}").into(),
                        cover: cover.clone(),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
            ))
            .into()
        })
        .collect();
    ui.library()
        .set_song_grid(Rc::new(VecModel::from(rows)).into());
    ui.show().unwrap();
    for dpr in [1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
        ui.shell().set_display_dpr(dpr);
        let width = (1280.0 * dpr) as usize;
        let height = (820.0 * dpr) as usize;
        window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
        let mut pixels = vec![slint::Rgb8Pixel::default(); width * height];
        let x = (90.0 * dpr) as usize;
        let mut reference = None;
        for _ in 0..32 {
            window.dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(100.0, 150.0),
                delta_x: 0.0,
                delta_y: -0.2,
            });
            window.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, width);
            });
            let top = |red| {
                (0..height)
                    .find(|&y| {
                        let pixel = pixels[y * width + x];
                        pixel.r == red && pixel.g == 50 && pixel.b == 160
                    })
                    .expect("the next two artwork rows must be visible")
            };
            let gap = top(70) - top(55);
            if let Some(reference) = reference {
                assert_eq!(
                    gap, reference,
                    "neighboring covers changed spacing at dpr={dpr}"
                );
            } else {
                reference = Some(gap);
            }
        }
    }
    ui.hide().unwrap();
}

#[test]
fn fractional_scaling_keeps_catalog_row_spacing_constant_while_scrolling() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = OrcaWindow::new().unwrap();
    models::install_grid_window(&ui);
    ui.preferences().set_icon_sidebar(true);
    ui.preferences().set_grid(true);
    ui.preferences().set_browse_artwork(true);
    ui.preferences().set_blurred_background(false);
    ui.library().set_columns(6);
    ui.library().set_group_columns(6);
    ui.show().unwrap();
    for page in ["albums", "artists"] {
        ui.shell().set_view(page.into());
        for dpr in [1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: dpr });
            ui.shell().set_display_dpr(dpr);
            let width = (1280.0 * dpr) as usize;
            let height = (820.0 * dpr) as usize;
            window.set_size(slint::PhysicalSize::new(width as u32, height as u32));
            let edge = (if page == "artists" { 144.0_f32 } else { 180.0 } * dpr).floor() as u32;
            let textured: Vec<slint::ModelRc<CatalogGroup>> = (0..12)
                .map(|row| {
                    let mut buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(edge, edge);
                    for (index, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
                        *pixel = slint::Rgb8Pixel {
                            r: 40 + row as u8 * 15,
                            g: 50,
                            b: (index as u32 / edge * 13) as u8,
                        };
                    }
                    let cover = slint::Image::from_rgb8(buffer);
                    Rc::new(VecModel::from(
                        (0..6)
                            .map(|column| CatalogGroup {
                                key: format!("texture-{row}-{column}").into(),
                                cover: cover.clone(),
                                ..Default::default()
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into()
                })
                .collect();
            ui.library()
                .set_group_grid(Rc::new(VecModel::from(textured)).into());
            let mut pixels = vec![slint::Rgb8Pixel::default(); width * height];
            let x = (170.0 * dpr) as usize;
            let mut reference = None;
            let mut texture_reference = None;
            for _ in 0..32 {
                window.dispatch_event(WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(100.0, 150.0),
                    delta_x: 0.0,
                    delta_y: -0.2,
                });
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, width);
                });
                let top = |red| {
                    (0..height)
                        .find(|&y| {
                            let pixel = pixels[y * width + x];
                            pixel.r == red && pixel.g == 50
                        })
                        .expect("the next two artwork rows must be visible")
                };
                let row_top = top(55);
                let texture: Vec<_> = (10..40)
                    .map(|dy| pixels[(row_top + dy) * width + x])
                    .collect();
                if let Some(expected) = &texture_reference {
                    assert_eq!(
                        &texture, expected,
                        "cover pixels shimmer on {page} at dpr={dpr}"
                    );
                } else {
                    texture_reference = Some(texture);
                }
                let gap = top(70) - row_top;
                if let Some(reference) = reference {
                    assert_eq!(
                        gap, reference,
                        "neighboring covers changed spacing on {page} at dpr={dpr}"
                    );
                } else {
                    reference = Some(gap);
                }
            }
        }
    }
    ui.hide().unwrap();
}
