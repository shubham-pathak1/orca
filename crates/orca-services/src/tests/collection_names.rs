use super::*;

fn track(b: &Backend, f: &Fixture, number: u8, album: &str) -> PathBuf {
    let path = f.0.join(format!("track-{number}.wav"));
    let mut wave = b"RIFF".to_vec();
    wave.extend_from_slice(&1636u32.to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&1600u32.to_le_bytes());
    wave.extend_from_slice(&[0; 1600]);
    fs::write(&path, wave).unwrap();
    let song = library::scan_music_file(&path, &f.0.join("artwork")).unwrap();
    db::save_songs_to_db(&b.conn, &[song]).unwrap();
    let cover = f.0.join(format!("cover-{number}.png"));
    image::RgbaImage::from_pixel(16, 16, image::Rgba([number, 100, 200, 255]))
        .save(&cover)
        .unwrap();
    b.save_metadata(
        &serde_json::json!({"path":path,"title":format!("Track {number}"),
        "artist":"Artist","album_artist":"Artist","album":album,"genre":"Rock",
        "lyrics":"[00:01.00]Original lyrics","cover_to_embed":cover})
        .to_string(),
    )
    .unwrap();
    path
}

#[test]
fn collection_names_write_tags_preserve_track_covers_lyrics_and_playlist_membership() {
    let f = Fixture::new();
    let b = f.backend();
    let a = track(&b, &f, 1, "Album");
    let c = track(&b, &f, 2, "Album");
    let other = track(&b, &f, 3, "Other");
    let original = b.track(a.to_str().unwrap()).unwrap();
    let second_cover = b.track(c.to_str().unwrap()).unwrap().artwork_original;
    let playlist = db::create_playlist(&b.conn, "Keep", None).unwrap();
    db::add_to_playlist(&b.conn, playlist, original.id).unwrap();
    db::update_album_artwork(
        &b.conn,
        "Artist:Album",
        Some("album.png"),
        Some("album-small.png"),
    )
    .unwrap();
    b.operation(r#"{"action":"collection-edit","kind":"albums","key":"Album","secondary":"Artist","name":"Renamed"}"#).unwrap();
    assert_eq!(b.track(a.to_str().unwrap()).unwrap().album, "Renamed");
    assert_eq!(b.track(other.to_str().unwrap()).unwrap().album, "Other");
    let raw = library::scan_music_file(&a, &f.0.join("artwork")).unwrap();
    assert_eq!(raw.album, "Renamed");
    assert_eq!(raw.lyrics.as_deref(), Some("[00:01.00]Original lyrics"));
    assert_eq!(
        raw.artwork.as_deref(),
        Some(original.artwork_original.as_str())
    );
    assert_eq!(
        b.track(c.to_str().unwrap()).unwrap().artwork_original,
        second_cover
    );
    b.operation(
        r#"{"action":"collection-edit","kind":"artists","key":"Artist","name":"New artist"}"#,
    )
    .unwrap();
    let raw = library::scan_music_file(&a, &f.0.join("artwork")).unwrap();
    assert_eq!(raw.artist, "New artist");
    assert_eq!(raw.album_artist, "New artist");
    assert_eq!(
        db::get_playlist_export_songs(&b.conn, playlist)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(b.track(a.to_str().unwrap()).unwrap().id, original.id);
    assert!(b
        .groups("albums", "")
        .unwrap()
        .iter()
        .any(|g| g.key == "Renamed" && g.artwork == "album.png"));
    b.operation(r#"{"action":"collection-edit","kind":"genres","key":"Rock","name":"Pop"}"#)
        .unwrap();
    assert_eq!(
        library::scan_music_file(&a, &f.0.join("artwork"))
            .unwrap()
            .genre
            .as_deref(),
        Some("Pop")
    );
    drop(b);
    let reopened = f.backend();
    assert_eq!(
        reopened.track(a.to_str().unwrap()).unwrap().album,
        "Renamed"
    );
}

#[test]
fn collection_preflight_refuses_missing_and_read_only_sources_before_any_write() {
    let f = Fixture::new();
    let b = f.backend();
    let a = track(&b, &f, 1, "Album");
    let c = track(&b, &f, 2, "Album");
    let before = fs::read(&a).unwrap();
    let request = r#"{"action":"collection-edit","kind":"albums","key":"Album","secondary":"Artist","name":"Renamed"}"#;
    let mut permissions = fs::metadata(&c).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&c, permissions).unwrap();
    assert!(b.operation(request).unwrap_err().contains("read-only"));
    assert_eq!(fs::read(&a).unwrap(), before);
    let mut permissions = fs::metadata(&c).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&c, permissions).unwrap();
    fs::remove_file(&c).unwrap();
    assert!(b.operation(request).unwrap_err().contains("unavailable"));
    assert_eq!(fs::read(&a).unwrap(), before);
    assert!(b.collection_overrides().unwrap().is_empty());
}
