//! Run with ORCA_METADATA_FIXTURES pointing at Lofty's minimal audio fixtures.
//! https://github.com/Serial-ATA/lofty-rs/tree/main/lofty/tests/files/assets/minimal
use orca_core::library::{scan_music_file, update_song_metadata_cover_action, SongMetadataUpdate};
use std::{fs, path::Path};

fn samples(path: &Path) -> Vec<u32> {
    rodio::Decoder::try_from(fs::File::open(path).unwrap())
        .unwrap_or_else(|e| panic!("decoder for {}: {e}", path.display()))
        .map(f32::to_bits)
        .collect()
}

#[test]
#[ignore = "requires downloaded upstream audio fixtures; never edits user audio"]
fn edits_and_cover_removal_preserve_decoded_audio_across_formats() {
    let fixtures = std::env::var_os("ORCA_METADATA_FIXTURES").expect("set ORCA_METADATA_FIXTURES");
    let directory =
        std::env::temp_dir().join(format!("orca-format-integrity-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for name in [
        "full_test.flac",
        "full_test.mp3",
        "m4a_codec_aac.m4a",
        "m4a_codec_alac.m4a",
        "full_test.ogg",
        "full_test.opus",
        "wav_format_pcm.wav",
        "full_test.aiff",
    ] {
        let path = directory.join(name);
        fs::copy(Path::new(&fixtures).join(name), &path).unwrap();
        // The current playback decoder does not support these codecs. Still
        // verify their tag round trips; do not claim audio decoding coverage.
        let decodable = !matches!(
            name,
            "m4a_codec_alac.m4a" | "full_test.opus" | "full_test.aiff"
        );
        let before = decodable.then(|| samples(&path));
        if let Some(before) = &before {
            assert!(!before.is_empty(), "{name}");
        }
        for remove in [false, true] {
            let update = SongMetadataUpdate {
                path: path.to_str().unwrap().into(),
                title: "Edited title 音楽".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Album artist".into(),
                year: (!remove).then_some(2026),
                track_number: (!remove).then_some(7),
                disc_number: (!remove).then_some(2),
                genre: (!remove).then(|| "Rock".into()),
                lyrics: (!remove).then(|| "[00:01.00]Saved lyrics".repeat(400)),
            };
            let cover = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cover.png");
            update_song_metadata_cover_action(update, (!remove).then_some(cover.as_path()), remove)
                .unwrap_or_else(|e| panic!("{name}, removal={remove}: {e}"));
            let saved = scan_music_file(&path, &directory.join("artwork")).unwrap();
            assert_eq!(saved.title, "Edited title 音楽", "{name}");
            assert_eq!(saved.year, (!remove).then_some(2026), "{name}");
            assert_eq!(saved.track_number, (!remove).then_some(7), "{name}");
            assert_eq!(saved.disc_number, (!remove).then_some(2), "{name}");
            assert_eq!(
                saved.genre.as_deref(),
                (!remove).then_some("Rock"),
                "{name}"
            );
            assert_eq!(saved.artwork.is_none(), remove, "{name}");
            assert_eq!(saved.lyrics.is_none(), remove, "{name}");
            if let Some(before) = &before {
                assert_eq!(
                    &samples(&path),
                    before,
                    "audio changed in {name}, removal={remove}"
                );
            }
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
