use parley::Layout;

pub(super) struct Rasterized {
    pub bytes: Vec<u8>,
    pub ink_top: u32,
    pub ink_bottom: u32,
}

#[cfg(test)]
pub(super) fn raster(
    layout: &Layout<()>,
    width: u32,
    height: u32,
    context: &mut swash::scale::ScaleContext,
) -> Rasterized {
    raster_padded(layout, width, height, 0, context)
}

pub(super) fn raster_padded(
    layout: &Layout<()>,
    width: u32,
    height: u32,
    padding: u32,
    context: &mut swash::scale::ScaleContext,
) -> Rasterized {
    let canvas_width = width + padding * 2;
    let canvas_height = height + padding * 2;
    let mut bytes = vec![0; canvas_width as usize * canvas_height as usize * 4];
    let mut ink_top = canvas_height;
    let mut ink_bottom = 0;
    let vertical_padding = (height as f32 - layout.height()) / 2.0;
    for line in layout.lines() {
        let mut x = line.metrics().offset + line.metrics().inline_min_coord;
        for run in line.runs() {
            let Some(font) =
                swash::FontRef::from_index(run.font().data.as_ref(), run.font().index as usize)
            else {
                x += run.advance();
                continue;
            };
            let mut scaler = context
                .builder(font)
                .size(run.font_size())
                .normalized_coords(run.normalized_coords())
                .hint(true)
                .build();
            for cluster in run.visual_clusters() {
                let mut glyph_x = x;
                for glyph in cluster.glyphs() {
                    let gx = glyph_x + glyph.x + padding as f32;
                    let gy = line.metrics().baseline + glyph.y + vertical_padding + padding as f32;
                    if let Some(mask) = swash::scale::Render::new(&[swash::scale::Source::Outline])
                        .offset(swash::zeno::Vector::new(gx.fract(), gy.fract()))
                        .render(&mut scaler, glyph.id as u16)
                    {
                        let left = gx.floor() as i32 + mask.placement.left;
                        let top = gy.floor() as i32 - mask.placement.top;
                        for my in 0..mask.placement.height {
                            for mx in 0..mask.placement.width {
                                let px = left + mx as i32;
                                let py = top + my as i32;
                                if px >= 0
                                    && py >= 0
                                    && px < canvas_width as i32
                                    && py < canvas_height as i32
                                {
                                    let at =
                                        (py as usize * canvas_width as usize + px as usize) * 4;
                                    let alpha =
                                        mask.data[(my * mask.placement.width + mx) as usize];
                                    bytes[at..at + 3].fill(255);
                                    bytes[at + 3] = bytes[at + 3].max(alpha);
                                    if alpha > 0 {
                                        ink_top = ink_top.min(py as u32);
                                        ink_bottom = ink_bottom.max(py as u32 + 1);
                                    }
                                }
                            }
                        }
                    }
                    glyph_x += glyph.advance;
                }
                x += cluster.advance();
            }
        }
    }
    Rasterized {
        bytes,
        ink_top,
        ink_bottom,
    }
}

// Blur alpha on the raster worker; keep RGB white to avoid dark edges.
pub(super) fn blur_alpha(bytes: &[u8], width: u32, height: u32, sigma: f32) -> Vec<u8> {
    if sigma <= 0.0 || !sigma.is_finite() {
        return bytes.to_vec();
    }
    let alpha = image::GrayImage::from_fn(width, height, |x, y| {
        image::Luma([bytes[((y * width + x) * 4 + 3) as usize]])
    });
    let blurred = image::imageops::fast_blur(&alpha, sigma);
    blurred
        .pixels()
        .flat_map(|pixel| [255, 255, 255, pixel[0]])
        .collect()
}
