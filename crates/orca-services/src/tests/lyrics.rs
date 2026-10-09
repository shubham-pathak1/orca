use super::*;

#[test]
fn playlist_membership_and_local_lyrics_cross_the_boundary() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.playlist_action("create", 0, 0, "Quiet").unwrap();
    let group = b.groups("playlists", "").unwrap().remove(0);
    let id = group.key.parse().unwrap();
    let song = b.track("一.wav").unwrap();
    b.playlist_action("add", id, song.id, "").unwrap();
    let q = ffi::Query {
        kind: "playlists".into(),
        key: group.key,
        ..Default::default()
    };
    assert_eq!(b.page(&q, 0, 128).unwrap().total, 1);
    b.playlist_action("remove", id, song.id, "").unwrap();
    assert_eq!(b.page(&q, 0, 128).unwrap().total, 0);
    assert!(b.playlist_action("create", 0, 0, " ").is_err());
    let song_path = f.0.join("lyrics.wav");
    b.conn
        .execute(
            "INSERT INTO songs(path,title,artist,duration) VALUES(?1,'Lyrics','A',1000)",
            [song_path.to_str().unwrap()],
        )
        .unwrap();
    fs::write(song_path.with_extension("lrc"), "\u{feff}[00:01.00]Hello").unwrap();
    assert_eq!(
        b.lyrics(song_path.to_str().unwrap()).unwrap(),
        "[00:01.00]Hello"
    );
    assert_eq!(
        b.fetch_missing_lyrics(song_path.to_str().unwrap()).unwrap(),
        "[00:01.00]Hello"
    );
    fs::write(song_path.with_extension("lrc"), "").unwrap();
    db::set_lyrics(&b.conn, song_path.to_str().unwrap(), "Cached lyrics").unwrap();
    assert_eq!(
        b.fetch_missing_lyrics(song_path.to_str().unwrap()).unwrap(),
        "Cached lyrics"
    );
    assert!(b.fetch_missing_lyrics("not-in-library.wav").is_err());
}

#[test]
#[ignore = "contacts LRCLIB; uses an isolated database and no audio hardware"]
fn lrclib_fetch_persists_lyrics_for_offline_reuse() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute("INSERT INTO songs(path,title,artist,duration) VALUES('network-lyrics.wav','Lovely Day','Bill Withers',257000)", []).unwrap();
    let lyrics = backend.fetch_missing_lyrics("network-lyrics.wav").unwrap();
    assert!(!lyrics.trim().is_empty());
    assert_eq!(
        db::get_lyrics(&backend.conn, "network-lyrics.wav"),
        Some(lyrics.clone())
    );
    assert_eq!(
        backend.fetch_missing_lyrics("network-lyrics.wav").unwrap(),
        lyrics
    );
}

#[test]
fn automatic_lyrics_reuses_saved_text_without_network() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_lyrics(&b.conn, "two.wav", "Cached lyrics").unwrap();
    assert_eq!(b.fetch_missing_lyrics("two.wav").unwrap(), "Cached lyrics");
}

#[test]
fn imported_lyrics_identify_the_target_and_do_not_modify_saved_lyrics() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_lyrics(&b.conn, "two.wav", "Saved lyrics").unwrap();
    let file = f.0.join("import.lrc");
    fs::write(&file, "\u{feff}[00:01.00]Draft lyrics").unwrap();
    let request = serde_json::json!({"action": "read-lyrics", "key": "two.wav", "file": file});
    let result: serde_json::Value =
        serde_json::from_str(&b.operation(&request.to_string()).unwrap()).unwrap();
    assert_eq!(result["path"], "two.wav");
    assert_eq!(result["lyrics"], "[00:01.00]Draft lyrics");
    assert_eq!(b.lyrics("two.wav").unwrap(), "Saved lyrics");
}
