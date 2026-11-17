//! Artist-browser membership. Keep the original track credit intact for display and tags.

fn feature_marker(text: &str) -> Option<(usize, usize)> {
    let lower = text.to_ascii_lowercase();
    ["featuring", "feat.", "feat", "ft.", "ft"]
        .into_iter()
        .flat_map(|marker| lower.match_indices(marker).map(move |(at, _)| (at, marker)))
        .filter(|(at, marker)| {
            (*at == 0
                || text[..*at]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '[')))
                && text[at + marker.len()..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_whitespace() || matches!(ch, ':' | ')'))
        })
        .map(|(at, marker)| (at, at + marker.len()))
        .min_by_key(|(at, _)| *at)
}

fn primary_part(credit: &str) -> &str {
    let end = feature_marker(credit).map_or(credit.len(), |(at, _)| at);
    credit[..end].trim().trim_end_matches(['(', '[']).trim_end()
}

fn featured_in_title(title: &str) -> Option<&str> {
    let (_, end) = feature_marker(title)?;
    Some(
        title[end..]
            .trim_start_matches([' ', ':'])
            .split([')', ']'])
            .next()
            .unwrap_or_default()
            .trim(),
    )
}

fn credited_in(featured: &str, name: &str) -> bool {
    let featured = featured.to_lowercase();
    let name = name.to_lowercase();
    featured.match_indices(&name).any(|(at, _)| {
        let before = featured[..at].chars().next_back();
        let after = featured[at + name.len()..].chars().next();
        before.is_none_or(|ch| !ch.is_alphanumeric())
            && after.is_none_or(|ch| !ch.is_alphanumeric())
    })
}

fn primary_credit_spans(artist: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut base = 0;
    for credit in artist.split(';') {
        let primary = primary_part(credit);
        let offset = base + credit.find(primary).unwrap_or(0);
        let mut start = 0;
        for (at, _) in primary.match_indices(',') {
            let following = primary[at + 1..]
                .split(',')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_end_matches('.');
            if ["jr", "sr", "ii", "iii", "iv"]
                .iter()
                .any(|suffix| following.eq_ignore_ascii_case(suffix))
            {
                continue;
            }
            result.push((offset + start, &primary[start..at]));
            start = at + 1;
        }
        result.push((offset + start, &primary[start..]));
        base += credit.len() + 1;
    }
    result
}

/// Semicolons and commas separate co-primary credits. Generational suffixes
/// remain attached; an inline `feat.` suffix is not a primary browser entry.
pub fn primary_artists(artist: &str, title: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (_, credit) in primary_credit_spans(artist) {
        let name = credit.trim();
        if !name.is_empty()
            && !names
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(name))
        {
            names.push(name.to_string());
        }
    }
    if names.len() > 1 {
        if let Some(featured) = featured_in_title(title) {
            let primary: Vec<_> = names
                .iter()
                .filter(|name| !credited_in(featured, name))
                .cloned()
                .collect();
            if !primary.is_empty() {
                names = primary;
            }
        }
    }
    names
}

pub fn contains(artist: &str, title: &str, name: &str) -> bool {
    primary_artists(artist, title)
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// Rename only the selected primary credit, preserving collaborators and the
/// original separators in the embedded track-artist tag.
pub fn rename(artist: &str, title: &str, old: &str, new: &str) -> Option<String> {
    if !contains(artist, title, old) {
        return None;
    }
    for (offset, credit) in primary_credit_spans(artist) {
        let name = credit.trim();
        if name.eq_ignore_ascii_case(old) {
            let at = offset + credit.find(name)?;
            let mut renamed = artist.to_string();
            renamed.replace_range(at..at + name.len(), new);
            return Some(renamed);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn co_primary_credit_belongs_to_both_artists() {
        assert_eq!(
            primary_artists("Raghav Kaushik; Yashika Sikka", "Khaare Raste"),
            ["Raghav Kaushik", "Yashika Sikka"]
        );
        assert_eq!(
            rename(
                "Raghav Kaushik; Yashika Sikka",
                "Khaare Raste",
                "Yashika Sikka",
                "Yashika Sikka 2"
            ),
            Some("Raghav Kaushik; Yashika Sikka 2".into())
        );
    }

    #[test]
    fn featured_artist_stays_in_track_credit_but_not_artist_browser() {
        let credit = "again&again; Abhi The Nomad";
        let title = "visa (feat. Abhi The Nomad)";
        assert_eq!(primary_artists(credit, title), ["again&again"]);
        assert_eq!(rename(credit, title, "Abhi The Nomad", "Guest"), None);
        assert_eq!(
            rename(credit, title, "again&again", "Again"),
            Some("Again; Abhi The Nomad".into())
        );
        assert_eq!(primary_artists("Main feat. Guest", "Song"), ["Main"]);
        assert_eq!(
            primary_artists(
                "Hans Zimmer, The Magnificent Six, Pharrell Williams",
                "Song"
            ),
            ["Hans Zimmer", "The Magnificent Six", "Pharrell Williams"]
        );
        assert_eq!(
            primary_artists("Grover Washington, Jr.; Artist", "Song"),
            ["Grover Washington, Jr.", "Artist"]
        );
        assert_eq!(
            rename("First, Second; Third", "Song", "Second", "New"),
            Some("First, New; Third".into())
        );
    }
}
