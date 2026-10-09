use crate::{LyricLine, LyricWord};
use slint::VecModel;
use std::rc::Rc;

pub fn timestamp(text: &str) -> Option<u64> {
    let (minutes, rest) = text.split_once(':')?;
    let rest = rest.replace(':', ".");
    let seconds: f64 = rest.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(
        minutes
            .parse::<u64>()
            .ok()?
            .checked_mul(60000)?
            .saturating_add((seconds * 1000.0).round() as u64),
    )
}
// Blank rows end the preceding lyric interval even though they are not rendered.
pub fn parse(text: &str) -> Vec<LyricLine> {
    let offset = text
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("[offset:")?
                .strip_suffix(']')?
                .trim()
                .parse::<i64>()
                .ok()
        })
        .unwrap_or(0);
    let mut timed: Vec<(f32, String, Vec<LyricWord>)> = vec![];
    let mut plain = vec![];
    let mut boundaries = vec![];
    for raw in text.lines() {
        let mut line = raw.trim();
        let mut times = vec![];
        while line.starts_with('[') {
            let Some(end) = line.find(']') else {
                break;
            };
            let Some(time) = timestamp(&line[1..end]) else {
                line = "";
                break;
            };
            times.push(time as f32);
            line = &line[end + 1..];
        }
        let mut segments = vec![];
        let mut rest = line;
        let mut current = None;
        let mut clean = String::new();
        while let Some(start) = rest.find('<') {
            let Some(end) = rest[start..].find('>') else {
                break;
            };
            if let Some(time) = timestamp(&rest[start + 1..start + end]) {
                let segment = &rest[..start];
                if !segment.is_empty() {
                    segments.push((segment.to_string(), current, Some(time as f32)));
                    clean.push_str(segment);
                }
                current = Some(time as f32);
                rest = &rest[start + end + 1..];
            } else {
                let segment = &rest[..start + end + 1];
                clean.push_str(segment);
                segments.push((segment.to_string(), current, current));
                rest = &rest[start + end + 1..];
            }
        }
        clean.push_str(rest);
        if !rest.is_empty() {
            segments.push((rest.to_string(), current, None));
        }
        for time in &times {
            boundaries.push(*time + offset as f32);
        }
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }
        if times.is_empty() {
            plain.push(LyricLine {
                text: clean.into(),
                time: -1.0,
                words: Default::default(),
            });
            continue;
        }
        let first = times[0];
        for original in times {
            let at = original + offset as f32;
            let shift = original - first + offset as f32;
            let mut words = if current.is_some() {
                segments
                    .iter()
                    .map(|(text, start, end)| {
                        let time = start.map_or(at, |time| time + shift);
                        LyricWord {
                            text: text.clone().into(),
                            time,
                            end_time: end.map_or(f32::NAN, |end| (end + shift).max(time)),
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![]
            };
            if let Some(word) = words.first_mut() {
                word.text = word.text.trim_start().into();
            }
            if let Some(word) = words.last_mut() {
                word.text = word.text.trim_end().into();
            }
            timed.push((at, clean.into(), words));
        }
    }
    boundaries.sort_by(f32::total_cmp);
    for (_, _, words) in &mut timed {
        if let Some(last) = words.last_mut() {
            if !last.end_time.is_finite() {
                last.end_time = boundaries
                    .iter()
                    .copied()
                    .find(|at| *at > last.time)
                    .unwrap_or(last.time + 1000.0);
            }
        }
    }
    timed.sort_by(|a, b| a.0.total_cmp(&b.0));
    if timed.is_empty() {
        plain
    } else {
        timed
            .into_iter()
            .map(|(time, text, words)| LyricLine {
                text: text.into(),
                time,
                words: Rc::new(VecModel::from(words)).into(),
            })
            .collect()
    }
}
pub fn active(lines: &[LyricLine], position: f32) -> i32 {
    if lines.first().map(|line| line.time < 0.0).unwrap_or(true) {
        return -1;
    }
    lines.partition_point(|line| line.time <= position) as i32 - 1
}
#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;
    #[test]
    fn enhanced_markers_and_metadata_are_not_displayed() {
        let lines =
            parse("[ar: Artist]\n[00:20.55]When <00:20.87>I <00:21.02>wake\n[00:30.00]Next");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text.as_str(), "When I wake");
        assert_eq!(active(&lines, 20550.0), 0);
        assert_eq!(lines[0].words.row_count(), 3);
        assert_eq!(lines[0].words.row_data(0).unwrap().time, 20550.0);
        assert_eq!(lines[0].words.row_data(0).unwrap().end_time, 20870.0);
    }
    #[test]
    fn duplicate_line_stamps_and_offsets_are_retained() {
        let lines = parse("[offset:-500]\n[00:01.00][00:03.00]Repeat");
        assert_eq!(lines[0].time, 500.0);
        assert_eq!(lines[1].time, 2500.0);
    }
    #[test]
    fn repeated_lines_shift_word_markers_and_blank_rows_end_last_word() {
        let lines = parse(
            "[offset:500]\n[00:01.00][00:03.00]Hello <00:02.00>world\n[00:04.00]\n[00:06.00]Next",
        );
        let first = lines[0].words.row_data(1).unwrap();
        let repeated = lines[1].words.row_data(1).unwrap();
        assert_eq!((first.time, first.end_time), (2500.0, 3500.0));
        assert_eq!((repeated.time, repeated.end_time), (4500.0, 6500.0));
        assert_eq!(lines.len(), 3);
    }
    #[test]
    fn overlapping_enhanced_lines_preserve_their_own_word_end_times() {
        let lines=parse("[00:57.89]A <00:58.27>lovely <00:58.95>day\n[00:58.83]Lovely <00:59.36>day\n[01:03.71]Next");
        assert_eq!(lines[0].words.row_data(2).unwrap().time, 58950.0);
        assert_eq!(lines[0].words.row_data(2).unwrap().end_time, 63710.0);
    }
}
