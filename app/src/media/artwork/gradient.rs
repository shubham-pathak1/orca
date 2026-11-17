//! Cover-derived radial gradients, baked once on the artwork worker.
use image::{imageops::FilterType, DynamicImage, Rgb, RgbImage};

type Color = [f32; 3];

pub(super) fn render(
    source: &DynamicImage,
    width: u32,
    height: u32,
    mut checkpoint: impl FnMut() -> Option<()>,
) -> Option<DynamicImage> {
    checkpoint()?;
    let colors = extract_mesh_palette(source)
        .map(|[h, s, l]| hsl_to_rgb([h, (s * 1.35).min(1.0), l.clamp(0.28, 0.58)]));
    let mut base = rgb_to_hsl(colors[0]);
    base[2] = 0.12;
    let base = hsl_to_rgb(base);
    let factor = (128.0 / width.max(height).max(1) as f32).min(1.0);
    let w = (width as f32 * factor).round().max(8.0) as u32;
    let h = (height as f32 * factor).round().max(8.0) as u32;
    let anchors = [(0.20, 0.25), (0.80, 0.20), (0.75, 0.80), (0.25, 0.75)];
    let speeds = [1.0, -0.7, 0.85, -1.15];
    let phase = std::f32::consts::PI * 0.45;
    let centers: [(f32, f32); 4] = std::array::from_fn(|i| {
        (
            (anchors[i].0 + 0.16 * (phase * speeds[i] + i as f32 * 1.7).cos()) * w as f32,
            (anchors[i].1 + 0.16 * (phase * speeds[i] * 0.9 + i as f32 * 2.3).sin()) * h as f32,
        )
    });
    let radius = w.max(h) as f32 * 0.62;
    let mut mesh = RgbImage::new(w, h);
    for y in 0..h {
        checkpoint()?;
        for x in 0..w {
            let mut color = base;
            for i in 0..4 {
                let distance = ((x as f32 + 0.5 - centers[i].0).powi(2)
                    + (y as f32 + 0.5 - centers[i].1).powi(2))
                .sqrt();
                let t = (distance / radius).clamp(0.0, 1.0);
                let falloff = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
                let alpha = 0.85 * (1.0 - falloff);
                for channel in 0..3 {
                    color[channel] += (colors[i][channel] - color[channel]) * alpha;
                }
            }
            mesh.put_pixel(x, y, Rgb(color.map(|v| (v * 255.0).round() as u8)));
        }
    }
    // Blur the small bitmap, then enlarge around its center to hide edge
    // clamping. Apply the readability scrim after the crop.
    let blurred = image::imageops::blur(&mesh, w as f32 * 64.0 / 1080.0);
    checkpoint()?;
    let mut output = RgbImage::new(w, h);
    for y in 0..h {
        checkpoint()?;
        let shade = 0.90 - 0.28 * y as f32 / h.saturating_sub(1).max(1) as f32;
        for x in 0..w {
            let sx = ((x as f32 - w as f32 / 2.0) / 1.3 + w as f32 / 2.0) as u32;
            let sy = ((y as f32 - h as f32 / 2.0) / 1.3 + h as f32 / 2.0) as u32;
            output.put_pixel(
                x,
                y,
                Rgb(blurred
                    .get_pixel(sx, sy)
                    .0
                    .map(|v| (v as f32 * shade).round() as u8)),
            );
        }
    }
    Some(DynamicImage::ImageRgb8(image::imageops::resize(
        &output,
        width,
        height,
        FilterType::Triangle,
    )))
}

fn extract_mesh_palette(source: &DynamicImage) -> [Color; 4] {
    // Bound palette extraction independently of the source cover size.
    // RGB555 median cut keeps broad color populations ahead of fine detail.
    let area = source.width() as f64 * source.height() as f64;
    let ratio = (112.0 * 112.0 / area.max(1.0)).sqrt().min(1.0);
    let image = source
        .resize_exact(
            (source.width() as f64 * ratio).ceil().max(1.0) as u32,
            (source.height() as f64 * ratio).ceil().max(1.0) as u32,
            FilterType::Nearest,
        )
        .to_rgb8();
    let mut histogram = vec![0_u32; 32768];
    for pixel in image.pixels() {
        let [r, g, b] = pixel.0.map(|v| (v >> 3) as usize);
        histogram[(r << 10) | (g << 5) | b] += 1;
    }
    let mut swatches = quantize_swatches(&histogram, true);
    if swatches.is_empty() {
        swatches = self::quantize_swatches(&histogram, false);
    }
    swatches.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut distinct: Vec<Color> = Vec::with_capacity(4);
    for (color, _) in swatches {
        let color = rgb_to_hsl(color);
        if distinct.iter().all(|other| {
            let gap = (color[0] - other[0]).abs();
            gap.min(360.0 - gap) >= 15.0 || (color[2] - other[2]).abs() >= 0.12
        }) {
            distinct.push(color);
            if distinct.len() == 4 {
                break;
            }
        }
    }
    if distinct.is_empty() {
        return [
            [58, 28, 113],
            [215, 109, 119],
            [43, 88, 118],
            [255, 175, 123],
        ]
        .map(|color| rgb_to_hsl(color.map(|v| v as f32 / 255.0)));
    }
    let count = distinct.len();
    while distinct.len() < 4 {
        let step = distinct.len() - count + 1;
        let [h, s, l] = distinct[(distinct.len() - count) % count];
        distinct.push([
            (h + 24.0 * step as f32) % 360.0,
            s,
            (l + 0.12 * step as f32).clamp(0.2, 0.7),
        ]);
    }
    std::array::from_fn(|i| distinct[i])
}

