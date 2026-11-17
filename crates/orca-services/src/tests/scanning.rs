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
    let folder_revision = db::folder_cache_revision(&b.conn).unwrap();
    b.start_scan(folder.to_str().unwrap()).unwrap();
    assert_eq!(db::folder_cache_revision(&b.conn).unwrap(), folder_revision);
    while b.snapshot().unwrap().scanning {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(b.statistics().unwrap().songs, 100);
    assert_eq!(db::folder_cache_revision(&b.conn).unwrap(), folder_revision);
    assert!(b.start_scan("does-not-exist").is_err());
}
