//! Shared conservative metadata matching for online media lookups.
pub(crate) fn names_match(query: &str, result: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let query = normalize(query);
    !query.is_empty() && query == normalize(result)
}
pub(crate) fn album_matches(query: &str, result: &str) -> bool {
    fn album(s: &str) -> &str {
        let s = s.trim();
        for suffix in [" - Single", " - EP"] {
            if s.to_lowercase().ends_with(&suffix.to_lowercase()) {
                return s[..s.len() - suffix.len()].trim();
            }
        }
        s
    }
    names_match(album(query), album(result))
}
