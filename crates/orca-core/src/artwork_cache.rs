use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;

pub struct ArtworkPaths {
    pub full: String,
    pub thumb: String,
    pub preview: String,
}

pub fn persist_artwork(
    song_path: &Path,
    artwork_dir: &Path,
    bytes: &[u8],
    mime: Option<&str>,
) -> Result<ArtworkPaths, String> {
    if bytes.is_empty() {
        return Err("Artwork bytes are empty".to_string());
    }

    fs::create_dir_all(artwork_dir).map_err(|error| error.to_string())?;

    let extension = artwork_extension_from_mime(mime.unwrap_or("image/jpeg"));
    let hash = artwork_hash(song_path, bytes);
    let original_path = artwork_dir.join(format!("art_{hash}.{extension}"));
    let thumb_path = artwork_dir.join(format!("thumb_{hash}_80.webp"));
    let preview_path = artwork_dir.join(format!("preview_{hash}_256.webp"));

    fs::write(&original_path, bytes).map_err(|error| error.to_string())?;

    let mut thumb_written = false;
    let mut preview_written = false;
    if let Ok(image) = image::load_from_memory(bytes) {
        if write_webp_derivative(&image, &thumb_path, 80).is_ok() {
            thumb_written = true;
        }
        if write_webp_derivative(&image, &preview_path, 256).is_ok() {
            preview_written = true;
        }
    }

    Ok(ArtworkPaths {
        full: original_path.to_string_lossy().to_string(),
        thumb: if thumb_written {
            thumb_path
        } else {
            original_path.clone()
        }
        .to_string_lossy()
        .to_string(),
        preview: if preview_written {
            preview_path
        } else {
            original_path.clone()
        }
        .to_string_lossy()
        .to_string(),
    })
}

pub fn write_webp_derivative(
    image: &image::DynamicImage,
    output_path: &Path,
    size: u32,
) -> Result<(), String> {
    let resized = image.resize(size, size, FilterType::Triangle).to_rgba8();
    let mut output = Vec::new();
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

fn artwork_extension_from_mime(mime: &str) -> &'static str {
    let normalized = mime.to_ascii_lowercase();
    if normalized.contains("png") {
        "png"
    } else if normalized.contains("webp") {
        "webp"
    } else if normalized.contains("gif") {
        "gif"
    } else if normalized.contains("bmp") {
        "bmp"
    } else {
        "jpg"
    }
}

fn artwork_hash(song_path: &Path, bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    song_path.to_string_lossy().hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
}
