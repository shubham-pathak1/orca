use super::*;

pub(super) fn verify(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let pixels = &mut case.pixels;
    let songs = &case.songs;
    // Already-loaded grid rows exercise native scrolling without image-decoding noise.
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
    ui.library().set_columns(6);
    ui.library()
        .set_song_grid(Rc::new(VecModel::from(colored_rows)).into());
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(pixels[150 * 1280 + 100].r > 150);
    let unselected = pixels.clone();
    let previous_track = ui.player().get_now();
    ui.player().set_now(Song {
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
    ui.player().set_now(previous_track);
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
    slint::platform::update_timers_and_animations();
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(100.0, 150.0),
        delta_x: 0.0,
        delta_y: -50.0,
    });
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let final_red = red_height(pixels);
    assert_eq!(
        initial_red - final_red,
        50,
        "public phase-less pointer input must preserve its distance through the native grid"
    );
    for delta in [-1.8, -2.2] {
        slint::platform::update_timers_and_animations();
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(100.0, 150.0),
            delta_x: 0.0,
            delta_y: delta,
        });
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
    }
    // Public PointerScrolled dispatches Slint's direct/cancelled phase. Real
    // Winit wheel/gesture phases and their physics are handled by the backend.
    std::thread::sleep(Duration::from_millis(220));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let precise_red = red_height(pixels);
    assert!(
        (3..=5).contains(&(final_red - precise_red)),
        "small wheel inputs must preserve their complete distance"
    );
    let precise_settled = pixels.clone();
    std::thread::sleep(Duration::from_millis(110));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_eq!(
        *pixels, precise_settled,
        "fine input must not leave a trailing animation"
    );
    let start = ui.library().get_artwork_scroll_position();
    let mut previous = start;
    for delta in [-50.0, -80.0, -100.0, -35.0] {
        slint::platform::update_timers_and_animations();
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(100.0, 150.0),
            delta_x: 0.0,
            delta_y: delta,
        });
        std::thread::sleep(Duration::from_millis(12));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let position = ui.library().get_artwork_scroll_position();
        assert!(
            position <= previous + 0.5,
            "continuous downward input must not jump backward while rows recycle"
        );
        previous = position;
    }
    std::thread::sleep(Duration::from_millis(220));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        (ui.library().get_artwork_scroll_position() - (start - 265.0)).abs() < 2.0,
        "native input routing must preserve total wheel distance across recycled rows"
    );
    slint::platform::update_timers_and_animations();
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(100.0, 150.0),
        delta_x: 0.0,
        delta_y: -900.0,
    });
    std::thread::sleep(Duration::from_millis(220));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert!(
        pixels[150 * 1280 + 100].b > 150,
        "large native wheel movement must reach the following grid rows"
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
    let selected = Rc::new(RefCell::new(Vec::new()));
    let observed = selected.clone();
    ui.player()
        .on_play(move |path| observed.borrow_mut().push(path.to_string()));
    ui.library().set_jump_index(0);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision() + 1);
    ui.window().request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let position = slint::LogicalPosition::new(100.0, 150.0);
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    ui.library().set_jump_index(12);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision() + 1);
    ui.window().request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    assert_eq!(ui.library().get_library_scroll_index(), 12);
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    assert!(
        selected.borrow().is_empty(),
        "recycling a pressed card must not play its replacement: {:?}",
        selected.borrow()
    );
    click(window, 100.0, 150.0);
    assert_eq!(&*selected.borrow(), &["scroll-2"]);
    let played = case.played.clone();
    ui.player().on_play(move |_| played.set(played.get() + 1));
    let mut initial = vec![songs.row_data(0).unwrap()];
    initial.resize(6, Song::default());
    ui.library().set_song_grid(
        Rc::new(VecModel::from(
            vec![Rc::new(VecModel::from(initial)).into()],
        ))
        .into(),
    );
    ui.library().set_jump_index(0);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    // Catalog overviews share wheel handling and virtualized rows, including
    // compact artist and playlist entries.
    for view in ["artists", "albums", "genres", "playlists"] {
        ui.shell().set_view(view.into());
        ui.library().set_group_columns(6);
        ui.library().set_group_grid(
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
        slint::platform::update_timers_and_animations();
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
        let previous_scroll = ui.library().get_catalog_scroll_y();
        assert!(
            previous_scroll < 0.0,
            "{view} should record its scroll position"
        );
        ui.library().set_detail_key(format!("{view}-0-0").into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        ui.library().set_detail_key("".into());
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        ui.library().set_catalog_scroll_target(previous_scroll);
        ui.library().set_catalog_scroll_restore_revision(
            ui.library()
                .get_catalog_scroll_restore_revision()
                .wrapping_add(1),
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
            (ui.library().get_catalog_scroll_y() - previous_scroll).abs() < 1.0,
            "{view} should return to the previous scroll position: before {previous_scroll}, after {}",
            ui.library().get_catalog_scroll_y()
        );
        assert_eq!(
            pixels[150 * 1280 + 100],
            scrolled_pixel,
            "{view} should show the same row after returning"
        );
    }
    let saved_songs = ui.library().get_songs();
    let saved_grid = ui.library().get_song_grid();
    let saved_groups = ui.library().get_group_grid();
    ui.shell().set_view("songs".into());
    ui.preferences().set_grid(true);
    ui.library().set_library_view_restore(false);
    let library: Vec<Song> = (0..600)
        .map(|index| Song {
            path: format!("position-{index}").into(),
            title: format!("Song {index}").into(),
            ..Default::default()
        })
        .collect();
    ui.library()
        .set_songs(Rc::new(VecModel::from(library.clone())).into());
    ui.library().set_song_grid(
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
    ui.library().set_jump_index(180);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision().wrapping_add(1));
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    std::thread::sleep(Duration::from_millis(20));
    slint::platform::update_timers_and_animations();
    window.draw_if_needed(|renderer| {
        renderer.render(pixels, 1280);
    });
    let anchor = ui.library().get_library_scroll_index();
    screenshot(pixels, "library-anchor-before-switch");
    assert!(
        anchor >= 174,
        "grid should record a song anchor after jumping, got {anchor}"
    );
    for grid in [false, true] {
        ui.library().set_library_view_restore(true);
        ui.preferences().set_grid(grid);
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        std::thread::sleep(Duration::from_millis(25));
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        assert!(
            (ui.library().get_library_scroll_index() - anchor).abs() < 6,
            "switching list/grid must preserve the visible song range"
        );
        if !grid {
            assert!(
                (ui.library().get_song_first() - anchor).abs() <= 2,
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
    ui.library().set_songs(Default::default());
    ui.library().set_song_grid(Default::default());
    ui.library().set_group_grid(Default::default());
    ui.library().set_search("missing result".into());
    for view in ["songs", "artists", "albums", "genres", "playlists"] {
        ui.shell().set_view(view.into());
        ui.library().set_catalog_loading(true);
        window.draw_if_needed(|renderer| {
            renderer.render(pixels, 1280);
        });
        let pending = pixels.clone();
        ui.library().set_catalog_loading(false);
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
    ui.library().set_search("".into());
    ui.library().set_songs(saved_songs);
    ui.library().set_song_grid(saved_grid);
    ui.library().set_group_grid(saved_groups);
    ui.shell().set_view("songs".into());
}

pub(super) fn verify_tables_and_collection_play(case: &mut UiCase) {
    let ui = &case.ui;
    let window = &case.window;

    let saved_grid = ui.preferences().get_grid();
    let saved_sort = ui.library().get_sort();
    let saved_songs = ui.library().get_songs();
    let saved_groups = ui.library().get_group_grid();
    let saved_dynamic = ui.preferences().get_dynamic_accent();
    let saved_now = ui.player().get_now();
    let saved_playing = ui.player().get_playing();
    let (tx, rx) = std::sync::mpsc::channel();
    controllers::collections::install(ui, tx);
    let opened = Rc::new(RefCell::new(Vec::new()));
    let observed = opened.clone();
    ui.library().on_open_group(move |key, secondary, _| {
        observed
            .borrow_mut()
            .push((key.to_string(), secondary.to_string()))
    });
    let cover = slint::Image::from_rgba8(
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            &[80, 60, 130, 255].repeat(80 * 80),
            80,
            80,
        ),
    );
    ui.library().set_songs(
        Rc::new(VecModel::from(vec![Song {
            id: 42,
            path: "table.flac".into(),
            title: "A".into(),
            artist: "Band; Guest".into(),
            album: "Album".into(),
            album_artist: "Band".into(),
            cover: cover.clone(),
            quality: "FLAC | 44.1 kHz | 900 kbps".into(),
            ..Default::default()
        }]))
        .into(),
    );
    ui.preferences().set_grid(false);
    ui.library().set_sort("artist".into());
    ui.library().set_jump_index(0);
    ui.library()
        .set_jump_revision(ui.library().get_jump_revision() + 1);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    let played = case.played.get();
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(162.0, 140.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    screenshot(&case.pixels, "library-table-hover-heart");
    click(window, 162.0, 140.0);
    assert!(matches!(rx.try_recv().unwrap(), Request::ToggleLiked(42)));
    assert_eq!(case.played.get(), played, "liking must not start playback");
    let mut liked = ui.library().get_songs().row_data(0).unwrap();
    liked.liked = true;
    ui.library()
        .set_songs(Rc::new(VecModel::from(vec![liked])).into());
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(600.0, 400.0),
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    assert!(
        (126..154)
            .flat_map(|y| (148..176).map(move |x| y * 1280 + x))
            .all(|index| case.pixels[index].r < 80),
        "a liked heart must also hide when the pointer leaves its song"
    );

    click(window, 900.0, 140.0);
    assert_eq!(ui.shell().get_view(), "albums");
    assert_eq!(
        opened.borrow().last(),
        Some(&("Album".into(), "Band".into()))
    );
    ui.shell().set_view("songs".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(window, 580.0, 140.0);
    assert_eq!(ui.shell().get_view(), "artists");
    assert_eq!(opened.borrow().last(), Some(&("Band".into(), "".into())));
    assert_eq!(
        case.played.get(),
        played,
        "collection links must not start playback"
    );
    let saved_song_grid = ui.library().get_song_grid();
    let grid_song = ui.library().get_songs().row_data(0).unwrap();
    ui.library().set_song_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            grid_song,
        ]))
        .into()]))
        .into(),
    );
    ui.preferences().set_grid(true);
    let columns = ui.library().get_columns().max(1) as f32;
    let artist_y =
        84.0 + (ui.get_browse_width() - 44.0 - 16.0 * (columns - 1.0)).floor() / columns + 38.0;
    for (x, artist) in [(100.0, "Band"), (158.0, "Guest")] {
        ui.shell().set_view("songs".into());
        window.draw_if_needed(|renderer| {
            renderer.render(&mut case.pixels, 1280);
        });
        click(window, x, artist_y);
        assert_eq!(ui.shell().get_view(), "artists", "grid link for {artist}");
        assert_eq!(opened.borrow().last(), Some(&(artist.into(), "".into())));
        assert_eq!(
            case.played.get(),
            played,
            "grid artist links must not play their song"
        );
    }
    ui.preferences().set_grid(false);
    ui.library().set_song_grid(saved_song_grid);
    ui.shell().set_view("songs".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(window, 160.0, 100.0);
    assert_eq!(ui.library().get_sort(), "title-asc");
    click(window, 160.0, 100.0);
    assert_eq!(ui.library().get_sort(), "title-desc");
    click(window, 1160.0, 100.0);
    assert_eq!(ui.library().get_sort(), "duration-asc");
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    screenshot(&case.pixels, "library-table-typography");
    let duration_pixels = case.pixels.clone();
    ui.library().set_sort("recent".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    let sort_label_changes = (20..60)
        .flat_map(|y| (1110..1250).map(move |x| y * 1280 + x))
        .filter(|&index| case.pixels[index] != duration_pixels[index])
        .count();
    assert!(
        sort_label_changes > 20,
        "list view must retain the sort control and its Recently added choice"
    );
    assert_eq!(crate::controllers::browse::query(ui).0.sort, "recent");
    screenshot(&case.pixels, "library-recent-sort");
    ui.library().set_sort("duration-asc".into());

    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("Artist".into());
    ui.library().set_detail_sort("".into());
    window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(window, 900.0, 326.0);
    assert_eq!(ui.library().get_detail_sort(), "album-asc");
    assert_eq!(crate::controllers::browse::query(ui).0.sort, "album-asc");
    click(window, 900.0, 326.0);
    assert_eq!(ui.library().get_detail_sort(), "album-desc");
    assert_eq!(ui.library().get_sort(), "duration-asc");
    screenshot(&case.pixels, "artist-table-sort");
    ui.library().set_detail_key("".into());
    ui.library().set_detail_sort("".into());
    let group = CatalogGroup {
        key: "Album".into(),
        secondary: "Artist".into(),
        title: "Album".into(),
        subtitle: "Artist".into(),
        cover,
        accent: slint::Color::from_rgb_u8(195, 160, 220),
        ..Default::default()
    };
    ui.library().set_group_columns(6);
    ui.library().set_group_grid(
        Rc::new(VecModel::from(vec![Rc::new(VecModel::from(vec![
            group
                .clone(
                );
            6
        ]))
        .into()]))
        .into(),
    );
    for view in ["albums", "genres", "artists", "playlists"] {
        ui.shell().set_view(view.into());
        window.dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(160.0, 150.0),
        });
        for light in [false, true] {
            ui.preferences().set_light_theme(light);
            for dynamic in [false, true] {
                ui.preferences().set_dynamic_accent(dynamic);
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut case.pixels, 1280);
                });
                screenshot(
                    &case.pixels,
                    &format!("{view}-play-overlay-{light}-{dynamic}"),
                );
            }
        }
        // Locate the visible accented overlay rather than hard-coding the card layout.
        let overlay: Vec<_> = case
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let (x, y) = (i % 1280, i / 1280);
                ((76..300).contains(&x)
                    && (104..400).contains(&y)
                    && (190..=200).contains(&p.r)
                    && (155..=165).contains(&p.g)
                    && (215..=225).contains(&p.b))
                .then_some((x, y))
            })
            .collect();
        assert!(
            overlay.len() > 40,
            "{view}: play overlay must be visible on hover"
        );
        let (x, y) = overlay
            .iter()
            .fold((0usize, 0usize), |sum, p| (sum.0 + p.0, sum.1 + p.1));
        click(
            window,
            x as f32 / overlay.len() as f32,
            y as f32 / overlay.len() as f32,
        );
        assert!(
            matches!(rx.try_recv(),Ok(Request::Collection(crate::protocol::CollectionAction::Toggle,query)) if query.kind==view && query.key=="Album"),
            "{view}: overlay must toggle its collection"
        );
        assert!(
            ui.library().get_detail_key().is_empty(),
            "overlay must not open the collection"
        );
        ui.player().set_now(Song {
            path: "playing.flac".into(),
            ..Default::default()
        });
        ui.player().set_playback_kind(view.into());
        ui.player().set_playback_key("Album".into());
        ui.player().set_playback_secondary("Artist".into());
        ui.player().set_playing(true);
        assert!(ui.player().invoke_collection_playing(
            view.into(),
            "Album".into(),
            "Artist".into()
        ));
        assert!(!ui.player().invoke_collection_playing(
            view.into(),
            "Other".into(),
            "Artist".into()
        ));
        window.draw_if_needed(|renderer| {
            renderer.render(&mut case.pixels, 1280);
        });
        screenshot(&case.pixels, &format!("{view}-pause-overlay"));
        ui.player().set_playing(false);
    }
    ui.player().set_playback_kind("".into());
    ui.player().set_playback_key("".into());
    ui.player().set_playback_secondary("".into());
    ui.player().set_now(saved_now);
    ui.player().set_playing(saved_playing);
    ui.preferences().set_light_theme(false);
    ui.preferences().set_dynamic_accent(saved_dynamic);
    ui.preferences().set_grid(saved_grid);
    ui.library().set_sort(saved_sort);
    ui.library().set_songs(saved_songs);
    ui.library().set_group_grid(saved_groups);
    ui.shell().set_view("songs".into());
}

