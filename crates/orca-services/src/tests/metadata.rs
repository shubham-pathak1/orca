use super::*;

#[test]
fn cancelled_operations_never_mutate_the_library() {
    let f = Fixture::new();
    let b = f.backend();
    seed(&b);
    let stopped = std::sync::atomic::AtomicBool::new(true);
    let request = operation_types::OperationRequest::from_value(
        serde_json::json!({"action":"collection-edit","kind":"artists","key":"A","name":"Changed"}),
    )
    .unwrap();
    assert!(b
        .execute_cancellable(&request, &stopped)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(b.collection_overrides().unwrap().is_empty());
}

#[test]
fn malformed_collection_override_reports_error_without_changing_playlist() {
    let directory = Fixture::new();
    let backend = directory.backend();
    backend.playlist_action("create", 0, 0, "Original").unwrap();
    let playlist = backend.groups("playlists", "").unwrap().remove(0);
    let key = serde_json::json!(["playlists", playlist.key, ""]).to_string();
    let overrides = serde_json::json!({key: false});
    db::set_setting(
        &backend.conn,
        "collection_overrides",
        &overrides.to_string(),
    )
    .unwrap();
    let result = backend.operation(&serde_json::json!({"action":"collection-edit", "kind":"playlists", "key":playlist.key, "name":"Changed"}).to_string());
    assert!(result.unwrap_err().contains("Invalid collection settings"));
    assert_eq!(
        backend.raw_groups("playlists", "").unwrap()[0].title,
        "Original"
    );
}

#[test]
fn saved_editor_cover_is_embedded_and_invalid_cover_does_not_change_the_file() {
    let f = Fixture::new();
    let b = f.backend();
    let path = f.0.join("editable.wav");
    let samples = vec![0u8; 1600];
    let mut wave = Vec::new();
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wave.extend_from_slice(&1u16.to_le_bytes()); // mono
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wave.extend_from_slice(&samples);
    fs::write(&path, &wave).unwrap();
    let scanned = library::scan_music_file(&path, &f.0.join("artwork")).unwrap();
    assert!(scanned.artwork.is_none());
    db::save_songs_to_db(&b.conn, &[scanned]).unwrap();
    let mut draft = serde_json::json!({"path":path,"title":"Edited title","artist":"Artist","album":"Album","album_artist":"Artist","cover_to_embed":f.0.join("missing.png")});
    let mut invalid_number = draft.clone();
    invalid_number["year"] = serde_json::json!(-1);
    assert!(b
        .save_metadata(&invalid_number.to_string())
        .unwrap_err()
        .starts_with("Invalid year"));
    assert_eq!(fs::read(&path).unwrap(), wave);
    assert!(b.save_metadata(&draft.to_string()).is_err());
    assert_eq!(fs::read(&path).unwrap(), wave);
    let invalid = f.0.join("truncated.png");
    fs::write(
        &invalid,
        &include_bytes!("../../../orca-core/tests/fixtures/cover.png")[..32],
    )
    .unwrap();
    draft["cover_to_embed"] = serde_json::json!(invalid);
    assert!(b.save_metadata(&draft.to_string()).is_err());
    assert_eq!(fs::read(&path).unwrap(), wave);
    let cover = f.0.join("fetched.png");
    fs::write(
        &cover,
        include_bytes!("../../../orca-core/tests/fixtures/cover.png"),
    )
    .unwrap();
    draft["cover_to_embed"] = serde_json::json!(cover);
    b.save_metadata(&draft.to_string()).unwrap();
    // A fresh file scan reads embedded pictures, not album database overrides.
    let saved = library::scan_music_file(&path, &f.0.join("verify-artwork")).unwrap();
    assert_eq!(saved.title, "Edited title");
    assert!(Path::new(saved.artwork.as_ref().unwrap()).is_file());
    assert!(!b
        .track(path.to_str().unwrap())
        .unwrap()
        .artwork_original
        .is_empty());
    draft["cover_to_embed"] = serde_json::Value::Null;
    draft["title"] = serde_json::json!("Edited again");
    b.save_metadata(&draft.to_string()).unwrap();
    let saved = library::scan_music_file(&path, &f.0.join("verify-artwork")).unwrap();
    assert_eq!(saved.title, "Edited again");
    assert!(saved.artwork.is_some());
    db::set_lyrics(
        &b.conn,
        path.to_str().unwrap(),
        "[00:01.00]Cached editor lyrics",
    )
    .unwrap();
    let loaded: serde_json::Value =
        serde_json::from_str(&b.metadata(path.to_str().unwrap()).unwrap()).unwrap();
    assert_eq!(loaded["lyrics"], "[00:01.00]Cached editor lyrics");
    draft["remove_cover"] = serde_json::json!(true);
    draft["lyrics"] = serde_json::json!("New embedded lyrics");
    b.save_metadata(&draft.to_string()).unwrap();
    let saved = library::scan_music_file(&path, &f.0.join("verify-artwork")).unwrap();
    assert!(saved.artwork.is_none());
    db::update_album_artwork(&b.conn, "Artist:Album", Some(cover.to_str().unwrap()), None).unwrap();
    let removed = b.track(path.to_str().unwrap()).unwrap();
    assert!(removed.artwork.is_empty());
    assert!(removed.artwork_thumb.is_empty());
    assert!(removed.artwork_original.is_empty());
    let editor: serde_json::Value =
        serde_json::from_str(&b.metadata(path.to_str().unwrap()).unwrap()).unwrap();
    assert!(editor["artwork"].is_null());
    assert!(
        b.track(path.to_str().unwrap()).unwrap().artwork.is_empty(),
        "rescanning must preserve explicit removal"
    );
    assert_eq!(
        b.lyrics(path.to_str().unwrap()).unwrap(),
        "New embedded lyrics"
    );
    fs::write(path.with_extension("lrc"), "[00:01.00]Old sidecar lyrics").unwrap();
    draft["lyrics"] = serde_json::Value::Null;
    b.save_metadata(&draft.to_string()).unwrap();
    assert!(b.lyrics(path.to_str().unwrap()).unwrap().is_empty());
    assert!(
        b.fetch_missing_lyrics(path.to_str().unwrap())
            .unwrap()
            .is_empty(),
        "manual removal must not trigger online fetching"
    );
    let editor: serde_json::Value =
        serde_json::from_str(&b.metadata(path.to_str().unwrap()).unwrap()).unwrap();
    assert!(editor["lyrics"].is_null());
    let reopened = f.backend();
    assert!(reopened.lyrics(path.to_str().unwrap()).unwrap().is_empty());
    assert!(reopened
        .fetch_missing_lyrics(path.to_str().unwrap())
        .unwrap()
        .is_empty());
    drop(reopened);
    fs::remove_file(path.with_extension("lrc")).unwrap();
    draft["lyrics"] = serde_json::json!("Restored lyrics");
    b.save_metadata(&draft.to_string()).unwrap();
    assert_eq!(b.lyrics(path.to_str().unwrap()).unwrap(), "Restored lyrics");
    assert!(b
        .track(path.to_str().unwrap())
        .unwrap()
        .artwork_original
        .is_empty());

    // A stale editor must never overwrite a change made after it opened.
    let mut stale: serde_json::Value =
        serde_json::from_str(&b.metadata(path.to_str().unwrap()).unwrap()).unwrap();
    draft["title"] = serde_json::json!("External edit");
    b.save_metadata(&draft.to_string()).unwrap();
    let external = fs::read(&path).unwrap();
    stale["title"] = serde_json::json!("Stale edit");
    assert!(b
        .save_metadata(&stale.to_string())
        .unwrap_err()
        .contains("changed since"));
    assert_eq!(fs::read(&path).unwrap(), external);

    // Simulate a database failure after the audio file has committed.
    b.conn.execute_batch("CREATE TRIGGER fail_saved_lyrics BEFORE INSERT ON lyrics BEGIN SELECT RAISE(ABORT,'simulated cache failure'); END;").unwrap();
    draft["title"] = serde_json::json!("Committed before interruption");
    draft["lyrics"] = serde_json::json!("Recovered lyrics");
    let error = b.save_metadata(&draft.to_string()).unwrap_err();
    assert!(error.starts_with("Metadata saved but library update failed"));
    assert!(f.0.join("metadata-save-recovery.json").is_file());
    assert_eq!(
        b.track(path.to_str().unwrap()).unwrap().title,
        "External edit"
    );
    b.conn
        .execute_batch("DROP TRIGGER fail_saved_lyrics;")
        .unwrap();
    drop(b);
    let recovered = f.backend();
    assert_eq!(
        recovered.track(path.to_str().unwrap()).unwrap().title,
        "Committed before interruption"
    );
    assert_eq!(
        recovered.lyrics(path.to_str().unwrap()).unwrap(),
        "Recovered lyrics"
    );
    assert!(!f.0.join("metadata-save-recovery.json").exists());
}
