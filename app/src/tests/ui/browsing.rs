use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let songs = &case.songs;
    // Wheel input updates the grid in the next frame, without an animation tail.
    let fixture = |color: [u8; 4]| {
        slint::Image::from_rgba8(
            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &color.repeat(100 * 100),
                100,
                100,
            ),
        )
    };
    let colored_rows: Vec<slint::ModelRc<Song>> = (0..6)
        .map(|index| {
            Rc::new(VecModel::from(vec![
                Song {
                    path: format!("scroll-{index}").into(),
                    title: format!("Row {index}").into(),
                    cover: fixture(if index == 0 {
                        [220, 0, 0, 255]
                    } else {
                        [0, 0, 220, 255]
                    }),
                    ..Default::default()
                };
                6
            ]))
            .into()
        })
        .collect();
    state.set_columns(6);
    state.set_song_grid(Rc::new(VecModel::from(colored_rows)).into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(pixels[150 * 1280 + 100].r > 150);
    let unselected = pixels.clone();
    let previous_track = state.get_now();
    state.set_now(Song {
        path: "scroll-0".into(),
        ..previous_track.clone()
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        (84..260)
            .flat_map(|y| (76..250).map(move |x| y * 1280 + x))
            .filter(|&index| pixels[index] != unselected[index])
            .count()
            > 100,
        "the loaded song must have a visible selection frame in the library grid"
    );
    screenshot(pixels, "selected-song-grid-smoke");
    state.set_now(previous_track);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let red_height = |pixels: &[slint::Rgb8Pixel]| {
        (100..600)
            .filter(|&y| {
                let pixel = pixels[y * 1280 + 100];
                pixel.r > 150 && pixel.b < 80
            })
            .count()
    };
    let initial_red = red_height(pixels);
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(100.0, 150.0),
        delta_x: 0.0,
        delta_y: -50.0,
    });
    std::thread::sleep(Duration::from_millis(35));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let intermediate_red = red_height(pixels);
    std::thread::sleep(Duration::from_millis(100));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let final_red = red_height(pixels);
    assert!(
        initial_red > intermediate_red && intermediate_red > final_red,
        "small wheel input must move through an intermediate frame before settling"
    );
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(100.0, 150.0),
        delta_x: 0.0,
        delta_y: -900.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[150 * 1280 + 100].b > 150,
        "grid must respond in the first frame after wheel input"
    );
    let settled = pixels.clone();
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(17));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
    }
    assert_eq!(
        *pixels, settled,
        "grid must not continue animating after wheel input stops"
    );
    let mut initial = vec![songs.row_data(0).unwrap()];
    initial.resize(6, Song::default());
    state.set_song_grid(
        Rc::new(VecModel::from(
            vec![Rc::new(VecModel::from(initial)).into()],
        ))
        .into(),
    );
    state.set_jump_index(0);
    state.set_jump_revision(state.get_jump_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    // Every catalog overview must respond to wheel input through the same
    // virtualized native scrolling path, including compact artist/playlist rows.
    for view in ["artists", "albums", "genres", "playlists"] {
        state.set_view(view.into());
        state.set_group_columns(6);
        state.set_group_grid(
            Rc::new(VecModel::from(
                (0..30)
                    .map(|row| {
                        Rc::new(VecModel::from(
                            (0..6)
                                .map(|cell| CatalogGroup {
                                    key: format!("{view}-{row}-{cell}").into(),
                                    title: format!("Group {row}").into(),
                                    subtitle: format!("Details {row}").into(),
                                    cover: fixture(if row < 3 {
                                        [220, 0, 0, 255]
                                    } else {
                                        [0, 0, 220, 255]
                                    }),
                                    ..Default::default()
                                })
                                .collect::<Vec<_>>(),
                        ))
                        .into()
                    })
                    .collect::<Vec<slint::ModelRc<CatalogGroup>>>(),
            ))
            .into(),
        );
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let before = pixels.clone();
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(100.0, 150.0),
            delta_x: 0.0,
            delta_y: -600.0,
        });
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert_ne!(
            *pixels, before,
            "{view} overview must respond to wheel scrolling"
        );
    }
    state.set_view("songs".into());
}
