use std::fs;
use std::io::Cursor;
use std::path::Path;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;

#[derive(Debug)]
pub struct ArtworkPaths {
    pub full: String,
    pub thumb: String,
    pub preview: String,
}

pub fn persist_artwork(
    artwork_dir: &Path,
    bytes: &[u8],
    mime: Option<&str>,
) -> Result<ArtworkPaths, String> {
    if bytes.is_empty() {
        return Err("Artwork bytes are empty".to_string());
    }

    fs::create_dir_all(artwork_dir).map_err(|error| error.to_string())?;

    let extension = artwork_extension_from_mime(mime.unwrap_or("image/jpeg"));
    let hash = artwork_hash(bytes);
    let original_path = artwork_dir.join(format!("art_{hash}.{extension}"));
    let thumb_path = artwork_dir.join(format!("thumb_{hash}_80.webp"));
    let preview_path = artwork_dir.join(format!("preview_{hash}_256.webp"));

    crate::atomic_file::write(&original_path, bytes).map_err(|error| error.to_string())?;

    let mut thumb_written = image::image_dimensions(&thumb_path).is_ok();
    let mut preview_written = image::image_dimensions(&preview_path).is_ok();
    if !thumb_written || !preview_written {
        if let Ok(image) = image::load_from_memory(bytes) {
            if !thumb_written && write_webp_derivative(&image, &thumb_path, 80).is_ok() {
                thumb_written = true;
            }
            if !preview_written && write_webp_derivative(&image, &preview_path, 256).is_ok() {
                preview_written = true;
            }
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
    crate::atomic_file::write(output_path, &output).map_err(|error| error.to_string())
}

pub(crate) fn artwork_extension_from_mime(mime: &str) -> &'static str {
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

fn artwork_hash(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

#[cfg(test)]
mod tests {
    use super::artwork_hash;

    #[test]
    fn artwork_hash_is_content_based() {
        assert_eq!(artwork_hash(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn inline_migration_keeps_original_bytes_and_writes_complete_derivatives() {
        use super::*;
        use base64::Engine;
        let folder = std::env::temp_dir().join(format!(
            "orca-inline-artwork-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&folder).unwrap();
        let conn = crate::db::init_db(folder.clone()).unwrap();
        let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            512,
            256,
            image::Rgb([120, 40, 210]),
        ));
        let mut png = Cursor::new(Vec::new());
        source.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let bytes = png.into_inner();
        let data = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        conn.execute("INSERT INTO songs(path,title,artist,duration,artwork_url) VALUES(?1,'Fixture','Artist',1000,?2)", ["fixture.flac",data.as_str()]).unwrap();
        let artwork = folder.join("artwork");
        assert_eq!(
            crate::db::migrate_inline_artwork_to_files(&conn, &artwork).unwrap(),
            1
        );
        let (full, thumb, preview): (String, String, String) = conn
            .query_row(
                "SELECT artwork_url,artwork_thumb_url,artwork_preview_url FROM songs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(fs::read(&full).unwrap(), bytes);
        assert_eq!(image::image_dimensions(&thumb).unwrap(), (80, 40));
        assert_eq!(image::image_dimensions(&preview).unwrap(), (256, 128));
        assert_eq!(
            crate::db::migrate_inline_artwork_to_files(&conn, &artwork).unwrap(),
            0
        );
        assert_eq!(fs::read(full).unwrap(), bytes);
        drop(conn);
        fs::remove_dir_all(folder).unwrap();
    }
}
