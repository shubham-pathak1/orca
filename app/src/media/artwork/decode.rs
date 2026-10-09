use super::{Decoded, Key};
use image::imageops::FilterType;
use std::path::Path;

pub(super) fn decode(key: Key) -> Decoded {
    let result = (|| {
        let image = if key.tiles.is_empty() {
            image::ImageReader::open(Path::new(&key.path))
                .ok()?
                .decode()
                .ok()?
        } else {
            compose(&key.tiles, key.edge)
        };
        let image = if key.backdrop && key.height == 0 {
            // A color wash has no recognizable cover texture. Derive four
            // broad colors once, then interpolate on the worker, not per frame.
            let colors = image.resize_exact(2, 2, FilterType::Triangle).to_rgb8();
            let wash = image::RgbaImage::from_fn(1024, 1024, |x, y| {
                let u = x as f32 / 1023.0;
                let v = y as f32 / 1023.0;
                let mut rgba = [0, 0, 0, 255];
                for (channel, value) in rgba.iter_mut().enumerate().take(3) {
                    let top = colors.get_pixel(0, 0)[channel] as f32 * (1.0 - u)
                        + colors.get_pixel(1, 0)[channel] as f32 * u;
                    let bottom = colors.get_pixel(0, 1)[channel] as f32 * (1.0 - u)
                        + colors.get_pixel(1, 1)[channel] as f32 * u;
                    *value = (top * (1.0 - v) + bottom * v).round() as u8;
                }
                image::Rgba(rgba)
            });
            image::DynamicImage::ImageRgba8(wash)
        } else if key.backdrop {
            // Upscale on the worker; software rendering otherwise exposes enlarged texels.
            image
                .resize(160, 160, FilterType::Triangle)
                .blur(5.0)
                .resize_to_fill(
                    if key.height == 0 { 1024 } else { key.edge },
                    if key.height == 0 { 1024 } else { key.height },
                    FilterType::Triangle,
                )
        } else if key.tiles.is_empty() || key.height > 0 {
            // Crop to the delegate's shape to avoid a second, blurry enlargement.
            image.resize_to_fill(
                key.edge,
                if key.height > 0 { key.height } else { key.edge },
                if key.edge <= 96 || key.height > 0 {
                    FilterType::Lanczos3
                } else {
                    FilterType::Triangle
                },
            )
        } else {
            image.resize(key.edge, key.edge, FilterType::Triangle)
        };
        let accent = if key.backdrop {
            [245; 3]
        } else {
            accent(&image)
        };
        let mut buffer = image.to_rgba8();
        if key.backdrop && key.height > 0 && key.radius == 0 {
            let width = buffer.width().max(1);
            for (x, _, pixel) in buffer.enumerate_pixels_mut() {
                let u = x as f32 / width.saturating_sub(1).max(1) as f32;
                let visibility = 0.12 + 0.22 * (1.0 - (2.0 * u - 1.0).abs());
                for (channel, base) in [9.0, 10.0, 12.0].into_iter().enumerate() {
                    pixel[channel] = (pixel[channel] as f32 * visibility
                        + base * (1.0 - visibility))
                        .round() as u8;
                }
                pixel[3] = 255;
            }
        }
        round_corners(&mut buffer, key.radius);
        Some((buffer.width(), buffer.height(), buffer.into_raw(), accent))
    })();
    let (width, height, bytes, accent) = result.unwrap_or((0, 0, vec![], [245, 245, 245]));
    Decoded {
        key,
        cancelled: false,
        width,
        height,
        bytes,
        accent,
    }
}
// Clip images on the worker because the software renderer clips children to
// rectangular bounds even when their parent Rectangle has a border radius.
pub(super) fn round_corners(image: &mut image::RgbaImage, radius: u32) {
    let (width, height) = image.dimensions();
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return;
    }
    let r = radius as f32;
    for y in 0..radius {
        for x in 0..radius {
            let dx = r - (x as f32 + 0.5);
            let dy = r - (y as f32 + 0.5);
            let coverage = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            for (px, py) in [
                (x, y),
                (width - 1 - x, y),
                (x, height - 1 - y),
                (width - 1 - x, height - 1 - y),
            ] {
                let pixel = image.get_pixel_mut(px, py);
                pixel.0[3] = (pixel.0[3] as f32 * coverage).round() as u8;
            }
        }
    }
}
// Compose once on the decode worker. The UI receives one cached bitmap, rather
// than retaining four independent images and scaling them on every paint.
pub(super) fn compose(paths: &[String], edge: u32) -> image::DynamicImage {
    let height = edge * 3 / 4;
    let mut canvas = image::RgbaImage::from_pixel(edge, height, image::Rgba([16, 16, 16, 255]));
    let gap = (edge / 220).max(1);
    let left = (edge - gap) / 2;
    let top = (height - gap) / 2;
    for (index, path) in paths.iter().take(4).enumerate() {
        let Ok(reader) = image::ImageReader::open(path) else {
            continue;
        };
        let Ok(image) = reader.decode() else {
            continue;
        };
        let (x, y, width, tile_height) = if paths.len() == 2 {
            if index == 0 {
                (0, 0, left, height)
            } else {
                (left + gap, 0, edge - left - gap, height)
            }
        } else if paths.len() == 3 && index == 0 {
            (0, 0, left, height)
        } else {
            let right = paths.len() == 3 || index % 2 == 1;
            let bottom = if paths.len() == 3 {
                index == 2
            } else {
                index >= 2
            };
            let x = if right { left + gap } else { 0 };
            let y = if bottom { top + gap } else { 0 };
            (
                x,
                y,
                if right { edge - x } else { left },
                if bottom { height - y } else { top },
            )
        };
        let tile = image
            .resize_to_fill(width, tile_height, FilterType::Triangle)
            .to_rgba8();
        image::imageops::overlay(&mut canvas, &tile, x as i64, y as i64);
    }
    image::DynamicImage::ImageRgba8(canvas)
}
pub(super) fn accent(image: &image::DynamicImage) -> [u8; 3] {
    let sample = image.resize_exact(48, 48, FilterType::Triangle).to_rgb8();
    let mut sum = [0u64; 3];
    let mut count = 0;
    for y in 0..48 {
        for x in (0..48).step_by(4) {
            let c = sample.get_pixel(x, y).0;
            let saturation = c.iter().max().unwrap() - c.iter().min().unwrap();
            let brightness = c.iter().map(|c| *c as u32).sum::<u32>() / 3;
            if saturation > 18 && (34..=232).contains(&brightness) {
                for i in 0..3 {
                    sum[i] += c[i] as u64;
                }
                count += 1;
            }
        }
    }
    if count == 0 {
        [245, 245, 245]
    } else {
        sum.map(|v| (v / count) as u8)
    }
}
