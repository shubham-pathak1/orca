use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;
use rusqlite::{params, Connection};

pub fn migrate_inline_artwork_to_files(
    conn: &Connection,
    artwork_dir: &Path,
) -> Result<usize, String> {
    fs::create_dir_all(artwork_dir).map_err(|error| error.to_string())?;

    let mut statement = conn
        .prepare("SELECT id, path, artwork_url FROM songs WHERE artwork_url LIKE 'data:%'")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let inline_rows: Vec<(i64, String, String)> = rows
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<_, _>>()?;

    let mut migrated = 0;
    for (id, song_path, artwork_url) in inline_rows {
        let Some((mime, base64_payload)) = parse_data_url(&artwork_url) else {
            continue;
        };
        let Ok(decoded) = BASE64_STANDARD.decode(base64_payload.as_bytes()) else {
            continue;
        };
        if decoded.is_empty() {
            continue;
        }

        let hash = hash_song_path(&song_path);
        let file_path = artwork_dir.join(format!("{}.{}", hash, artwork_extension_from_mime(mime)));
        let thumb_path = artwork_dir.join(format!("thumb_{}_80.webp", hash));
        let preview_path = artwork_dir.join(format!("preview_{}_256.webp", hash));
        fs::write(&file_path, &decoded).map_err(|error| error.to_string())?;
        let (thumb_url, preview_url) =
            write_artwork_derivatives(&decoded, &thumb_path, &preview_path).unwrap_or((None, None));

        conn.execute(
            "UPDATE songs SET artwork_url = ?1, artwork_thumb_url = ?2, artwork_preview_url = ?3 WHERE id = ?4",
            params![
                file_path.to_string_lossy().to_string(),
                thumb_url,
                preview_url,
                id
            ],
        )
        .map_err(|error| error.to_string())?;
        migrated += 1;
    }

    Ok(migrated)
}

fn parse_data_url(value: &str) -> Option<(&str, &str)> {
    if !value.starts_with("data:") {
        return None;
    }

    let mut segments = value.splitn(2, ',');
    let header = segments.next()?;
    let payload = segments.next()?.trim();
    if !header.contains(";base64") {
        return None;
    }

    let mime = header
        .trim_start_matches("data:")
        .split(';')
        .next()
        .unwrap_or("image/jpeg");
    Some((mime, payload))
}

fn hash_song_path(path: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn write_artwork_derivatives(
    bytes: &[u8],
    thumb_path: &Path,
    preview_path: &Path,
) -> Result<(Option<String>, Option<String>), String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let thumb = write_webp_derivative(&image, thumb_path, 80)
        .ok()
        .map(|_| thumb_path.to_string_lossy().to_string());
    let preview = write_webp_derivative(&image, preview_path, 256)
        .ok()
        .map(|_| preview_path.to_string_lossy().to_string());
    Ok((thumb, preview))
}

fn write_webp_derivative(
    image: &image::DynamicImage,
    output_path: &Path,
    size: u32,
) -> Result<(), String> {
    let resized = image.resize(size, size, FilterType::Triangle).to_rgba8();
    let mut output = Vec::new();
    WebPEncoder::new_lossless(Cursor::new(&mut output))
        .encode(
            resized.as_raw(),
            resized.width(),
            resized.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
    fs::write(output_path, output).map_err(|error| error.to_string())
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
