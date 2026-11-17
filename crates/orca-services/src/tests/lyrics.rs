use super::*;

#[test]
fn playlist_membership_and_local_lyrics_cross_the_boundary() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.playlist_action("create", 0, 0, "Quiet").unwrap();
    let group = user_playlist(&b);
    let id = group.key.parse().unwrap();
    let song = b.track("一.wav").unwrap();
    b.playlist_action("add", id, song.id, "").unwrap();
    let q = types::Query {
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
#[ignore = "contacts online lyrics providers; uses an isolated database and no audio hardware"]
fn online_fetch_is_available_without_writing_to_the_library() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    backend.conn.execute("INSERT INTO songs(path,title,artist,duration) VALUES('network-lyrics.wav','Lovely Day','Bill Withers',257000)", []).unwrap();
    let lyrics = backend.fetch_missing_lyrics("network-lyrics.wav").unwrap();
    assert!(!lyrics.trim().is_empty());
    assert_eq!(db::get_lyrics(&backend.conn, "network-lyrics.wav"), None);
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
fn legacy_empty_lyrics_cache_does_not_block_automatic_lookup() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_lyrics(&b.conn, "two.wav", "").unwrap();
    let fetched = b
        .fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            Ok("[00:01.00]Found online".into())
        })
        .unwrap();
    assert_eq!(fetched, "[00:01.00]Found online");
    assert_eq!(db::get_lyrics(&b.conn, "two.wav"), Some(String::new()));
    assert!(!db::lyrics_suppressed(&b.conn, "two.wav"));
}

#[test]
fn automatic_lyrics_are_reused_only_in_the_current_session() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let first = b
        .fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            Ok("Plain online lyrics".into())
        })
        .unwrap();
    assert_eq!(first, "Plain online lyrics");
    assert_eq!(b.lyrics("two.wav").unwrap(), first);
    assert_eq!(db::get_lyrics(&b.conn, "two.wav"), None);
    assert_eq!(
        b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            panic!("same-session lookup should use memory")
        })
        .unwrap(),
        first
    );
    drop(b);
    let reopened = f.backend();
    assert_eq!(reopened.lyrics("two.wav").unwrap(), "");
    assert_eq!(
        reopened
            .fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
                Ok("[00:01.00]New synced lyrics".into())
            })
            .unwrap(),
        "[00:01.00]New synced lyrics"
    );
}

#[test]
fn explicit_clear_and_manual_save_override_session_lyrics() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| Ok("Online lyrics".into()))
        .unwrap();
    db::set_lyrics_with_suppression(&b.conn, "two.wav", "", true).unwrap();
    assert_eq!(b.lyrics("two.wav").unwrap(), "");
    assert_eq!(
        b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            panic!("explicit clear must not fetch")
        })
        .unwrap(),
        ""
    );
    // Older builds could write an automatic result while suppression was set.
    db::set_lyrics_with_suppression(&b.conn, "two.wav", "Old online result", true).unwrap();
    assert_eq!(b.lyrics("two.wav").unwrap(), "");
    db::set_lyrics(&b.conn, "two.wav", "Manually saved lyrics").unwrap();
    assert_eq!(b.lyrics("two.wav").unwrap(), "Manually saved lyrics");
}

#[test]
fn missing_lyrics_are_retried_in_the_next_session() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    assert_eq!(
        b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            Err("Lyrics not found".into())
        })
        .unwrap(),
        ""
    );
    b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
        panic!("not-found should be remembered for this session")
    })
    .unwrap();
    drop(b);
    let reopened = f.backend();
    assert_eq!(
        reopened
            .fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
                Ok("Added later".into())
            })
            .unwrap(),
        "Added later"
    );
}

#[test]
fn retagging_a_song_invalidates_its_session_lookup() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
        Ok("Lyrics for old title".into())
    })
    .unwrap();
    b.conn
        .execute("UPDATE songs SET title='Retagged' WHERE path='two.wav'", [])
        .unwrap();
    assert_eq!(b.lyrics("two.wav").unwrap(), "");
    assert_eq!(
        b.fetch_missing_lyrics_with("two.wav", None, |title, _, _, _, _| {
            assert_eq!(title, "Retagged");
            Ok("Lyrics for new title".into())
        })
        .unwrap(),
        "Lyrics for new title"
    );
}

#[test]
fn refresh_bypasses_legacy_cache_without_writing_it_and_reuses_result_this_session() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_lyrics(&b.conn, "two.wav", "Old plain lyrics").unwrap();
    assert_eq!(
        b.fetch_missing_lyrics("two.wav").unwrap(),
        "Old plain lyrics"
    );
    let fresh = b
        .refresh_lyrics_with("two.wav", None, |_, _, _, _, _| {
            Ok("[00:01.00]New synced lyrics".into())
        })
        .unwrap();
    assert_eq!(fresh, "[00:01.00]New synced lyrics");
    assert_eq!(b.lyrics("two.wav").unwrap(), fresh);
    assert_eq!(
        b.fetch_missing_lyrics_with("two.wav", None, |_, _, _, _, _| {
            panic!("refreshed lyrics should be reused during this session")
        })
        .unwrap(),
        fresh
    );
    assert_eq!(
        db::get_lyrics(&b.conn, "two.wav"),
        Some("Old plain lyrics".into())
    );
    drop(b);
    assert_eq!(f.backend().lyrics("two.wav").unwrap(), "Old plain lyrics");
}

#[test]
fn refresh_failure_keeps_saved_lyrics_and_later_user_edit_wins() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    db::set_lyrics(&b.conn, "two.wav", "Saved lyrics").unwrap();
    assert!(b
        .refresh_lyrics_with("two.wav", None, |_, _, _, _, _| {
            Err("Lyrics not found".into())
        })
        .is_err());
    assert_eq!(b.lyrics("two.wav").unwrap(), "Saved lyrics");
    b.refresh_lyrics_with("two.wav", None, |_, _, _, _, _| Ok("New online".into()))
        .unwrap();
    db::set_lyrics(&b.conn, "two.wav", "New manual lyrics").unwrap();
    assert_eq!(b.lyrics("two.wav").unwrap(), "New manual lyrics");
}

#[test]
fn cancelled_or_retagged_refresh_cannot_replace_the_current_song() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let stop = AtomicBool::new(false);
    assert!(b
        .refresh_lyrics_with("two.wav", Some(&stop), |_, _, _, _, _| {
            stop.store(true, Ordering::Relaxed);
            Ok("Late lyrics".into())
        })
        .is_err());
    assert_eq!(b.lyrics("two.wav").unwrap(), "");
    assert!(b
        .refresh_lyrics_with("two.wav", None, |_, _, _, _, _| {
            b.conn
                .execute("UPDATE songs SET title='Retagged' WHERE path='two.wav'", [])
                .unwrap();
            Ok("Old title lyrics".into())
        })
        .is_err());
    assert_eq!(b.lyrics("two.wav").unwrap(), "");
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