pub(super) fn verify_seek_previews(case: &mut UiCase) {
    let saved_duration = case.ui.player().get_duration();
    let saved_position = case.ui.player().get_position();
    let saved_tips = case.ui.preferences().get_show_tooltips();
    let saved_style = case.ui.preferences().get_seek_style();
    let saved_now = case.ui.player().get_now();
    case.ui.player().set_now(Song {
        path: "preview.flac".into(),
        title: "Seek preview".into(),
        ..Default::default()
    });
    case.ui.player().set_duration(240000.0);
    case.ui.player().set_position(60000.0);
    for full in [false, true] {
        case.ui.player().set_full_player(full);
        for style in ["line", "waveform"] {
            case.ui.preferences().set_seek_style(style.into());
            case.ui.preferences().set_show_tooltips(false);
            case.window.dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(
                    if full { 640.0 } else { 650.0 },
                    if full { 622.0 } else { 748.0 },
                ),
            });
            case.window.draw_if_needed(|renderer| {
                renderer.render(&mut case.pixels, 1280);
            });
            let without = case.pixels.clone();
            case.ui.preferences().set_show_tooltips(true);
            case.window.draw_if_needed(|renderer| {
                renderer.render(&mut case.pixels, 1280);
            });
            let y = if full { 574..622 } else { 700..748 };
            let changed = y
                .flat_map(|y| (600..720).map(move |x| y * 1280 + x))
                .filter(|&index| case.pixels[index] != without[index])
                .count();
            assert!(
                changed > 20,
                "{full} {style}: a timestamp must appear above the hovered seekbar"
            );
            assert_eq!(
                case.ui.player().get_position(),
                60000.0,
                "hovering must not seek"
            );
            screenshot(&case.pixels, &format!("seek-preview-{full}-{style}"));
        }
    }
    case.window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(1000.0, 100.0),
    });
    case.ui.player().set_full_player(false);
    case.ui.player().set_duration(saved_duration);
    case.ui.player().set_position(saved_position);
    case.ui.preferences().set_show_tooltips(saved_tips);
    case.ui.preferences().set_seek_style(saved_style);
    case.ui.player().set_now(saved_now);
}

