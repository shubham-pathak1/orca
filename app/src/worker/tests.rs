use super::*;
use orca_services::types::Snapshot;

#[test]
fn metadata_failure_keeps_its_editor_generation_and_never_reports_success() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../target/slint/metadata-failure-{}",
        std::process::id()
    ));
    let service = Service::start(directory, false);
    service
        .tx
        .send(Request::Metadata(crate::protocol::MetadataRequest::Load {
            path: "absent.flac".into(),
            fetch_cover: false,
            generation: 42,
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        assert!(
            Instant::now() < deadline,
            "metadata failure must be delivered promptly"
        );
        match service.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::MetadataFailed(at, error)) => {
                assert_eq!(at, 42);
                assert!(!error.is_empty());
                break;
            }
            Ok(Event::Metadata(..) | Event::MetadataSaved(..)) => {
                panic!("failed load cannot complete successfully")
            }
            _ => {}
        }
    }
}
#[test]
fn failed_operation_is_reported_as_failure_without_success_event() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../target/slint/operation-failure-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let service = Service::start(directory, false);
    let request=OperationRequest::from_value(serde_json::json!({"action":"collection-edit","kind":"albums","key":"Missing","name":"Changed"})).unwrap();
    service
        .tx
        .send(Request::Operation(request.clone()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        assert!(
            Instant::now() < deadline,
            "operation should report a failure promptly"
        );
        match service.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::OperationFailed(actual, error)) if actual == request => {
                assert!(error.to_string().contains("no longer exists"));
                break;
            }
            Ok(Event::Operation(actual, _)) if actual == request => {
                panic!("failure must not become a successful empty response")
            }
            _ => {}
        }
    }
    drop(service);
}
#[test]
fn audio_errors_report_once_per_attempt_and_reappear_after_recovery() {
    let mut reporter = PlaybackErrors::default();
    let mut snapshot = Snapshot {
        playback_error: "playback:play:bad file".into(),
        playback_error_revision: 1,
        output_status: r#"{"error":"no device"}"#.into(),
        ..Default::default()
    };
    assert_eq!(reporter.collect(&snapshot).len(), 2);
    assert!(reporter.collect(&snapshot).is_empty());
    snapshot.playback_error_revision = 2;
    assert_eq!(reporter.collect(&snapshot), ["playback:play:bad file"]);
    snapshot.playback_error.clear();
    snapshot.output_status = r#"{"error":""}"#.into();
    assert!(reporter.collect(&snapshot).is_empty());
    snapshot.output_status = r#"{"error":"no device"}"#.into();
    assert_eq!(reporter.collect(&snapshot), ["output:no device"]);
}
#[test]
fn rejected_play_preserves_context_history_and_pending_target() {
    let mut navigation = Navigation::default();
    navigation.context(vec!["a".into(), "b".into()], "a");
    navigation.manual.push("missing".into());
    let original = serde_json::to_value(&navigation).unwrap();
    let mut requested = "a".to_string();
    let mut planned = navigation.clone();
    let target = planned.next("a", false, false).unwrap();
    assert!(
        submit_play(&mut navigation, &mut requested, planned, target, |_| Err(
            "Audio file no longer exists".into()
        ))
        .is_err()
    );
    assert_eq!(serde_json::to_value(&navigation).unwrap(), original);
    assert_eq!(requested, "a");
    let planned = navigation.clone();
    submit_play(&mut navigation, &mut requested, planned, "b".into(), |_| {
        Ok(())
    })
    .unwrap();
    assert_eq!(requested, "b");
    assert_eq!(navigation.history.last().map(String::as_str), Some("b"));
}
#[test]
fn cancelling_gapless_discards_old_plan_only_after_audio_accepts_it() {
    let mut preloaded = "old-next".to_string();
    let mut planned = Some(Navigation::default());
    assert!(cancel_preload(&mut preloaded, &mut planned, || Err("disconnected".into())).is_err());
    assert_eq!(preloaded, "old-next");
    assert!(planned.is_some());
    cancel_preload(&mut preloaded, &mut planned, || Ok(())).unwrap();
    assert!(preloaded.is_empty());
    assert!(planned.is_none());
    cancel_preload(&mut preloaded, &mut planned, || panic!("nothing queued")).unwrap();
}
#[test]
fn newest_analysis_request_replaces_pending_older_requests() {
    let (jobs, receiver) = latest_channel();
    jobs.submit((1, "first"));
    jobs.submit((2, "second"));
    jobs.submit((3, "latest"));
    assert_eq!(receiver.receive(), Some((3, "latest")));
    assert!(receiver.pending.lock().unwrap().is_none());
}
#[test]
fn watched_library_reconciles_offline_deletions_live_restores_and_source_removal() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../target/slint/watch-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = directory.join("music");
    fs::create_dir_all(&root).unwrap();
    let song = root.join("song.wav");
    let samples = vec![0u8; 1600];
    let mut wave = Vec::new();
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wave.extend_from_slice(&samples);
    fs::write(&song, &wave).unwrap();
    {
        let mut backend = orca_services::new_backend(directory.to_str().unwrap(), false).unwrap();
        backend.start_scan(root.to_str().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(6);
        while backend.snapshot().unwrap().scanning {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(backend.statistics().unwrap().songs, 1);
        backend.shutdown();
    }
    fs::remove_file(&song).unwrap();
    let service = Service::start(directory.clone(), false);
    fn wait(service: &Service, wanted: u64) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            assert!(
                Instant::now() < deadline,
                "library did not reconcile to {wanted}"
            );
            match service.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Event::Statistics(stats)) if stats.songs as u64 == wanted => return,
                Ok(Event::Error(error)) => panic!("{error}"),
                _ => {}
            }
        }
    }
    wait(&service, 0);
    fs::write(&song, &wave).unwrap();
    wait(&service, 1);
    fs::remove_file(&song).unwrap();
    wait(&service, 0);
    fs::write(&song, &wave).unwrap();
    wait(&service, 1);
    let backend = orca_services::new_backend(directory.to_str().unwrap(), false).unwrap();
    let root = backend.statistics().unwrap().roots.remove(0);
    drop(backend);
    service
        .tx
        .send(Request::Operation(OperationRequest::RemoveSource {
            key: root,
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < deadline);
        match service.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Operation(request, _)) if request.removes_source() => {
                service.tx.send(Request::SourceChanged).unwrap();
                break;
            }
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    wait(&service, 0);
    drop(service);
    let backend = orca_services::new_backend(directory.to_str().unwrap(), false).unwrap();
    assert!(backend.statistics().unwrap().roots.is_empty());
    assert_eq!(backend.statistics().unwrap().songs, 0);
    drop(backend);
    fs::remove_file(&song).unwrap();
    fs::remove_dir(directory.join("music")).unwrap();
}

