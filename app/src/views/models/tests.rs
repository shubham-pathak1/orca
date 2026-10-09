use super::*;
#[test]
fn disabled_browse_artwork_keeps_metadata_without_requesting_images() {
    let (tx, _) = std::sync::mpsc::channel();
    let cache = Rc::new(RefCell::new(ArtworkCache::new(1024)));
    let store = Tracks::new(tx, cache.clone());
    store.apply(
        1,
        0,
        1,
        vec![Track {
            path: "song.flac".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            artwork: "preview.webp".into(),
            artwork_thumb: "thumb.webp".into(),
            ..Default::default()
        }],
    );
    store.artwork.set(false);
    for grid in [false, true] {
        let row = store.row(0, 192, grid);
        assert_eq!(row.title, "Song");
        assert_eq!(row.path, "song.flac");
        assert_eq!(row.cover.size().width, 0);
    }
    let groups = GroupGrid {
        landscape: Cell::new(false),
        visible_rows: RefCell::new(HashMap::new()),
        artwork: Cell::new(false),
        circular: Cell::new(false),
        groups: RefCell::new(vec![Group {
            key: "playlist".into(),
            title: "Playlist".into(),
            artwork: "cover.webp".into(),
            ..Default::default()
        }]),
        notify: ModelNotify::default(),
        columns: Cell::new(1),
        edge: Cell::new(192),
        cache: cache.clone(),
    };
    assert_eq!(
        groups.row_data(0).unwrap().row_data(0).unwrap().title,
        "Playlist"
    );
    assert_eq!(cache.borrow().requested_count(), 0);
    store.artwork.set(true);
    store.row(0, 192, true);
    assert_eq!(cache.borrow().requested_count(), 1);
}
#[test]
fn pending_existing_covers_are_not_marked_missing_and_complete_normally() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/slint/model-artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("cover.png");
    image::RgbaImage::from_pixel(48, 48, image::Rgba([200, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let cache = Rc::new(RefCell::new(ArtworkCache::new(1024 * 1024)));
    let track = Track {
        path: "track.flac".into(),
        artwork: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let pending = grid_song(&track, 0, 64, &cache);
    assert!(!pending.cover_missing);
    assert_eq!(pending.cover.size().width, 0);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        cache.borrow_mut().drain();
        let ready = grid_song(&track, 0, 64, &cache);
        if ready.cover.size().width > 0 {
            assert!(!ready.cover_missing);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(grid_song(&Track::default(), 0, 64, &cache).cover_missing);
}
#[test]
fn high_dpi_small_covers_decode_at_display_size_from_tauri_thumbnails() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/model-dpi-test");
    std::fs::create_dir_all(&directory).unwrap();
    let thumb = directory.join("thumb.png");
    let preview = directory.join("preview.png");
    image::RgbaImage::from_pixel(80, 80, image::Rgba([20, 40, 220, 255]))
        .save(&thumb)
        .unwrap();
    image::RgbaImage::from_pixel(256, 256, image::Rgba([220, 40, 20, 255]))
        .save(&preview)
        .unwrap();
    let track = Track {
        path: "track.flac".into(),
        artwork_thumb: thumb.to_string_lossy().into_owned(),
        artwork: preview.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let cache = Rc::new(RefCell::new(ArtworkCache::new(1024 * 1024)));
    let (tx, _) = std::sync::mpsc::channel();
    let store = Rc::new(Tracks::new(tx, cache.clone()));
    store.apply(1, 0, 1, vec![track.clone()]);
    store.list_edge.set(physical_artwork_edge(28, 1.25));
    let list = SongList {
        store,
        notify: ModelNotify::default(),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        cache.borrow_mut().drain();
        let rows = [
            list.row_data(0).unwrap(),
            song(&track, 0, physical_artwork_edge(44, 1.25), &cache),
            song(&track, 0, physical_artwork_edge(48, 1.25), &cache),
        ];
        if rows.iter().all(|row| row.cover.size().width > 0) {
            assert_eq!(
                rows.iter()
                    .map(|row| row.cover.size().width)
                    .collect::<Vec<_>>(),
                vec![35, 55, 60]
            );
            let center = rows[2].cover.to_rgba8().unwrap();
            assert!(
                center.as_slice()[30 * 60 + 30].b > 200,
                "small covers must use the thumbnail source"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "DPI artwork requests did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn list_uses_thumbnail_and_grid_uses_preview_independently_of_decode_size() {
    let track = Track {
        artwork: "preview.webp".into(),
        artwork_thumb: "thumb.webp".into(),
        ..Default::default()
    };
    assert_eq!(artwork_path(&track, true), "thumb.webp");
    assert_eq!(artwork_path(&track, false), "preview.webp");
    assert_eq!(
        artwork_path(
            &Track {
                artwork: "preview.webp".into(),
                ..Default::default()
            },
            true
        ),
        "preview.webp"
    );
}
#[test]
fn pending_reloads_do_not_clear_already_visible_grid_covers() {
    let (tx, _) = std::sync::mpsc::channel();
    let cache = Rc::new(RefCell::new(ArtworkCache::new(1024 * 1024)));
    let store = Rc::new(Tracks::new(tx, cache));
    store.apply(
        1,
        0,
        1,
        vec![Track {
            path: "current-song".into(),
            artwork: "pending-preview".into(),
            ..Default::default()
        }],
    );
    let grid = SongGrid {
        store,
        notify: ModelNotify::default(),
        columns: Cell::new(1),
        edge: Cell::new(160),
        visible_rows: RefCell::new(HashMap::new()),
    };
    let row = grid.row_data(0).unwrap();
    let model = grid
        .visible_rows
        .borrow()
        .get(&0)
        .unwrap()
        .upgrade()
        .unwrap();
    let mut song = row.row_data(0).unwrap();
    song.cover =
        slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(160, 160));
    model.set_row_data(0, song);
    let _ = grid.row_data(0);
    assert_eq!(row.row_data(0).unwrap().cover.size().width, 160);
    grid.artwork_changed(&[0]);
    assert_eq!(row.row_data(0).unwrap().cover.size().width, 160);
    drop(model);
    drop(row);
    grid.artwork_changed(&[]);
    assert!(grid.visible_rows.borrow().is_empty());
}
#[test]
fn cover_completion_updates_one_visible_cell_without_replacing_the_row() {
    let (tx, _) = std::sync::mpsc::channel();
    let store = Rc::new(Tracks::new(
        tx,
        Rc::new(RefCell::new(ArtworkCache::new(1024))),
    ));
    store.apply(
        1,
        0,
        2,
        vec![
            Track {
                title: "First".into(),
                ..Default::default()
            },
            Track {
                title: "Second".into(),
                ..Default::default()
            },
        ],
    );
    let grid = SongGrid {
        store: store.clone(),
        notify: ModelNotify::default(),
        columns: Cell::new(2),
        edge: Cell::new(192),
        visible_rows: RefCell::new(HashMap::new()),
    };
    let row = grid.row_data(0).unwrap();
    let mut first = store.rows.borrow().get(&0).unwrap().clone();
    let mut second = store.rows.borrow().get(&1).unwrap().clone();
    first.title = "Updated first".into();
    second.title = "Unrelated change".into();
    store.apply(1, 0, 2, vec![first, second]);
    grid.artwork_changed(&[0]);
    assert_eq!(row.row_data(0).unwrap().title, "Updated first");
    assert_eq!(row.row_data(1).unwrap().title, "Second");
    drop(row);
    grid.artwork_changed(&[]);
    assert!(
        grid.visible_rows.borrow().is_empty(),
        "offscreen rows must not retain images"
    );
}
#[test]
fn catalog_cover_updates_reuse_the_row_and_leave_other_cells_unchanged() {
    let grid = GroupGrid {
        landscape: Cell::new(false),
        visible_rows: RefCell::new(HashMap::new()),
        artwork: Cell::new(false),
        circular: Cell::new(false),
        groups: RefCell::new(vec![
            Group {
                key: "a".into(),
                title: "First".into(),
                artwork: "a.webp".into(),
                ..Default::default()
            },
            Group {
                key: "b".into(),
                title: "Second".into(),
                artwork: "b.webp".into(),
                ..Default::default()
            },
        ]),
        notify: ModelNotify::default(),
        columns: Cell::new(2),
        edge: Cell::new(135),
        cache: Rc::new(RefCell::new(ArtworkCache::new(1024))),
    };
    let row = grid.row_data(0).unwrap();
    let before = grid
        .visible_rows
        .borrow()
        .get(&0)
        .unwrap()
        .upgrade()
        .unwrap();
    let same = grid.row_data(0).unwrap();
    let after = grid
        .visible_rows
        .borrow()
        .get(&0)
        .unwrap()
        .upgrade()
        .unwrap();
    assert!(Rc::ptr_eq(&before, &after));
    grid.groups.borrow_mut()[0].title = "Updated".into();
    grid.groups.borrow_mut()[1].title = "Unrelated".into();
    grid.artwork_changed(&[Key {
        path: "a.webp".into(),
        tiles: vec![],
        edge: 135,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    }]);
    assert_eq!(row.row_data(0).unwrap().title, "Updated");
    assert_eq!(same.row_data(1).unwrap().title, "Second");
    drop(before);
    drop(after);
    drop(row);
    drop(same);
    grid.artwork_changed(&[]);
    assert!(grid.visible_rows.borrow().is_empty());
}
#[test]
fn artwork_completion_only_invalidates_tracks_using_that_cover() {
    let (tx, _) = std::sync::mpsc::channel();
    let tracks = Tracks::new(tx, Rc::new(RefCell::new(ArtworkCache::new(1024))));
    tracks.apply(
        1,
        0,
        3,
        vec![
            Track {
                artwork: "a.webp".into(),
                ..Default::default()
            },
            Track {
                artwork: "b.webp".into(),
                ..Default::default()
            },
            Track {
                artwork: "a.webp".into(),
                ..Default::default()
            },
        ],
    );
    let mut rows = tracks.artwork_indices(&[Key {
        path: "a.webp".into(),
        tiles: vec![],
        edge: 192,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    }]);
    rows.sort_unstable();
    assert_eq!(rows, vec![0, 2]);
    assert!(tracks
        .artwork_indices(&[Key {
            path: "a.webp".into(),
            tiles: vec![],
            edge: 160,
            height: 0,
            radius: 0,
            backdrop: true,
            revision: 0
        }])
        .is_empty());
}
