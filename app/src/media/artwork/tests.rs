use super::decode::{compose, round_corners};
use super::*;
#[test]
fn three_cover_collage_has_a_full_height_left_tile_and_two_right_tiles() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/collage-test");
    std::fs::create_dir_all(&directory).unwrap();
    let colors = [[220, 0, 0, 255], [0, 220, 0, 255], [0, 0, 220, 255]];
    let paths: Vec<_> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| {
            let path = directory.join(format!("tile-{index}.png"));
            image::RgbaImage::from_pixel(16, 16, image::Rgba(*color))
                .save(&path)
                .unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect();
    let image = compose(&paths, 101).to_rgba8();
    assert_eq!(image.dimensions(), (101, 75));
    assert_eq!(image.get_pixel(10, 10).0, colors[0]);
    assert_eq!(image.get_pixel(10, 65).0, colors[0]);
    assert_eq!(image.get_pixel(90, 10).0, colors[1]);
    assert_eq!(image.get_pixel(90, 65).0, colors[2]);
    assert_eq!(image.get_pixel(50, 10).0, [16, 16, 16, 255]);
}
#[test]
fn grid_and_genre_covers_round_after_cropping_to_the_display_shape() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("grid-corners.png");
    image::RgbaImage::from_pixel(80, 120, image::Rgba([220, 120, 80, 255]))
        .save(&path)
        .unwrap();
    let path = path.to_string_lossy().into_owned();
    let cache = ArtworkCache::new(1024 * 1024);
    for (landscape, tiles) in [
        (false, vec![]),
        (true, vec![]),
        (true, vec![path.clone(), path.clone()]),
    ] {
        let decoded = decode(cache.grid_key(&path, &tiles, 192, landscape));
        assert_eq!(
            (decoded.width, decoded.height),
            (192, if landscape { 144 } else { 192 })
        );
        for (x, y) in [
            (0, 0),
            (191, 0),
            (0, decoded.height - 1),
            (191, decoded.height - 1),
        ] {
            assert_eq!(
                decoded.bytes[((y * decoded.width + x) * 4 + 3) as usize],
                0,
                "all displayed corners must be transparent"
            );
        }
        assert_eq!(
            decoded.bytes[((decoded.height / 2 * decoded.width + 48) * 4 + 3) as usize],
            255,
            "rounding must preserve the cover interior"
        );
    }
}
#[test]
fn rounded_and_circular_covers_keep_their_centers_and_transparent_corners() {
    let mut rounded = image::RgbaImage::from_pixel(100, 100, image::Rgba([220, 120, 80, 255]));
    round_corners(&mut rounded, 8);
    assert_eq!(rounded.get_pixel(0, 0).0[3], 0);
    assert_eq!(rounded.get_pixel(50, 50).0, [220, 120, 80, 255]);
    assert_eq!(rounded.get_pixel(50, 0).0[3], 255);
    let mut circular = image::RgbaImage::from_pixel(100, 100, image::Rgba([220, 120, 80, 255]));
    round_corners(&mut circular, 50);
    assert_eq!(circular.get_pixel(0, 0).0[3], 0);
    assert_eq!(circular.get_pixel(50, 50).0[3], 255);
    assert_eq!(circular.get_pixel(10, 10).0[3], 0);
    assert!(circular
        .pixels()
        .any(|pixel| pixel.0[3] > 0 && pixel.0[3] < 255));
}
#[test]
fn queue_and_decoded_memory_are_bounded() {
    let mut cache = ArtworkCache::new(1024);
    for index in 0..1000 {
        cache.get(&format!("missing-{index}.png"), 48, false);
    }
    assert!(cache.pending.len() <= 12);
    assert!(cache.deferred.len() <= 128);
    assert!(cache.wanted.lock().unwrap().len() <= 140);
    assert!(cache.bytes <= cache.limit);
}
#[test]
fn visible_requests_complete_even_when_the_worker_queue_is_full() {
    let mut cache = ArtworkCache::new(1024);
    for index in 0..60 {
        cache.get(&format!("visible-{index}.png"), 48, false);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut completed = HashSet::new();
    while !cache.pending.is_empty() || !cache.deferred.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        completed.extend(cache.drain().into_iter().map(|key| key.path));
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        completed.len(),
        60,
        "delegates must not need a global refresh to retry"
    );
    assert_eq!(cache.entries.len(), 60);
    cache.get("deferred-before-clear.png", 48, false);
    cache.clear();
    assert!(cache.deferred.is_empty());
}
#[test]
fn bad_files_produce_an_empty_result_instead_of_panicking() {
    let decoded = decode(Key {
        path: "nonexistent.png".into(),
        tiles: vec![],
        edge: 192,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    });
    assert!(decoded.bytes.is_empty());
}
#[test]
fn portrait_covers_fill_the_display_square_without_another_upscale() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("portrait.png");
    image::RgbaImage::from_pixel(80, 160, image::Rgba([100, 120, 140, 255]))
        .save(&path)
        .unwrap();
    let decoded = decode(Key {
        path: path.to_string_lossy().into(),
        tiles: vec![],
        edge: 64,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    });
    assert_eq!((decoded.width, decoded.height), (64, 64));
}
#[test]
fn backdrop_is_interpolated_once_without_enlarged_texel_steps() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("gradient.png");
    let source = image::RgbaImage::from_fn(160, 160, |x, _| {
        image::Rgba([(x * 255 / 159) as u8, 0, 0, 255])
    });
    source.save(&path).unwrap();
    let decoded = decode(Key {
        path: path.to_string_lossy().into_owned(),
        tiles: vec![],
        edge: 160,
        height: 0,
        radius: 0,
        backdrop: true,
        revision: 0,
    });
    assert_eq!((decoded.width, decoded.height), (1024, 1024));
    let row = &decoded.bytes[512 * 1024 * 4..513 * 1024 * 4];
    assert!(row
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect::<Vec<_>>()
        .windows(2)
        .all(|p| p[0].abs_diff(p[1]) <= 1));
    assert!(decoded.bytes.len() <= 4 * 1024 * 1024);
}
#[test]
fn full_player_shading_is_opaque_and_continuous_across_the_center() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("full-blur.png");
    image::RgbaImage::from_pixel(160, 160, image::Rgba([200, 140, 100, 255]))
        .save(&path)
        .unwrap();
    let decoded = decode(Key {
        path: path.to_string_lossy().into_owned(),
        tiles: vec![],
        edge: 1024,
        height: 576,
        radius: 0,
        backdrop: true,
        revision: 0,
    });
    assert_eq!((decoded.width, decoded.height), (1024, 576));
    let row = &decoded.bytes[288 * 1024 * 4..289 * 1024 * 4];
    assert!(row.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    assert!(row
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect::<Vec<_>>()
        .windows(2)
        .all(|p| p[0].abs_diff(p[1]) <= 1));
    assert!(
        row[512 * 4] > row[0],
        "center ambience should remain more visible than the edges"
    );
}
#[test]
fn oversized_decoded_images_do_not_break_the_memory_budget() {
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("large.png");
    image::RgbaImage::from_pixel(48, 48, image::Rgba([100, 120, 140, 255]))
        .save(&path)
        .unwrap();
    let mut cache = ArtworkCache::new(1024);
    cache.get(&path.to_string_lossy(), 48, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !cache.pending.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        cache.drain();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(cache.bytes, 0);
    assert!(cache.bytes <= cache.limit);
}
#[test]
fn obsolete_viewport_jobs_do_not_populate_the_cache_and_can_be_requested_again() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../target/slint/cancel-artwork-test");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("cover.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([100, 120, 140, 255]))
        .save(&path)
        .unwrap();
    let path = path.to_string_lossy().into_owned();
    let mut cache = ArtworkCache::new(1024 * 1024);
    for edge in 100..180 {
        cache.exact(&path, edge);
    }
    cache.retain_requests(&HashSet::new());
    assert!(cache.deferred.is_empty());
    assert!(cache.wanted.lock().unwrap().is_empty());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !cache.pending.is_empty() {
        assert!(std::time::Instant::now() < deadline);
        cache.drain();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(cache.entries.is_empty());
    assert!(
        !cache.failed(&path),
        "cancellation must not become a missing-cover failure"
    );
    cache.exact(&path, 135);
    while cache.exact(&path, 135).size().width == 0 {
        assert!(std::time::Instant::now() < deadline);
        cache.drain();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(cache.exact(&path, 135).size().width, 135);
}
#[test]
fn memory_pressure_preserves_small_thumbnails_before_large_covers() {
    let mut cache = ArtworkCache::new(50_000);
    let thumb = Key {
        path: "thumb".into(),
        tiles: vec![],
        edge: 28,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    };
    let large = Key {
        path: "large".into(),
        edge: 100,
        ..thumb.clone()
    };
    for (used, key) in [thumb.clone(), large.clone()].into_iter().enumerate() {
        let bytes = (key.edge * key.edge * 4) as usize;
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                image: Image::default(),
                bytes,
                used: used as u64,
                accent: [0; 3],
            },
        );
    }
    let new = Key {
        path: "new".into(),
        ..large.clone()
    };
    cache.wanted.lock().unwrap().insert(new.clone());
    let (tx, rx) = mpsc::sync_channel(1);
    cache.rx = rx;
    tx.send(Decoded {
        key: new,
        cancelled: false,
        width: 100,
        height: 100,
        bytes: vec![255; 40_000],
        accent: [0; 3],
    })
    .unwrap();
    cache.drain();
    assert!(cache.entries.contains_key(&thumb));
    assert!(!cache.entries.contains_key(&large));
    assert!(cache.bytes <= cache.limit);
}
#[test]
fn speculative_requests_are_bounded_cancellable_and_wait_for_visible_work() {
    let mut cache = ArtworkCache::new(1024 * 1024);
    cache.stop_worker();
    let (tx, jobs) = mpsc::sync_channel(8);
    cache.tx = tx;
    for index in 0..100 {
        cache.prefetch_cover(&format!("nearby-{index}"), 28, 1, false);
    }
    assert_eq!(cache.prefetch.len(), 64);
    assert!(cache.pending.is_empty());
    cache.exact("visible", 160);
    assert_eq!(jobs.try_recv().unwrap().path, "visible");
    cache.schedule();
    assert!(
        jobs.try_recv().is_err(),
        "prefetch must wait for the visible decode"
    );
    cache.pending.clear();
    cache.schedule();
    assert!(jobs.try_recv().unwrap().path.starts_with("nearby-"));
    assert_eq!(
        cache.pending.len(),
        1,
        "only one speculative decode starts at a time"
    );
    cache.retain_requests(&HashSet::from(["visible".to_string()]));
    assert!(cache.prefetch.is_empty());
    cache.clear();
    assert!(cache.protected.is_empty());
}
#[test]
fn cache_pressure_preserves_visible_grid_covers_before_offscreen_thumbnails() {
    let mut cache = ArtworkCache::new(80_000);
    let template = Key {
        path: String::new(),
        tiles: vec![],
        edge: 100,
        height: 0,
        radius: 0,
        backdrop: false,
        revision: 0,
    };
    let visible = Key {
        path: "visible".into(),
        ..template.clone()
    };
    let thumb = Key {
        path: "offscreen".into(),
        edge: 28,
        ..template.clone()
    };
    for key in [&visible, &thumb] {
        let bytes = (key.edge * key.edge * 4) as usize;
        cache.bytes += bytes;
        cache.entries.insert(
            key.clone(),
            Entry {
                image: Image::default(),
                bytes,
                used: 0,
                accent: [0; 3],
            },
        );
    }
    cache.retain_requests(&HashSet::from(["visible".into(), "new".into()]));
    let key = Key {
        path: "new".into(),
        ..template
    };
    cache.wanted.lock().unwrap().insert(key.clone());
    let (tx, rx) = mpsc::sync_channel(1);
    cache.rx = rx;
    tx.send(Decoded {
        key,
        cancelled: false,
        width: 100,
        height: 100,
        bytes: vec![255; 40_000],
        accent: [0; 3],
    })
    .unwrap();
    cache.drain();
    assert!(cache.entries.contains_key(&visible));
    assert!(!cache.entries.contains_key(&thumb));
    assert_eq!(cache.bytes, 80_000);
}
#[test]
fn latest_player_cover_replaces_queued_priority_cover_and_clear_cancels_it() {
    let mut cache = ArtworkCache::new(1024 * 1024);
    // Stop consumption to inspect the single bounded priority slot.
    cache.stop_worker();
    cache.player_cover("first-original", 340, 6);
    cache.player_cover("second-original", 340, 6);
    assert_eq!(
        cache.priority.lock().unwrap().as_ref().unwrap().path,
        "second-original"
    );
    assert_eq!(cache.pending.len(), 1);
    assert_eq!(cache.wanted.lock().unwrap().len(), 1);
    assert!(cache.deferred.is_empty());
    cache.clear();
    assert!(cache.priority.lock().unwrap().is_none());
    assert!(cache.pending.is_empty());
    assert!(cache.wanted.lock().unwrap().is_empty());
}
#[test]
fn pending_player_cover_reuses_only_current_song_cached_artwork_without_new_jobs() {
    let mut cache = ArtworkCache::new(1024 * 1024);
    for (path, edge) in [
        ("current-thumb", 28),
        ("current-preview", 160),
        ("previous-song", 396),
    ] {
        let key = Key {
            path: path.into(),
            tiles: vec![],
            edge,
            height: 0,
            radius: 0,
            backdrop: false,
            revision: 0,
        };
        let pixels = SharedPixelBuffer::<Rgba8Pixel>::new(edge, edge);
        let bytes = (edge * edge * 4) as usize;
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                image: Image::from_rgba8(pixels),
                bytes,
                used: 0,
                accent: [0; 3],
            },
        );
    }
    let requested = cache.requested_count();
    assert_eq!(
        cache
            .cached_cover(&["current-thumb", "current-preview"])
            .size()
            .width,
        160
    );
    assert_eq!(cache.cached_cover(&["unloaded-song"]).size().width, 0);
    assert_eq!(cache.requested_count(), requested);
}
#[test]
fn invalidation_prevents_old_jobs_from_repopulating_the_cache() {
    let mut cache = ArtworkCache::new(1024);
    cache.get("missing-old.png", 48, false);
    cache.clear();
    std::thread::sleep(std::time::Duration::from_millis(120));
    cache.drain();
    assert!(cache.entries.is_empty());
    assert!(cache.pending.is_empty());
}
