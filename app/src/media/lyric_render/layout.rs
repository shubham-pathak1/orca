use super::{Config, Line, Prepared, Span};
use parley::{Alignment, AlignmentOptions, FontContext, FontWeight, LayoutContext, StyleProperty};
use std::collections::HashMap;

pub(super) fn shape(
    fonts: &mut FontContext,
    context: &mut LayoutContext<()>,
    line: Line,
    config: &Config,
) -> Prepared {
    let mut builder = context.ranged_builder(fonts, &line.text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(config.size as f32));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(900.0)));
    builder.push_default(StyleProperty::OverflowWrap(parley::OverflowWrap::BreakWord));
    if !config.family.is_empty() {
        builder.push_default(StyleProperty::FontFamily(config.family.as_str().into()));
    }
    let mut layout = builder.build(&line.text);
    layout.break_all_lines(Some(config.width as f32));
    layout.align(Alignment::Center, AlignmentOptions::default());
    let height = (layout.height() + 32.0 * config.dpr)
        .ceil()
        .max(84.0 * config.dpr) as u32;
    let padding = (height as f32 - layout.height()) / 2.0;
    let mut spans = vec![];
    for row in layout.lines() {
        let metrics = row.metrics();
        let mut x = metrics.offset + metrics.inline_min_coord;
        for run in row.runs() {
            for cluster in run.visual_clusters() {
                let range = cluster.text_range();
                let advance = cluster.advance();
                if let Some(word) = line
                    .words
                    .iter()
                    .find(|word| word.start < range.end && word.end > range.start)
                {
                    let y = metrics.baseline - metrics.ascent + padding;
                    if let Some(previous) = spans.last_mut().filter(|span: &&mut Span| {
                        span.time == word.time
                            && (span.y - y).abs() < 0.5
                            && (span.x + span.width - x).abs() < 0.5
                    }) {
                        previous.width += advance;
                    } else {
                        spans.push(Span {
                            x,
                            y,
                            width: advance,
                            height: metrics.line_height,
                            time: word.time,
                            end_time: word.end_time,
                            rtl: run.is_rtl(),
                        });
                    }
                }
                x += advance;
            }
        }
    }
    // A word can wrap across multiple rows. Divide its time interval between
    // those rectangles so the second row doesn't illuminate prematurely.
    let mut totals = HashMap::<(u32, u32), f32>::new();
    for span in &spans {
        *totals
            .entry((span.time.to_bits(), span.end_time.to_bits()))
            .or_default() += span.width;
    }
    let mut covered = HashMap::<(u32, u32), f32>::new();
    for span in &mut spans {
        let key = (span.time.to_bits(), span.end_time.to_bits());
        let total = totals[&key].max(0.001);
        let at = covered.entry(key).or_default();
        let start = span.time;
        let duration = (span.end_time - start).max(0.0);
        span.time = start + duration * (*at / total);
        *at += span.width;
        span.end_time = start + duration * (*at / total);
    }
    Prepared {
        layout,
        spans,
        height,
    }
}
