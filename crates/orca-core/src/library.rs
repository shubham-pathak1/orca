use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;

use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::prelude::ItemKey;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalSong {
    pub id: Option<i64>,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration: u32,
    pub artwork: Option<String>,
    pub lyrics: Option<String>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub format: Option<String>,
}

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

pub fn scan_music_folder(folder_path: &Path, artwork_dir: &Path) -> Result<Vec<LocalSong>, String> {
    let mut songs = Vec::new();

    for entry in WalkDir::new(folder_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(ext) = path.extension() else {
            continue;
        };
        let ext_str = ext.to_string_lossy().to_lowercase();
        let is_audio =
            ext_str == "mp3" || ext_str == "flac" || ext_str == "m4a" || ext_str == "wav";
        if !is_audio {
            continue;
        }

        if let Ok(song) = extract_local_metadata(path, artwork_dir) {
            songs.push(song);
        }
    }

    Ok(songs)
}

fn extract_local_metadata(path: &Path, artwork_dir: &Path) -> Result<LocalSong, String> {
    let tagged_file = Probe::open(path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();
    let duration = properties.duration().as_millis() as u32;

    let (title, artist, album_artist, album, year, track_number, disc_number) = if let Some(t) = tag {
        let title = normalize_text(
            t.title().map(|a| a.to_string()).or_else(|| {
                path.file_name()
                    .map(|f| f.to_string_lossy().to_string())
            }),
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
        let track_number = t.track().map(|n| n as i32).filter(|n| *n > 0).or_else(|| {
            parse_tag_i32(t.get_string(&ItemKey::TrackNumber))
        });
        let disc_number = t.disk().map(|n| n as i32).filter(|n| *n > 0).or_else(|| {
            parse_tag_i32(t.get_string(&ItemKey::DiscNumber))
        });

        (
            title,
            artist,
            album_artist,
            album,
            year,
            track_number,
            disc_number,
        )
    } else {
        (
            normalize_text(Some(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            ), "Unknown Title"),
            "Unknown Artist".to_string(),
            "Unknown Artist".to_string(),
            "Unknown Album".to_string(),
            None,
            None,
            None,
        )
    };

    let lyrics = tag.and_then(|t| {
        t.get_string(&ItemKey::Lyrics)
            .map(|s| s.to_string())
    });

    let artwork = tag.and_then(|t| t.pictures().iter().next()).and_then(|p| {
        persist_artwork(path, artwork_dir, p.data(), p.mime_type().map(|m| m.as_str())).ok()
    });

    let sample_rate = properties.sample_rate();
    let bitrate = properties.audio_bitrate();
    let bit_depth = properties.bit_depth();
    let format = path.extension().map(|e| e.to_string_lossy().to_uppercase());

    Ok(LocalSong {
        id: None,
        path: path.to_string_lossy().to_string(),
        title,
        artist,
        album_artist,
        album,
        year,
        track_number,
        disc_number,
        duration,
        artwork,
        lyrics,
        sample_rate,
        bitrate,
        bit_depth,
        format,
    })
}

fn artwork_extension_from_mime(mime: &str) -> &'static str {
    let normalized = mime.to_ascii_lowercase();
    if normalized.contains("png") { "png" }
    else if normalized.contains("webp") { "webp" }
    else if normalized.contains("gif") { "gif" }
    else if normalized.contains("bmp") { "bmp" }
    else { "jpg" }
}

fn persist_artwork(
    song_path: &Path,
    artwork_dir: &Path,
    bytes: &[u8],
    mime: Option<&str>,
) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("Artwork bytes are empty".to_string());
    }

    fs::create_dir_all(artwork_dir).map_err(|e| e.to_string())?;

    let ext = artwork_extension_from_mime(mime.unwrap_or("image/jpeg"));
    let hash = hash_song_path(song_path);
    let original_path = artwork_dir.join(format!("art_{}.{}", hash, ext));
    let thumb_path = artwork_dir.join(format!("thumb_{}.jpg", hash));

    if !original_path.exists() {
        fs::write(&original_path, bytes).map_err(|e| e.to_string())?;
    }

    if !thumb_path.exists() {
        if let Ok(img) = image::load_from_memory(bytes) {
            let thumbnail = img.resize(300, 300, FilterType::Triangle);
            let mut output: Vec<u8> = Vec::new();
            if thumbnail.write_to(&mut Cursor::new(&mut output), image::ImageFormat::Jpeg).is_ok() {
                let _ = fs::write(&thumb_path, &output);
            }
        }
    }

    Ok(original_path.to_string_lossy().to_string())
}

fn hash_song_path(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    hasher.finish()
}