pub(super) fn verify_queue_remove_and_audio_details(case: &mut UiCase) {
    let saved_now = case.ui.player().get_now();
    let saved_queue = case.ui.player().get_queue();
    let removed = Rc::new(RefCell::new(Vec::new()));
    let observed = removed.clone();
    case.ui.player().on_queue_action(move |action, path, _| {
        observed
            .borrow_mut()
            .push((action.to_string(), path.to_string()))
    });
    case.ui.player().set_now(Song {
        path: "audio.flac".into(),
        title: "Audio".into(),
        quality: "FLAC | 44.1 kHz | 900 kbps".into(),
        bit_depth: 24,
        ..Default::default()
    });
    case.ui.player().set_queue(
        Rc::new(VecModel::from(vec![Song {
            path: "queued.flac".into(),
            title: "Queued song".into(),
            duration: "3:00".into(),
            ..Default::default()
        }]))
        .into(),
    );
    for full in [false, true] {
        case.ui.player().set_full_player(full);
        case.ui.player().set_queue_open(true);
        case.window.draw_if_needed(|renderer| {
            renderer.render(&mut case.pixels, 1280);
        });
        case.window.dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(1243.0, 114.0),
        });
        case.window.draw_if_needed(|renderer| {
            renderer.render(&mut case.pixels, 1280);
        });
        click(&case.window, 1243.0, 114.0);
        assert_eq!(
            removed.borrow().last(),
            Some(&("remove".into(), "queued.flac".into()))
        );
        assert_eq!(
            removed.borrow().len(),
            if full { 2 } else { 1 },
            "remove must not also play the row"
        );
        case.ui.player().set_queue_open(false);
    }
    case.ui.player().set_full_player(false);
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(&case.window, 200.0, 790.0);
    assert!(
        case.ui.dialogs().get_audio_info_open(),
        "the bottom quality label must open audio details"
    );
    assert_eq!(case.ui.shell().get_focus_layer(), "dialog");
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    case.ui.dialogs().set_audio_info_open(false);
    case.ui.player().set_full_player(true);
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    click(&case.window, 640.0, 556.0);
    assert!(
        case.ui.dialogs().get_audio_info_open(),
        "the full-player quality badge must open the same details"
    );
    case.window.draw_if_needed(|renderer| {
        renderer.render(&mut case.pixels, 1280);
    });
    case.ui.dialogs().set_audio_info_open(false);
    case.ui.player().set_full_player(false);
    case.ui.player().set_now(saved_now);
    case.ui.player().set_queue(saved_queue);
}