fn rgb555_components(color: usize) -> [u32; 3] {
    [
        (color >> 10) as u32,
        ((color >> 5) & 31) as u32,
        (color & 31) as u32,
    ]
}

fn quantize_swatches(histogram: &[u32], filtered: bool) -> Vec<(Color, u32)> {
    let allowed = |color: Color| {
        let [h, s, l] = rgb_to_hsl(color);
        !filtered || (l > 0.05 && l < 0.95 && !((10.0..=37.0).contains(&h) && s <= 0.82))
    };
    let colors: Vec<usize> = histogram
        .iter()
        .enumerate()
        .filter(|(color, count)| {
            **count > 0 && allowed(rgb555_components(*color).map(|v| (v << 3) as f32 / 255.0))
        })
        .map(|(color, _)| color)
        .collect();
    if colors.is_empty() {
        return vec![];
    }
    if colors.len() <= 24 {
        return colors
            .into_iter()
            .map(|color| {
                (
                    rgb555_components(color).map(|v| (v << 3) as f32 / 255.0),
                    histogram[color],
                )
            })
            .collect();
    }
    let mut boxes = vec![colors];
    while boxes.len() < 24 {
        let (index, _) = boxes
            .iter()
            .enumerate()
            .max_by_key(|(_, colors)| color_box_volume(colors))
            .unwrap();
        if boxes[index].len() <= 1 {
            break;
        }
        let mut colors = boxes.swap_remove(index);
        let ranges = channel_ranges(&colors);
        let axis = if ranges[0] >= ranges[1] && ranges[0] >= ranges[2] {
            0
        } else if ranges[1] >= ranges[2] {
            1
        } else {
            2
        };
        colors.sort_unstable_by_key(|color| {
            let [r, g, b] = rgb555_components(*color);
            match axis {
                1 => (g << 10) | (r << 5) | b,
                2 => (b << 10) | (g << 5) | r,
                _ => *color as u32,
            }
        });
        let mid = colors.iter().map(|color| histogram[*color]).sum::<u32>() / 2;
        let mut count = 0;
        let split = colors
            .iter()
            .position(|color| {
                count += histogram[*color];
                count >= mid
            })
            .unwrap_or(0)
            .min(colors.len() - 2)
            + 1;
        let upper = colors.split_off(split);
        boxes.push(upper);
        boxes.push(colors);
    }
    boxes
        .into_iter()
        .filter_map(|colors| {
            let mut sum = [0; 3];
            let mut count = 0;
            for color in colors {
                count += histogram[color];
                for (i, value) in rgb555_components(color).into_iter().enumerate() {
                    sum[i] += value * histogram[color];
                }
            }
            let color =
                sum.map(|v| (((v as f32 / count as f32).round() as u32) << 3) as f32 / 255.0);
            allowed(color).then_some((color, count))
        })
        .collect()
}

fn channel_ranges(colors: &[usize]) -> [u32; 3] {
    let mut min = [31; 3];
    let mut max = [0; 3];
    for &color in colors {
        for (i, value) in rgb555_components(color).into_iter().enumerate() {
            min[i] = min[i].min(value);
            max[i] = max[i].max(value);
        }
    }
    std::array::from_fn(|i| max[i] - min[i])
}

fn color_box_volume(colors: &[usize]) -> u32 {
    channel_ranges(colors).map(|v| v + 1).into_iter().product()
}

fn rgb_to_hsl([r, g, b]: Color) -> Color {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta == 0.0 {
        return [0.0, 0.0, l];
    }
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    [h * 60.0, delta / (1.0 - (2.0 * l - 1.0).abs()), l]
}

fn hsl_to_rgb([h, s, l]: Color) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let values = match h as u32 / 60 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    values.map(|v| v + l - c / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artwork_changes_the_mesh_and_cancellation_stops_it() {
        let red = DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 32, Rgb([210, 20, 60])));
        let blue = DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 32, Rgb([20, 60, 210])));
        let a = render(&red, 128, 72, || Some(())).unwrap().to_rgb8();
        let b = render(&blue, 128, 72, || Some(())).unwrap().to_rgb8();
        assert_ne!(a, b);
        assert_eq!(a.dimensions(), (128, 72));
        for y in 0..72 {
            for x in 1..128 {
                for c in 0..3 {
                    assert!(a.get_pixel(x, y)[c].abs_diff(a.get_pixel(x - 1, y)[c]) <= 8);
                }
            }
        }
        assert!(render(&red, 128, 72, || None).is_none());
    }
}
