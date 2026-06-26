use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Tag;
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
    pub genre: Option<String>,
    pub duration: u32,
    pub artwork: Option<String>,
    pub artwork_thumb: Option<String>,
    pub artwork_preview: Option<String>,
    pub lyrics: Option<String>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub format: Option<String>,
    pub modified_at: Option<i64>,
    pub file_size: Option<u64>,
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

pub fn scan_music_folder(
    folder_path: &Path,
    artwork_dir: &Path,
    existing_map: &std::collections::HashMap<String, (i64, u64, LocalSong)>,
) -> Result<Vec<LocalSong>, String> {
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
            ext_str == "mp3" || ext_str == "flac" || ext_str == "m4a" || ext_str == "wav" ||
            ext_str == "ogg" || ext_str == "opus" || ext_str == "aiff" || ext_str == "aif";
        if !is_audio {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        let mut reused = false;

        if let Some((stored_mtime, stored_size, cached_song)) = existing_map.get(&path_str) {
            if let Ok(metadata) = fs::metadata(path) {
                let current_size = metadata.len();
                let current_mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                if current_size == *stored_size && current_mtime == *stored_mtime {
                    songs.push(cached_song.clone());
                    reused = true;
                }
            }
        }

        if !reused {
            if let Ok(song) = scan_music_file(path, artwork_dir) {
                songs.push(song);
            }
        }
    }

    Ok(songs)
}

pub fn scan_music_file(path: &Path, artwork_dir: &Path) -> Result<LocalSong, String> {
    let tagged_file = Probe::open(path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();
    let duration = properties.duration().as_millis() as u32;

    let (title, artist, album_artist, album, year, track_number, disc_number, genre) = if let Some(t) = tag {
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
        let genre = t.genre().map(|value| value.trim().to_string()).filter(|value| !value.is_empty());

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
            None,
        )
    };

    let lyrics = tag.and_then(|t| {
        t.get_string(&ItemKey::Lyrics)
            .map(|s| s.to_string())
    });

    let artwork_paths = tag.and_then(|t| t.pictures().iter().next()).and_then(|p| {
        persist_artwork(path, artwork_dir, p.data(), p.mime_type().map(|m| m.as_str())).ok()
    });

    let sample_rate = properties.sample_rate();
    let bitrate = properties.audio_bitrate();
    let bit_depth = properties.bit_depth();
    let format = path.extension().map(|e| e.to_string_lossy().to_uppercase());

    let file_metadata = fs::metadata(path).ok();
    let file_size = file_metadata.as_ref().map(|m| m.len());
    let modified_at = file_metadata
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

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
        genre,
        duration,
        artwork: artwork_paths.as_ref().map(|paths| paths.full.clone()),
        artwork_thumb: artwork_paths.as_ref().map(|paths| paths.thumb.clone()),
        artwork_preview: artwork_paths.as_ref().map(|paths| paths.preview.clone()),
        lyrics,
        sample_rate,
        bitrate,
        bit_depth,
        format,
        modified_at,
        file_size,
    })
}

#[derive(Deserialize)]
pub struct SongMetadataUpdate {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<String>,
    pub lyrics: Option<String>,
}

pub fn update_song_metadata(update: SongMetadataUpdate) -> Result<(), String> {
    let path = Path::new(&update.path);
    let mut tagged_file = Probe::open(path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let tag = writable_tag_mut(&mut tagged_file)?;

    set_text(tag, ItemKey::TrackTitle, &update.title);
    set_text(tag, ItemKey::TrackArtist, &update.artist);
    set_text(tag, ItemKey::AlbumTitle, &update.album);
    set_text(tag, ItemKey::AlbumArtist, &update.album_artist);
    set_optional_number(tag, ItemKey::Year, update.year);
    set_optional_number(tag, ItemKey::TrackNumber, update.track_number);
    set_optional_number(tag, ItemKey::DiscNumber, update.disc_number);
    set_optional_text(tag, ItemKey::Genre, update.genre.as_deref());
    set_optional_text(tag, ItemKey::Lyrics, update.lyrics.as_deref());

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())
}

