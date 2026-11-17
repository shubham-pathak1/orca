//! Disposable generated audio only; no user library or external fixtures.
use orca_core::library::{
    read_advanced_metadata, update_song_metadata, AdvancedMetadata, SongMetadataUpdate,
};
use std::{fs, path::Path};
fn samples(path: &Path) -> Vec<u32> {
    rodio::Decoder::try_from(fs::File::open(path).unwrap())
        .unwrap()
        .map(f32::to_bits)
        .collect()
}
#[test]
fn advanced_tags_round_trip_clear_and_survive_basic_edits_without_changing_audio() {
    let directory = std::env::temp_dir().join(format!("orca-advanced-tags-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("generated.wav");
    let pcm: Vec<u8> = (0..800i16)
        .flat_map(|sample| sample.wrapping_mul(41).to_le_bytes())
        .collect();
    let mut wave = Vec::new();
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wave.extend_from_slice(&pcm);
    fs::write(&path, wave).unwrap();
    let before = samples(&path);
    let make_update = |advanced| SongMetadataUpdate {
        path: path.to_str().unwrap().into(),
        title: "Title".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Artist".into(),
        year: Some(2015),
        track_number: None,
        disc_number: None,
        genre: None,
        lyrics: None,
        advanced,
    };
    for advisory in [Some(true), Some(false), None] {
        update_song_metadata(make_update(Some(AdvancedMetadata {
            composer: Some("Composer".into()),
            comment: Some("A note".into()),
            explicit: advisory,
        })))
        .unwrap();
        update_song_metadata(make_update(None)).unwrap();
        let read = read_advanced_metadata(&path).unwrap();
        assert_eq!(read.composer.as_deref(), Some("Composer"));
        assert_eq!(read.comment.as_deref(), Some("A note"));
        assert_eq!(read.explicit, advisory);
        assert_eq!(samples(&path), before);
    }
    update_song_metadata(make_update(Some(AdvancedMetadata::default()))).unwrap();
    let read = read_advanced_metadata(&path).unwrap();
    assert!(read.composer.is_none() && read.comment.is_none() && read.explicit.is_none());
    assert_eq!(samples(&path), before);
    fs::remove_dir_all(directory).unwrap();
}

// One valid mono, 16-bit, verbatim FLAC frame; no external encoder or user audio.
fn generated_flac() -> Vec<u8> {
    let mut flac = b"fLaC".to_vec();
    flac.extend_from_slice(&[0, 0, 0, 34]);
    flac.extend_from_slice(&[0, 16, 0, 16, 0, 0, 0, 0, 0, 0]);
    let stream = (44100u64 << 44) | (15u64 << 36) | 16;
    flac.extend_from_slice(&stream.to_be_bytes());
    flac.extend_from_slice(&[0; 16]);
    flac.extend_from_slice(&[4, 0, 0, 17]);
    flac.extend_from_slice(&9u32.to_le_bytes());
    flac.extend_from_slice(b"Orca test");
    flac.extend_from_slice(&0u32.to_le_bytes());
    flac.extend_from_slice(&[0x81, 0, 0, 64]);
    flac.extend_from_slice(&[0; 64]);
    let mut frame = vec![0xff, 0xf8, 0x69, 0x08, 0, 15];
    let mut crc = 0u8;
    for byte in &frame {
        crc ^= *byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 7
            } else {
                crc << 1
            };
        }
    }
    frame.push(crc);
    frame.push(2);
    for sample in 0..16i16 {
        frame.extend_from_slice(&sample.wrapping_mul(1709).to_be_bytes());
    }
    let mut crc = 0u16;
    for byte in &frame {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    frame.extend_from_slice(&crc.to_be_bytes());
    flac.extend(frame);
    flac
}
#[test]
fn flac_advisory_round_trip_updates_scanner_and_preserves_audio_and_other_tags() {
    let directory = std::env::temp_dir().join(format!("orca-flac-advisory-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("generated.flac");
    fs::write(&path, generated_flac()).unwrap();
    let before = samples(&path);
    assert_eq!(before.len(), 16);
    for explicit in [Some(true), Some(false), None] {
        let update = || SongMetadataUpdate {
            path: path.to_string_lossy().into_owned(),
            title: "Title".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            year: Some(2024),
            track_number: Some(2),
            disc_number: Some(1),
            genre: Some("Pop".into()),
            lyrics: None,
            advanced: Some(AdvancedMetadata {
                composer: Some("Composer".into()),
                comment: Some("Keep this comment".into()),
                explicit,
            }),
        };
        let cover = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cover.png");
        orca_core::library::update_song_metadata_with_cover(update(), Some(&cover)).unwrap();
        {
            use lofty::file::TaggedFileExt;
            let tagged = lofty::probe::Probe::open(&path)
                .unwrap()
                .guess_file_type()
                .unwrap()
                .read()
                .unwrap();
            let picture = tagged.primary_tag().unwrap().pictures().first().unwrap();
            assert_eq!(
                picture.data(),
                fs::read(&cover).unwrap(),
                "the fetched/local cover bytes must be embedded in FLAC"
            );
        }
        let mut basic = update();
        basic.advanced = None;
        basic.title = "Edited title".into();
        update_song_metadata(basic).unwrap();
        let read = read_advanced_metadata(&path).unwrap();
        assert_eq!(read.explicit, explicit);
        assert_eq!(read.comment.as_deref(), Some("Keep this comment"));
        let scanned =
            orca_core::library::scan_music_file(&path, &directory.join("artwork")).unwrap();
        assert_eq!(scanned.explicit, explicit);
        assert_eq!(scanned.composer.as_deref(), Some("Composer"));
        assert_eq!(samples(&path), before);
    }
    fs::remove_dir_all(directory).unwrap();
}
