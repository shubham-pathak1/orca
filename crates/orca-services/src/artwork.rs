//! Translate owned artwork requests without reading files or using the network.
use crate::{
    operation_types::{CollectionKind, CoverFetch},
    types::Track,
};
use orca_core::online_artwork::{ArtistRecording, ArtworkQuery};

pub(super) fn artist_query(fetch: &CoverFetch, tracks: &[Track]) -> ArtworkQuery {
    let mut selected = Vec::new();
    // Prefer evidence from different releases, then fill with different titles.
    for diverse in [true, false] {
        for track in tracks {
            if selected.len() == 3 {
                break;
            }
            if track.title.trim().is_empty()
                || selected
                    .iter()
                    .any(|t: &&Track| t.title.eq_ignore_ascii_case(&track.title))
            {
                continue;
            }
            if diverse
                && selected
                    .iter()
                    .any(|t| t.album.eq_ignore_ascii_case(&track.album))
            {
                continue;
            }
            selected.push(track);
        }
    }
    let mut result = query(fetch, selected.first().copied());
    result.artist_recordings = selected
        .into_iter()
        .skip(1)
        .map(|track| ArtistRecording {
            title: track.title.clone(),
            album: if track.album.trim().eq_ignore_ascii_case("Unknown Album") {
                String::new()
            } else {
                track.album.clone()
            },
            duration_ms: Some(track.duration_ms).filter(|&n| n > 0),
        })
        .collect();
    result
}

pub(super) fn query(fetch: &CoverFetch, track: Option<&Track>) -> ArtworkQuery {
    if fetch.kind == CollectionKind::Artists {
        let mut query = ArtworkQuery::artist(&fetch.key);
        if let Some(track) = track {
            query.title = Some(track.title.clone()).filter(|s| !s.trim().is_empty());
            query.album = if track.album.trim().eq_ignore_ascii_case("Unknown Album") {
                String::new()
            } else {
                track.album.clone()
            };
            query.duration_ms = Some(track.duration_ms).filter(|&n| n > 0);
        }
        return query;
    }
    let artist = if fetch.artist.trim().is_empty() {
        track.map(|t| t.artist.as_str()).unwrap_or_default()
    } else {
        &fetch.artist
    };
    let title = fetch
        .title
        .as_deref()
        .or_else(|| track.map(|t| t.title.as_str()));
    let mut query = ArtworkQuery::album(artist, &fetch.album, title);
    query.track_artist = fetch
        .track_artist
        .clone()
        .or_else(|| track.map(|t| t.artist.clone()));
    query.duration_ms = fetch
        .duration_ms
        .or_else(|| track.map(|t| t.duration_ms))
        .filter(|&n| n > 0);
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artist_evidence_prefers_different_releases_and_is_bounded_to_three_titles() {
        let fetch: CoverFetch =
            serde_json::from_value(serde_json::json!({"kind":"artists","key":"Gini"})).unwrap();
        let tracks: Vec<_> = [
            ("First", "A"),
            ("First", "A"),
            ("Second", "A"),
            ("Third", "B"),
            ("Fourth", "C"),
            ("Fifth", "D"),
        ]
        .into_iter()
        .map(|(title, album)| Track {
            title: title.into(),
            album: album.into(),
            duration_ms: 120000,
            ..Default::default()
        })
        .collect();
        let result = artist_query(&fetch, &tracks);
        assert_eq!(result.title.as_deref(), Some("First"));
        assert_eq!(
            result
                .artist_recordings
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            ["Third", "Fourth"]
        );
    }
    #[test]
    fn artist_query_keeps_recording_evidence_without_using_track_cover() {
        let fetch: CoverFetch =
            serde_json::from_value(serde_json::json!({"kind":"artists","key":"Gini"})).unwrap();
        let track = Track {
            artist: "Gini".into(),
            title: "Sukoon".into(),
            album: "Sukoon".into(),
            duration_ms: 186000,
            artwork: "album.jpg".into(),
            ..Default::default()
        };
        let query = query(&fetch, Some(&track));
        assert_eq!(query.artist, "Gini");
        assert_eq!(query.title.as_deref(), Some("Sukoon"));
        assert_eq!(query.album, "Sukoon");
        assert_eq!(query.duration_ms, Some(186000));
    }
    #[test]
    fn draft_identity_wins_over_stored_tags_and_duration_stays_attached() {
        let fetch: CoverFetch = serde_json::from_value(serde_json::json!({"kind":"albums","key":"Artist:Edited album","artist":"Album artist","album":"Edited album","title":"Edited title","track_artist":"Edited artist","editor_path":"song.flac"})).unwrap();
        let track = Track {
            title: "Old title".into(),
            artist: "Old artist".into(),
            duration_ms: 123000,
            ..Default::default()
        };
        let query = query(&fetch, Some(&track));
        assert_eq!(query.title.as_deref(), Some("Edited title"));
        assert_eq!(query.track_artist.as_deref(), Some("Edited artist"));
        assert_eq!(query.album, "Edited album");
        assert_eq!(query.artist, "Album artist");
        assert_eq!(query.duration_ms, Some(123000));
    }
}
