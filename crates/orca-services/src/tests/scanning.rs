use super::*;

#[test]
fn scan_persists_tracks_and_shutdown_joins_cancelled_worker() {
    let f = Fixture::new();
    let mut b = f.backend();
    let folder = f.0.join("music");
    fs::create_dir(&folder).unwrap();
    for i in 0..100 {
        fs::write(folder.join(format!("song{i}.mp3")), b"tagless").unwrap();
    }
    b.start_scan(folder.to_str().unwrap()).unwrap();
    assert!(b.start_scan(folder.to_str().unwrap()).is_err());
    b.shutdown();
    assert!(!b.snapshot().unwrap().scanning);
    b.start_scan(folder.to_str().unwrap()).unwrap();
    while b.snapshot().unwrap().scanning {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(b.statistics().unwrap().songs, 100);
    assert_eq!(b.statistics().unwrap().roots.len(), 1);
    b.start_scan(folder.to_str().unwrap()).unwrap();
    while b.snapshot().unwrap().scanning {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(b.statistics().unwrap().songs, 100);
    assert!(b.start_scan("does-not-exist").is_err());
}

#[test]
#[ignore = "requires an ORCA_STRESS_LIBRARY fixture; uses a disposable database"]
fn generated_stress_library_scans_and_rescans_without_duplicates() {
    let root = std::env::var("ORCA_STRESS_LIBRARY").expect("set ORCA_STRESS_LIBRARY");
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(std::path::Path::new(&root).join("stress-library.json")).unwrap(),
    )
    .unwrap();
    let expected = report["tracks"].as_u64().unwrap();
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    for pass in 1..=2 {
        let started = std::time::Instant::now();
        backend.start_scan(&root).unwrap();
        while backend.snapshot().unwrap().scanning {
            assert!(
                started.elapsed() < Duration::from_secs(600),
                "scan exceeded ten minutes"
            );
            thread::sleep(Duration::from_millis(50));
        }
        let stats = backend.statistics().unwrap();
        assert_eq!(stats.songs as u64, expected);
        let page = backend
            .page(&crate::types::Query::default(), 0, 20)
            .unwrap()
            .tracks;
        assert!(!page.is_empty());
        assert!(page
            .iter()
            .all(|track| track.title.contains("Stress track") && track.duration_ms > 0));
        assert!(page
            .iter()
            .any(|track| track.artist.contains("Stress artist")));
        eprintln!(
            "Generated WAV scan pass {pass}: {expected} tracks in {:?}",
            started.elapsed()
        );
    }
    backend.shutdown();
}
