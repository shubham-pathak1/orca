use super::*;

fn scan(backend: &mut Backend, folder: &Path) {
    backend.start_scan(folder.to_str().unwrap()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = backend.snapshot().unwrap();
        if !snapshot.scanning {
            assert!(snapshot.error.is_empty(), "{}", snapshot.error);
            return;
        }
        assert!(std::time::Instant::now() < deadline, "scan did not finish");
        thread::sleep(Duration::from_millis(5));
    }
}

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

#[cfg(any(unix, windows))]
#[test]
fn rescan_preserves_song_identity_and_user_data_after_rename() {
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    let folder = fixture.0.join("music");
    fs::create_dir(&folder).unwrap();
    let original = folder.join("original.mp3");
    let renamed = folder.join("renamed.mp3");
    fs::write(&original, b"test audio").unwrap();
    scan(&mut backend, &folder);
    let song = db::get_all_songs(&backend.conn).unwrap().pop().unwrap();
    let id = song.id.unwrap();
    let playlist = db::create_playlist(&backend.conn, "Keep this song", None).unwrap();
    db::add_to_playlist(&backend.conn, playlist, id).unwrap();
    db::set_song_liked(&backend.conn, id, true).unwrap();
    db::set_lyrics(&backend.conn, &song.path, "Saved lyrics").unwrap();
    db::save_waveform(&backend.conn, &song.path, 1, &[0.5]).unwrap();

    fs::copy(&original, folder.join("copy.mp3")).unwrap();
    fs::rename(&original, &renamed).unwrap();
    scan(&mut backend, &folder);
    let songs = db::get_all_songs(&backend.conn).unwrap();
    assert_eq!(songs.len(), 2);
    let song = songs.iter().find(|song| song.id == Some(id)).unwrap();
    assert!(Path::new(&song.path).ends_with("renamed.mp3"));
    assert!(db::is_song_liked(&backend.conn, id).unwrap());
    assert_eq!(
        db::get_song_playlist_ids(&backend.conn, id).unwrap(),
        [playlist]
    );
    assert_eq!(
        db::get_lyrics(&backend.conn, &song.path).as_deref(),
        Some("Saved lyrics")
    );
    assert_eq!(
        db::get_cached_waveform(&backend.conn, &song.path, 1).unwrap(),
        Some(vec![0.5])
    );

    #[cfg(windows)]
    {
        let capitalized = folder.join("Renamed.mp3");
        fs::rename(&renamed, &capitalized).unwrap();
        scan(&mut backend, &folder);
        let songs = db::get_all_songs(&backend.conn).unwrap();
        assert_eq!(songs.len(), 2);
        let song = songs.iter().find(|song| song.id == Some(id)).unwrap();
        assert!(song.path.ends_with("Renamed.mp3"));
    }
}

#[cfg(any(unix, windows))]
#[test]
fn rescanning_the_source_before_the_destination_preserves_moved_song_collections() {
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    let source = fixture.0.join("source");
    let destination = fixture.0.join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let old_path = source.join("track.mp3");
    let new_path = destination.join("track.mp3");
    fs::write(&old_path, b"test audio").unwrap();
    scan(&mut backend, &source);
    scan(&mut backend, &destination);

    let id = db::get_all_songs(&backend.conn).unwrap()[0].id.unwrap();
    db::set_song_liked(&backend.conn, id, true).unwrap();
    let playlist = db::create_playlist(&backend.conn, "Moved tracks", None).unwrap();
    db::add_to_playlist(&backend.conn, playlist, id).unwrap();
    fs::rename(&old_path, &new_path).unwrap();
    scan(&mut backend, &source);
    assert_eq!(backend.statistics().unwrap().songs, 0);
    assert_eq!(db::liked_songs_summary(&backend.conn).unwrap().count, 0);
    drop(backend);

    let mut backend = fixture.backend();
    scan(&mut backend, &destination);
    assert_eq!(backend.statistics().unwrap().songs, 1);
    assert_eq!(
        backend
            .id_for_path(&new_path.canonicalize().unwrap().to_string_lossy())
            .unwrap(),
        Some(id)
    );
    assert!(db::is_song_liked(&backend.conn, id).unwrap());
    assert_eq!(
        db::get_song_playlist_ids(&backend.conn, id).unwrap(),
        [playlist]
    );
}
