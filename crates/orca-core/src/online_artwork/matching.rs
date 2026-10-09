//! Conservative identity matching; release/version qualifiers retain meaning.
use super::ArtworkQuery;
use serde_json::Value;

pub(super) use crate::media_identity::{album_matches, names_match};
pub(super) fn lead_artist(artist: &str) -> &str {
    artist.split(';').next().unwrap_or(artist).trim()
}
pub(super) fn credit_matches(value: &Value, artist: &str) -> bool {
    value["artist-credit"].as_array().is_some_and(|credits| {
        credits.iter().any(|credit| {
            [credit["artist"]["name"].as_str(), credit["name"].as_str()]
                .into_iter()
                .flatten()
                .any(|name| names_match(artist, name))
        })
    })
}
pub(super) fn track_matches(track: &Value, query: &ArtworkQuery) -> bool {
    let artist = lead_artist(query.track_artist.as_deref().unwrap_or(&query.artist));
    track["artistName"]
        .as_str()
        .is_some_and(|name| names_match(artist, name))
        && query.title.as_deref().is_some_and(|title| {
            track["trackName"]
                .as_str()
                .is_some_and(|name| names_match(title, name))
        })
        && (query.album.is_empty()
            || track["collectionName"]
                .as_str()
                .is_some_and(|album| album_matches(&query.album, album)))
        && query
            .duration_ms
            .filter(|&duration| duration > 0)
            .is_none_or(|duration| {
                track["trackTimeMillis"]
                    .as_u64()
                    .is_some_and(|found| found.abs_diff(duration) <= 3000)
            })
}
