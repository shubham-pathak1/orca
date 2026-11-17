//! Map original lyric timing to pronunciation highlight rectangles.
use super::{Line, Span};

pub(super) struct Cluster {
    pub start: f32,
    pub end: f32,
    pub span: Span,
}

pub(super) fn project(line: &Line, clusters: &[Cluster]) -> Vec<Span> {
    let count = line.text.chars().count() as f32;
    if count == 0.0 || line.words.is_empty() || clusters.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut previous = 0.0;
    let mut previous_time = line.words[0].time;
    for word in &line.words {
        let (Some(prefix), Some(text)) = (
            line.text.get(..word.start),
            line.text.get(word.start..word.end),
        ) else {
            continue;
        };
        let start = prefix.chars().count() as f32 / count;
        let end = start + text.chars().count() as f32 / count;
        // Whitespace advances over the original pause, rather than jumping.
        append(
            &mut spans,
            clusters,
            previous,
            start,
            previous_time,
            word.time,
        );
        append(&mut spans, clusters, start, end, word.time, word.end_time);
        previous = end;
        previous_time = word.end_time;
    }
    append(
        &mut spans,
        clusters,
        previous,
        1.0,
        previous_time,
        previous_time,
    );
    spans
}

fn append(
    spans: &mut Vec<Span>,
    clusters: &[Cluster],
    start: f32,
    end: f32,
    time: f32,
    end_time: f32,
) {
    if end <= start {
        return;
    }
    let first = spans.len();
    for cluster in clusters {
        let lo = start.max(cluster.start);
        let hi = end.min(cluster.end);
        if hi <= lo || cluster.end <= cluster.start {
            continue;
        }
        let scale = cluster.span.width / (cluster.end - cluster.start);
        let duration = (end_time - time).max(0.0);
        let mut span = cluster.span.clone();
        span.x += (lo - cluster.start) * scale;
        span.width = (hi - lo) * scale;
        span.time = time + duration * (lo - start) / (end - start);
        span.end_time = time + duration * (hi - start) / (end - start);
        // Merge adjacent clusters to keep the number of animated spans small.
        if spans.len() > first {
            let previous = spans.last_mut().unwrap();
            if (previous.y - span.y).abs() < 0.5
                && (previous.x + previous.width - span.x).abs() < 0.5
            {
                previous.width += span.width;
                previous.end_time = span.end_time;
                continue;
            }
        }
        spans.push(span);
    }
}
