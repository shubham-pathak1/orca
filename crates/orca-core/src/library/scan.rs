use super::LocalSong;
use crate::artwork_cache::persist_artwork;
use lofty::{
    file::{AudioFile, TaggedFileExt},
    prelude::*,
    probe::Probe,
};
use std::{fs, path::Path};

fn normalize_text(value: Option<String>, unknown: &str) -> String {
    let raw = value.unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return unknown.to_string();
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower == "unknown"
        || lower == "none"
        || lower == "null"
        || lower == "n/a"
        || lower == "na"
        || lower == "-"
        || lower == "?"
    {
        return unknown.to_string();
    }

    trimmed.to_string()
}

fn parse_tag_i32(value: Option<&str>) -> Option<i32> {
    let text = value?.trim();
    if text.is_empty() {
        return None;
    }
    let first = text.split('/').next().unwrap_or(text).trim();
    first.parse::<i32>().ok().filter(|n| *n > 0)
}

pub(super) fn explicit_advisory(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "1" | "4" | "true" | "yes" | "explicit" => Some(true),
        "2" | "false" | "no" | "clean" | "inoffensive" => Some(false),
        _ => None,
    }
}

pub fn scan_music_file(path: &Path, artwork_dir: &Path) -> Result<LocalSong, String> {
    scan_music_file_with_metadata(path, artwork_dir, |_| {})
}

pub fn scan_music_file_with_metadata<F>(
    path: &Path,
    artwork_dir: &Path,
    on_metadata: F,
) -> Result<LocalSong, String>
where
    F: Fn(&LocalSong),
{
    let tagged_file = match Probe::open(path)
        .map_err(|e| e.to_string())
        .and_then(|p| p.read().map_err(|e| e.to_string()))
    {
        Ok(tf) => tf,
        Err(_) => {
            // File couldn't be parsed by lofty (e.g. truly tagless or malformed).
            // Still index it using the filename so it shows up in the library.
            let title = normalize_text(
                path.file_stem().map(|f| f.to_string_lossy().to_string()),
                "Unknown Title",
            );
            let file_metadata = fs::metadata(path).ok();
            let file_size = file_metadata.as_ref().map(|m| m.len());
            let modified_at = file_metadata
                .as_ref()
                .and_then(crate::scanner::modification_time);
            let format = path
                .extension()
                .map(|e| e.to_string_lossy().to_uppercase().to_string());
            let song = LocalSong {
                id: None,
                path: path.to_string_lossy().to_string(),
                title,
                artist: "Unknown Artist".to_string(),
                album_artist: "Unknown Artist".to_string(),
                album: "Unknown Album".to_string(),
                year: None,
                track_number: None,
                disc_number: None,
                genre: None,
                explicit: None,
                composer: None,
                duration: 0,
                artwork: None,
                artwork_thumb: None,
                artwork_preview: None,
                lyrics: None,
                sample_rate: None,
                bitrate: None,
                bit_depth: None,
                format,
                modified_at,
                file_size,
            };
            on_metadata(&song);
            return Ok(song);
        }
    };

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();
    let duration = properties.duration().as_millis() as u32;

    let (title, artist, album_artist, album, year, track_number, disc_number, genre) =
        if let Some(t) = tag {
            let title = normalize_text(
                t.title()
                    .map(|a| a.to_string())
                    .or_else(|| path.file_stem().map(|f| f.to_string_lossy().to_string())),
                "Unknown Title",
            );

            let artist = normalize_text(t.artist().map(|a| a.to_string()), "Unknown Artist");
            let album_artist = normalize_text(
                t.get_string(&ItemKey::AlbumArtist)
                    .map(|a| a.to_string())
                    .or_else(|| Some(artist.clone())),
                "Unknown Artist",
            );
            let album = normalize_text(t.album().map(|a| a.to_string()), "Unknown Album");

            let year = t.year().map(|y| y as i32).filter(|y| *y > 0);
            let track_number = t
                .track()
                .map(|n| n as i32)
                .filter(|n| *n > 0)
                .or_else(|| parse_tag_i32(t.get_string(&ItemKey::TrackNumber)));
            let disc_number = t
                .disk()
                .map(|n| n as i32)
                .filter(|n| *n > 0)
                .or_else(|| parse_tag_i32(t.get_string(&ItemKey::DiscNumber)));
            let genre = t
                .genre()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());

            (
                title,
                artist,
                album_artist,
                album,
                year,
                track_number,
                disc_number,
                genre,
            )
        } else {
            (
                normalize_text(
                    Some(
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                    ),
                    "Unknown Title",
                ),
                "Unknown Artist".to_string(),
                "Unknown Artist".to_string(),
                "Unknown Album".to_string(),
                None,
                None,
                None,
                None,
            )
        };

    let lyrics = tag.and_then(|t| t.get_string(&ItemKey::Lyrics).map(|s| s.to_string()));
    let explicit = tag.and_then(|tag| explicit_advisory(super::metadata::advisory_value(tag)));

    let sample_rate = properties.sample_rate();
    let bitrate = properties.audio_bitrate();
    let bit_depth = properties.bit_depth();
    let format = path
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase().to_string());

    let file_metadata = fs::metadata(path).ok();
    let file_size = file_metadata.as_ref().map(|m| m.len());
    let modified_at = file_metadata
        .as_ref()
        .and_then(crate::scanner::modification_time);

    let mut song = LocalSong {
        id: None,
        path: path.to_string_lossy().to_string(),
        title,
        artist,
        album_artist,
        album,
        year,
        track_number,
        disc_number,
        genre,
        explicit,
        composer: tag
            .and_then(|tag| tag.get_string(&ItemKey::Composer))
            .map(str::to_owned),
        duration,
        artwork: None,
        artwork_thumb: None,
        artwork_preview: None,
        lyrics,
        sample_rate,
        bitrate,
        bit_depth,
        format,
        modified_at,
        file_size,
    };

    // Let the library index the lightweight metadata before image decoding and resizing.
    on_metadata(&song);

    if let Some(paths) = tag
        .and_then(|t| t.pictures().iter().next())
        .and_then(|picture| {
            persist_artwork(
                artwork_dir,
                picture.data(),
                picture.mime_type().map(|mime| mime.as_str()),
            )
            .ok()
        })
    {
        song.artwork = Some(paths.full);
        song.artwork_thumb = Some(paths.thumb);
        song.artwork_preview = Some(paths.preview);
    }

    Ok(song)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::{
        config::WriteOptions,
        tag::{Tag, TagType},
    };

    #[test]
    fn advisory_tag_from_disposable_audio_controls_explicit_status() {
        let file = std::env::temp_dir().join(format!(
            "orca-explicit-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&38_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&2_u32.to_le_bytes());
        wav.extend_from_slice(&0_i16.to_le_bytes());
        fs::write(&file, wav).unwrap();
        assert_eq!(
            scan_music_file(&file, std::env::temp_dir().as_path())
                .unwrap()
                .explicit,
            None
        );

        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::ParentalAdvisory, "1".into());
        tag.save_to_path(&file, WriteOptions::default()).unwrap();
        assert_eq!(
            scan_music_file(&file, std::env::temp_dir().as_path())
                .unwrap()
                .explicit,
            Some(true)
        );

        tag.insert_text(ItemKey::ParentalAdvisory, "2".into());
        tag.save_to_path(&file, WriteOptions::default()).unwrap();
        assert_eq!(
            scan_music_file(&file, std::env::temp_dir().as_path())
                .unwrap()
                .explicit,
            Some(false)
        );
        fs::remove_file(file).unwrap();
    }
}