pub fn replace_song_cover(song_path: &Path, image_path: &Path) -> Result<(), String> {
    let mut tagged_file = Probe::open(song_path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let tag = writable_tag_mut(&mut tagged_file)?;

    clear_pictures(tag);
    let mut file = fs::File::open(image_path).map_err(|e| e.to_string())?;
    let mut picture = Picture::from_reader(&mut file).map_err(|e| e.to_string())?;
    picture.set_pic_type(PictureType::CoverFront);
    tag.push_picture(picture);

    tagged_file
        .save_to_path(song_path, WriteOptions::default())
        .map_err(|e| e.to_string())
}

pub fn remove_song_cover(song_path: &Path) -> Result<(), String> {
    let mut tagged_file = Probe::open(song_path)
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    if let Ok(tag) = writable_tag_mut(&mut tagged_file) {
        clear_pictures(tag);
    }

    tagged_file
        .save_to_path(song_path, WriteOptions::default())
        .map_err(|e| e.to_string())
}

fn writable_tag_mut(tagged_file: &mut lofty::file::TaggedFile) -> Result<&mut Tag, String> {
    ensure_primary_tag(tagged_file)?;
    let primary_type = tagged_file.primary_tag_type();

    if tagged_file.contains_tag_type(primary_type) {
        return tagged_file
            .primary_tag_mut()
            .ok_or_else(|| "Could not access the primary metadata tag".to_string());
    }

    tagged_file
        .first_tag_mut()
        .ok_or_else(|| "Could not create a writable metadata tag".to_string())
}

fn ensure_primary_tag(tagged_file: &mut lofty::file::TaggedFile) -> Result<(), String> {
    if tagged_file.primary_tag().is_some() || tagged_file.first_tag().is_some() {
        return Ok(());
    }

    let tag_type = tagged_file.primary_tag_type();
    tagged_file.insert_tag(Tag::new(tag_type));
    Ok(())
}

fn set_text(tag: &mut Tag, key: ItemKey, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        tag.remove_key(&key);
    } else {
        tag.insert_text(key, trimmed.to_string());
    }
}

fn set_optional_text(tag: &mut Tag, key: ItemKey, value: Option<&str>) {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            tag.insert_text(key, value.to_string());
        }
        None => tag.remove_key(&key),
    }
}

fn set_optional_number(tag: &mut Tag, key: ItemKey, value: Option<i32>) {
    match value.filter(|value| *value > 0) {
        Some(value) => {
            tag.insert_text(key, value.to_string());
        }
        None => tag.remove_key(&key),
    }
}

fn clear_pictures(tag: &mut Tag) {
    while !tag.pictures().is_empty() {
        tag.remove_picture(0);
    }
}

fn artwork_extension_from_mime(mime: &str) -> &'static str {
    let normalized = mime.to_ascii_lowercase();
    if normalized.contains("png") { "png" }
    else if normalized.contains("webp") { "webp" }
    else if normalized.contains("gif") { "gif" }
    else if normalized.contains("bmp") { "bmp" }
    else { "jpg" }
}

struct ArtworkPaths {
    full: String,
    thumb: String,
    preview: String,
}

fn persist_artwork(
    song_path: &Path,
    artwork_dir: &Path,
    bytes: &[u8],
    mime: Option<&str>,
) -> Result<ArtworkPaths, String> {
    if bytes.is_empty() {
        return Err("Artwork bytes are empty".to_string());
    }

    fs::create_dir_all(artwork_dir).map_err(|e| e.to_string())?;

    let ext = artwork_extension_from_mime(mime.unwrap_or("image/jpeg"));
    let hash = hash_song_path(song_path);
    let original_path = artwork_dir.join(format!("art_{}.{}", hash, ext));
    let thumb_path = artwork_dir.join(format!("thumb_{}_80.webp", hash));
    let preview_path = artwork_dir.join(format!("preview_{}_256.webp", hash));

    fs::write(&original_path, bytes).map_err(|e| e.to_string())?;

    let mut thumb_written = false;
    let mut preview_written = false;
    if let Ok(img) = image::load_from_memory(bytes) {
        if write_webp_derivative(&img, &thumb_path, 80).is_ok() {
            thumb_written = true;
        }
        if write_webp_derivative(&img, &preview_path, 256).is_ok() {
            preview_written = true;
        }
    }

    Ok(ArtworkPaths {
        full: original_path.to_string_lossy().to_string(),
        thumb: if thumb_written { thumb_path } else { original_path.clone() }.to_string_lossy().to_string(),
        preview: if preview_written { preview_path } else { original_path.clone() }.to_string_lossy().to_string(),
    })
}

fn write_webp_derivative(
    image: &image::DynamicImage,
    output_path: &Path,
    size: u32,
) -> Result<(), String> {
    let resized = image.resize(size, size, FilterType::Triangle).to_rgba8();
    let mut output: Vec<u8> = Vec::new();
    let encoder = WebPEncoder::new_lossless(Cursor::new(&mut output));
    encoder
        .encode(
            resized.as_raw(),
            resized.width(),
            resized.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
    fs::write(output_path, &output).map_err(|error| error.to_string())
}

fn hash_song_path(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    hasher.finish()
}