#[test]
fn collection_header_is_delivered_before_its_song_page_without_playback() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../target/slint/header-service-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let backend = new_backend(directory.to_str().unwrap(), false).unwrap();
    backend
        .playlist_action("create", 0, 0, "Header fixture")
        .unwrap();
    let group = backend
        .groups("playlists", "")
        .unwrap()
        .into_iter()
        .find(|group| group.title == "Header fixture")
        .unwrap();
    let cover = directory.join("cover.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([120, 160, 200, 255]))
        .save(&cover)
        .unwrap();
    backend.operation(&serde_json::json!({"action":"collection-edit","kind":"playlists","key":group.key,"cover":cover}).to_string()).unwrap();
    drop(backend);
    let service = Service::start(directory.clone(), false);
    service.latest_query.store(1, Ordering::Relaxed);
    service
        .tx
        .send(Request::Browse(
            1,
            Query {
                kind: "playlists".into(),
                key: group.key.clone(),
                ..Default::default()
            },
            "playlists".into(),
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut header = false;
    loop {
        assert!(Instant::now() < deadline, "collection data did not arrive");
        match service.rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Operation(
                OperationRequest::GroupDetail(_),
                OperationResult::Detail(result),
            )) => {
                assert_eq!(result.title, "Header fixture");
                assert_eq!(result.count, 0);
                assert!(!result.artwork.as_deref().unwrap().is_empty());
                assert!(!result.backdrop.as_deref().unwrap().is_empty());
                header = true;
            }
            Ok(Event::Page(1, 0, 0, _)) => {
                assert!(header, "local header must not wait on the operation worker");
                break;
            }
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    let started = Instant::now();
    drop(service);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "local read must not interfere with shutdown"
    );
}

#[test]
fn no_audio_service_queries_and_shuts_down_without_audio_device_access() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../target/slint/service-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let service = Service::start(directory.clone(), false);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < deadline, "service did not initialize");
        match service.rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Ready) => break,
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    service.latest_query.store(1, Ordering::Relaxed);
    service
        .tx
        .send(Request::Browse(1, Query::default(), "songs".into()))
        .unwrap();
    loop {
        assert!(
            Instant::now() < deadline,
            "empty library query did not finish"
        );
        match service.rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Page(1, 0, 0, rows)) => {
                assert!(rows.is_empty());
                break;
            }
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    // The picker result carries its original target so a subsequently opened
    // editor cannot accidentally adopt this song's artwork or lyrics.
    fs::write(
        directory.join("selected.lrc"),
        "\u{feff}[00:01.00]Selected lyrics",
    )
    .unwrap();
    service
        .tx
        .send(Request::Metadata(
            crate::protocol::MetadataRequest::SelectAsset {
                kind: crate::protocol::MetadataAssetKind::Lyrics,
                target: "first.flac".into(),
                file: directory.join("selected.lrc").to_string_lossy().into(),
                generation: 1,
            },
        ))
        .unwrap();
    loop {
        assert!(
            Instant::now() < deadline,
            "selected lyrics were not delivered"
        );
        match service.rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::MetadataAsset(action, target, text, generation)) => {
                assert_eq!(action, crate::protocol::MetadataAssetKind::Lyrics);
                assert_eq!(generation, 1);
                assert_eq!(target, "first.flac");
                assert_eq!(text, "[00:01.00]Selected lyrics");
                break;
            }
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    let first = directory.join("first-folder");
    let second = directory.join("second-folder");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    service
        .tx
        .send(Request::Folder(first.to_string_lossy().into()))
        .unwrap();
    service
        .tx
        .send(Request::Folder(second.to_string_lossy().into()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(
            Instant::now() < deadline,
            "queued folder scans did not both complete"
        );
        match service.rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Statistics(stats))
                if stats
                    .roots
                    .iter()
                    .any(|root| root.ends_with("first-folder"))
                    && stats
                        .roots
                        .iter()
                        .any(|root| root.ends_with("second-folder")) =>
            {
                break
            }
            Ok(Event::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    let shutdown = Instant::now();
    drop(service);
    assert!(
        shutdown.elapsed() < Duration::from_secs(2),
        "idle workers failed to stop promptly"
    );
    assert!(
        !directory.join("slint-session.json").exists(),
        "no-audio validation must not write playback state"
    );
}
#[test]
fn catalog_letters_find_collection_titles_in_display_order() {
    let groups: Vec<_> = ["Zulu", "Alpha", "beta", "Delta"]
        .into_iter()
        .map(|title| Group {
            title: title.into(),
            ..Default::default()
        })
        .collect();
    assert_eq!(catalog_letter_index(&groups, "B"), 2);
    assert_eq!(catalog_letter_index(&groups, "C"), 3);
    assert_eq!(catalog_letter_index(&groups, "#"), 0);
    assert_eq!(catalog_letter_index(&[], "A"), 0);
}
#[test]
fn older_settings_keep_visual_defaults_and_minimal_options_round_trip() {
    let old: Settings =
        serde_json::from_str(r#"{"grid":false,"font_family":"Custom","animated_lyrics":true}"#)
            .unwrap();
    assert!(old.browse_artwork && old.compact_artwork && old.full_artwork);
    assert!(!old.light_theme);
    assert_eq!(old.font_family, "Custom");
    let mut minimal = old;
    minimal.browse_artwork = false;
    minimal.compact_artwork = false;
    minimal.full_artwork = false;
    minimal.blurred_background = false;
    minimal.light_theme = true;
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&minimal).unwrap()).unwrap();
    assert!(
        !restored.browse_artwork
            && !restored.compact_artwork
            && !restored.full_artwork
            && !restored.blurred_background
    );
    assert_eq!(restored.font_family, "Custom");
    assert!(restored.light_theme);
    assert!(serde_json::to_value(&restored)
        .unwrap()
        .get("animated_lyrics")
        .is_none());
}
#[test]
fn restored_session_preserves_position_volume_queue_and_shuffle_choice() {
    let empty: Session = serde_json::from_str("{}").unwrap();
    assert_eq!(empty.volume, 1.0);
    let paused_empty: Session = serde_json::from_str(r#"{"volume":0.25}"#).unwrap();
    assert!(paused_empty.path.is_empty());
    assert_eq!(paused_empty.volume, 0.25);
    let mut navigation = Navigation::default();
    navigation.context(vec!["a".into(), "b".into(), "c".into()], "a");
    navigation.shuffle = true;
    navigation.repeat = 1;
    navigation.record("b");
    navigation.manual.push("outside".into());
    navigation.remove("b", "c");
    let session = Session {
        path: "b".into(),
        position: 12345,
        volume: 0.65,
        navigation,
    };
    let mut restored: Session =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    assert_eq!(restored.path, "b");
    assert_eq!(restored.position, 12345);
    assert_eq!(restored.volume, 0.65);
    assert_eq!(
        restored.navigation.upcoming("b"),
        session.navigation.upcoming("b")
    );
    let mut original = session.navigation;
    for _ in 0..4 {
        let current = restored.path.clone();
        let expected = original.next(&current, false, true);
        let next = restored.navigation.next(&current, false, true);
        assert_eq!(next, expected);
        if let Some(next) = next {
            original.record(&next);
            restored.navigation.record(&next);
            restored.path = next;
        }
    }
}
#[test]
fn closing_before_slider_commands_are_processed_saves_the_last_valid_volume() {
    let (tx, rx) = mpsc::channel();
    tx.send(Request::Volume(0.35)).unwrap();
    tx.send(Request::Command(PlayerAction::Pause)).unwrap();
    tx.send(Request::Volume(0.2)).unwrap();
    tx.send(Request::Shutdown).unwrap();
    tx.send(Request::Volume(f64::NAN)).unwrap();
    assert_eq!(shutdown_volume(&rx, 1.0), 0.2);
    assert_eq!(shutdown_volume(&rx, 0.2), 0.2);
}
#[test]
fn settings_updates_replace_existing_file() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/slint/settings-test");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    write_json(&path, &Settings::default()).unwrap();
    let settings = Settings {
        grid: false,
        ..Default::default()
    };
    write_json(&path, &settings).unwrap();
    assert!(!read_json::<Settings>(&path).unwrap().grid);
}