pub(super) fn verify_controller(ui: &OrcaWindow) {
    let (worker, _events, requests) = crate::worker::AppWorker::test_channels();
    let worker = Rc::new(worker);
    let cache = Rc::new(RefCell::new(artwork::ArtworkCache::new(1024 * 1024)));
    let catalog = Rc::new(models::CatalogModels::new(worker.tx.clone(), cache.clone()));
    let timer = Rc::new(slint::Timer::default());
    controllers::browse::install(ui, worker, catalog, cache, timer.clone());

    ui.shell().set_view("artists".into());
    ui.library().set_detail_key("".into());
    ui.library().set_detail_secondary("".into());
    ui.library().set_search("".into());
    ui.library().set_catalog_loading(false);
    ui.library().set_catalog_scroll_y(420.0);
    ui.library()
        .invoke_open_group("Artist".into(), "".into(), "Artist".into());
    assert_eq!(ui.library().get_detail_key(), "Artist");
    std::thread::sleep(Duration::from_millis(3));
    slint::platform::update_timers_and_animations();
    assert!(
        matches!(requests.try_recv().unwrap(),Request::Browse(_,query,view) if query.key=="Artist" && view=="artists")
    );
    ui.library().invoke_go_back();
    assert!(ui.library().get_detail_key().is_empty());
    assert_eq!(ui.library().get_catalog_scroll_target(), 420.0);
    assert!(ui.library().get_catalog_scroll_restore_pending());
    timer.stop();
}
