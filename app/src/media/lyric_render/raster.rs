use parley::Layout;

pub(super) fn raster(
    layout: &Layout<()>,
    width: u32,
    height: u32,
    context: &mut swash::scale::ScaleContext,
) -> Vec<u8> {
    let mut bytes = vec![0; width as usize * height as usize * 4];
    let padding = (height as f32 - layout.height()) / 2.0;
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
                    let gx = glyph_x + glyph.x;
                    let gy = line.metrics().baseline + glyph.y + padding;
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
                                if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                                    let at = (py as usize * width as usize + px as usize) * 4;
                                    let alpha =
                                        mask.data[(my * mask.placement.width + mx) as usize];
                                    bytes[at..at + 3].fill(255);
                                    bytes[at + 3] = bytes[at + 3].max(alpha);
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
    bytes
}
