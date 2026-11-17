use super::{BackdropStyle, Decoded, Key, BACKDROP_EDGE, BACKDROP_SOURCE_EDGE};
use image::imageops::FilterType;
use std::path::Path;

#[cfg(test)]
pub(super) fn decode(key: Key) -> Decoded {
    decode_cancellable(key, |_| true)
}

pub(super) fn decode_cancellable(key: Key, mut wanted: impl FnMut(&Key) -> bool) -> Decoded {
    let mut cancelled = false;
    let mut checkpoint = || {
        if wanted(&key) {
            Some(())
        } else {
            cancelled = true;
            None
        }
    };
    let result = (|| {
        checkpoint()?;
        let image = if key.tiles.is_empty() {
            image::ImageReader::open(Path::new(&key.path))
                .ok()?
                .decode()
                .ok()?
        } else {
            compose(&key.tiles, key.edge)
        };
        checkpoint()?;
        let image = if key.backdrop == Some(BackdropStyle::Gradient) {
            super::gradient::render(&image, key.edge, key.height, &mut checkpoint)?
        } else if key.backdrop.is_some() && key.height == 0 {
            // A color wash has no recognizable cover texture. Derive four
            // broad colors once, then interpolate on the worker, not per frame.
            let colors = image.resize_exact(2, 2, FilterType::Triangle).to_rgb8();
            let mut wash = image::RgbaImage::new(BACKDROP_EDGE, BACKDROP_EDGE);
            for (x, y, pixel) in wash.enumerate_pixels_mut() {
                if x == 0 && y.is_multiple_of(64) {
                    checkpoint()?;
                }
                let u = x as f32 / (BACKDROP_EDGE - 1) as f32;
                let v = y as f32 / (BACKDROP_EDGE - 1) as f32;
                let mut rgba = [0, 0, 0, 255];
                for (channel, value) in rgba.iter_mut().enumerate().take(3) {
                    let top = colors.get_pixel(0, 0)[channel] as f32 * (1.0 - u)
                        + colors.get_pixel(1, 0)[channel] as f32 * u;
                    let bottom = colors.get_pixel(0, 1)[channel] as f32 * (1.0 - u)
                        + colors.get_pixel(1, 1)[channel] as f32 * u;
                    *value = (top * (1.0 - v) + bottom * v).round() as u8;
                }
                if let Some(blend) = key.blend {
                    let color = blend.apply([rgba[0], rgba[1], rgba[2]]);
                    rgba[..3].copy_from_slice(&color);
                }
                *pixel = image::Rgba(rgba);
            }
            image::DynamicImage::ImageRgba8(wash)
        } else if key.backdrop.is_some() {
            image
                .resize(
                    BACKDROP_SOURCE_EDGE,
                    BACKDROP_SOURCE_EDGE,
                    FilterType::Triangle,
                )
                .blur(5.0 * BACKDROP_SOURCE_EDGE as f32 / 160.0)
                .resize_to_fill(key.edge, key.height, FilterType::Triangle)
        } else if key.tiles.is_empty() || key.height > 0 {
            // Crop to the delegate's shape to avoid a second, blurry enlargement.
            let height = if key.height > 0 { key.height } else { key.edge };
            if image.width() == key.edge && image.height() == height {
                image
            } else {
                image.resize_to_fill(key.edge, height, FilterType::Lanczos3)
            }
        } else {
            image.resize(key.edge, key.edge, FilterType::Triangle)
        };
        checkpoint()?;
        let accent = if key.backdrop.is_some() {
            [245; 3]
        } else {
            accent(&image)
        };
        checkpoint()?;
        let mut buffer = image.into_rgba8();
        if key.backdrop == Some(BackdropStyle::Artwork) && key.height > 0 && key.radius == 0 {
            let width = buffer.width().max(1);
            let opacity = (0.78_f32 * 255.0) as u16;
            for (x, _, pixel) in buffer.enumerate_pixels_mut() {
                let u = x as f32 / width.saturating_sub(1).max(1) as f32;
                let visibility = 0.12 + 0.22 * (1.0 - (2.0 * u - 1.0).abs());
                for (channel, base) in [9.0, 10.0, 12.0].into_iter().enumerate() {
                    let shaded = (pixel[channel] as f32 * visibility + base * (1.0 - visibility))
                        .round() as u16;
                    pixel[channel] =
                        (shaded * opacity / 255 + base as u16 * (255 - opacity) / 255) as u8;
                }
                pixel[3] = 255;
            }
        }
        round_corners(&mut buffer, key.radius);
        checkpoint()?;
        Some((buffer.width(), buffer.height(), buffer.into_raw(), accent))
    })();
    let (width, height, bytes, accent) = result.unwrap_or((0, 0, vec![], [245, 245, 245]));
    Decoded {
        key,
        cancelled,
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
    let inner = (r - 0.5).powi(2);
    let outer = (r + 0.5).powi(2);
    for y in 0..radius {
        for x in 0..radius {
            let dx = r - (x as f32 + 0.5);
            let dy = r - (y as f32 + 0.5);
            let distance = dx * dx + dy * dy;
            if distance <= inner {
                continue;
            }
            let coverage = if distance >= outer {
                0.0
            } else {
                (r + 0.5 - distance.sqrt()).clamp(0.0, 1.0)
            };
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
