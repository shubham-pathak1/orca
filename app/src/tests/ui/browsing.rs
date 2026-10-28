use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;
    let state = ui.global::<AppState>();
    let pixels = &mut case.pixels;
    let songs = &case.songs;
    // Coarse wheel steps glide; fine deltas and large jumps remain direct.
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
    // Catalog overviews share wheel handling and virtualized rows, including
    // compact artist and playlist entries.
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
        let scrolled_pixel = pixels[150 * 1280 + 100];
        let previous_scroll = state.get_catalog_scroll_y();
        assert!(
            previous_scroll < 0.0,
            "{view} should record its scroll position"
        );
        state.set_detail_key(format!("{view}-0-0").into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        state.set_detail_key("".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        state.set_catalog_scroll_target(previous_scroll);
        state.set_catalog_scroll_restore_revision(
            state.get_catalog_scroll_restore_revision().wrapping_add(1),
        );
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        std::thread::sleep(Duration::from_millis(20));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        screenshot(pixels, &format!("catalog-return-{view}"));
        assert!(
            (state.get_catalog_scroll_y() - previous_scroll).abs() < 1.0,
            "{view} should return to the previous scroll position: before {previous_scroll}, after {}",
            state.get_catalog_scroll_y()
        );
        assert_eq!(
            pixels[150 * 1280 + 100],
            scrolled_pixel,
            "{view} should show the same row after returning"
        );
    }
    let saved_songs = state.get_songs();
    let saved_grid = state.get_song_grid();
    let saved_groups = state.get_group_grid();
    state.set_view("songs".into());
    state.set_grid(true);
    state.set_library_view_restore(false);
    let library: Vec<Song> = (0..600)
        .map(|index| Song {
            path: format!("position-{index}").into(),
            title: format!("Song {index}").into(),
            ..Default::default()
        })
        .collect();
    state.set_songs(Rc::new(VecModel::from(library.clone())).into());
    state.set_song_grid(
        Rc::new(VecModel::from(
            library
                .chunks(6)
                .map(|chunk| Rc::new(VecModel::from(chunk.to_vec())).into())
                .collect::<Vec<slint::ModelRc<Song>>>(),
        ))
        .into(),
    );
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    state.set_jump_index(180);
    state.set_jump_revision(state.get_jump_revision().wrapping_add(1));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let anchor = state.get_library_scroll_index();
    screenshot(pixels, "library-anchor-before-switch");
    assert!(
        anchor >= 174,
        "grid should record a song anchor after jumping, got {anchor}"
    );
    for grid in [false, true] {
        state.set_library_view_restore(true);
        state.set_grid(grid);
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        std::thread::sleep(Duration::from_millis(25));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(
            (state.get_library_scroll_index() - anchor).abs() < 6,
            "switching list/grid must preserve the visible song range"
        );
        if !grid {
            assert!(
                (state.get_song_first() - anchor).abs() <= 2,
                "the rendered list viewport must restore near the grid's song anchor"
            );
        }
        screenshot(
            pixels,
            if grid {
                "library-position-restored-grid"
            } else {
                "library-position-restored-list"
            },
        );
    }
    state.set_songs(Default::default());
    state.set_song_grid(Default::default());
    state.set_group_grid(Default::default());
    state.set_search("missing result".into());
    for view in ["songs", "artists", "albums", "genres", "playlists"] {
        state.set_view(view.into());
        state.set_catalog_loading(true);
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let pending = pixels.clone();
        state.set_catalog_loading(false);
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let changed = (200..650)
            .flat_map(|y| (200..1000).map(move |x| y * 1280 + x))
            .filter(|&index| pixels[index] != pending[index])
            .count();
        assert!(
            changed > 100,
            "{view} must show an empty-search message only after loading finishes"
        );
        screenshot(pixels, &format!("empty-search-{view}"));
    }
    state.set_search("".into());
    state.set_songs(saved_songs);
    state.set_song_grid(saved_grid);
    state.set_group_grid(saved_groups);
    state.set_view("songs".into());
}
