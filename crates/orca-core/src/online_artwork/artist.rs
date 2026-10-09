//! Combine recording evidence using provider-local identities, never release images.
use super::{
    matching::{album_matches, names_match, track_matches},
    providers::{array, encode, mb_search, quoted, urls},
    transport::Transport,
    ArtworkQuery, LookupError,
};
use serde_json::Value;
use std::collections::HashSet;

fn recordings(query: &ArtworkQuery) -> Vec<ArtworkQuery> {
    let mut result = Vec::new();
    if query.title.as_ref().is_some_and(|t| !t.trim().is_empty()) {
        let mut first = query.clone();
        first.artist_recordings.clear();
        result.push(first);
    }
    for recording in query.artist_recordings.iter().take(3) {
        if result.len() == 3 {
            break;
        }
        if recording.title.trim().is_empty()
            || result.iter().any(|q| {
                q.title
                    .as_deref()
                    .is_some_and(|t| names_match(t, &recording.title))
            })
        {
            continue;
        }
        let mut next = ArtworkQuery::artist(&query.artist);
        next.title = Some(recording.title.clone());
        next.album = recording.album.clone();
        next.duration_ms = recording.duration_ms.filter(|&d| d > 0);
        result.push(next);
    }
    result
}

// Prefer exact release evidence. Different tagging/mastering labels may still
// identify the same artist when title AND duration agree. Without duration, an
// explicit album hint remains required.
fn prefer_album<'a>(
    mut matches: Vec<&'a Value>,
    query: &ArtworkQuery,
    album: impl Fn(&Value) -> Option<&str>,
) -> Vec<&'a Value> {
    if !query.album.is_empty() {
        let exact = |value: &&Value| album(value).is_some_and(|a| album_matches(&query.album, a));
        if matches.iter().any(exact) || query.duration_ms.is_none() {
            matches.retain(exact);
        }
    }
    matches
}

fn intersect<T: Eq + std::hash::Hash>(
    evidence: &mut Option<HashSet<T>>,
    next: HashSet<T>,
) -> Result<(), LookupError> {
    // Missing provider coverage is not contradictory evidence.
    if next.is_empty() {
        return Ok(());
    }
    if let Some(current) = evidence {
        current.retain(|id| next.contains(id));
        if current.is_empty() {
            return Err(LookupError::AmbiguousArtist);
        }
    } else {
        *evidence = Some(next);
    }
    Ok(())
}

pub(super) fn identify(
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Option<(ArtworkQuery, u64)>, LookupError> {
    let mut evidence = None;
    let mut canonical = None;
    for recording in recordings(query) {
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        let term = format!("{} {}", query.artist, recording.title.as_deref().unwrap());
        let response = client.json(&format!(
            "https://itunes.apple.com/search?{}",
            encode(&[
                ("term", &term),
                ("entity", "song"),
                ("media", "music"),
                ("limit", "50")
            ])
        ))?;
        let mut title_query = recording.clone();
        title_query.album.clear();
        let matches = prefer_album(
            array(&response, "results")?
                .iter()
                .filter(|t| track_matches(t, &title_query))
                .collect(),
            &recording,
            |t| t["collectionName"].as_str(),
        );
        let ids = matches
            .iter()
            .filter_map(|t| t["artistId"].as_u64())
            .collect();
        intersect(&mut evidence, ids)?;
        if let Some(name) = matches.first().and_then(|t| t["artistName"].as_str()) {
            canonical = Some(name.to_owned());
        }
    }
    if let Some(id) = evidence
        .filter(|ids| ids.len() == 1)
        .and_then(|ids| ids.into_iter().next())
    {
        let mut found = query.clone();
        if let Some(name) = canonical {
            found.artist = name;
        }
        Ok(Some((found, id)))
    } else {
        Ok(None)
    }
}

pub(super) fn deezer(
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    let mut evidence = None;
    for recording in recordings(query) {
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        let title = recording.title.as_deref().unwrap();
        let search = format!(
            "artist:\"{}\" track:\"{}\"",
            quoted(&query.artist),
            quoted(title)
        );
        let response = client.json(&format!(
            "https://api.deezer.com/search?{}",
            encode(&[("q", &search), ("limit", "25")])
        ))?;
        let matches = prefer_album(
            array(&response, "data")?
                .iter()
                .filter(|t| {
                    t["artist"]["name"]
                        .as_str()
                        .is_some_and(|n| names_match(&query.artist, n))
                        && t["title"].as_str().is_some_and(|n| names_match(title, n))
                        && recording.duration_ms.is_none_or(|d| {
                            t["duration"].as_u64().is_some_and(|seconds| {
                                seconds.saturating_mul(1000).abs_diff(d) <= 3000
                            })
                        })
                })
                .collect(),
            &recording,
            |t| t["album"]["title"].as_str(),
        );
        intersect(
            &mut evidence,
            matches
                .iter()
                .filter_map(|t| t["artist"]["id"].as_u64())
                .collect(),
        )?;
    }
    if let Some(ids) = evidence {
        if ids.len() != 1 {
            return Err(LookupError::AmbiguousArtist);
        }
        let id = ids.into_iter().next().unwrap();
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        let artist = client.json(&format!("https://api.deezer.com/artist/{id}"))?;
        if artist["id"].as_u64() != Some(id)
            || !artist["name"]
                .as_str()
                .is_some_and(|n| names_match(&query.artist, n))
        {
            return Err(LookupError::AmbiguousArtist);
        }
        // Do not substitute another same-name artist when this verified artist
        // has no portrait.
        return Ok(urls(
            [
                artist["picture_xl"].as_str(),
                artist["picture_big"].as_str(),
            ]
            .into_iter()
            .flatten()
            .filter(|url| !url.contains("/images/artist//"))
            .map(str::to_owned),
        ));
    }
    super::providers::deezer_artist(&query.artist, client)
}

pub(super) fn musicbrainz_identity(
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Option<String>, LookupError> {
    let mut evidence = None;
    for recording in recordings(query) {
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        let title = recording.title.as_deref().unwrap();
        let search = format!(
            "recording:\"{}\" AND artist:\"{}\"",
            quoted(title),
            quoted(&query.artist)
        );
        let result = mb_search("recording", &search, client)?;
        let mut matches: Vec<_> = array(&result, "recordings")?
            .iter()
            .filter(|r| {
                r["title"].as_str().is_some_and(|n| names_match(title, n))
                    && recording
                        .duration_ms
                        .is_none_or(|d| r["length"].as_u64().is_some_and(|n| n.abs_diff(d) <= 3000))
            })
            .collect();
        if !recording.album.is_empty() {
            let exact = |r: &&Value| {
                r["releases"].as_array().is_some_and(|releases| {
                    releases.iter().any(|release| {
                        release["title"]
                            .as_str()
                            .is_some_and(|n| album_matches(&recording.album, n))
                    })
                })
            };
            if matches.iter().any(exact) || recording.duration_ms.is_none() {
                matches.retain(exact);
            }
        }
        let ids = matches
            .into_iter()
            .filter_map(|r| r["artist-credit"].as_array())
            .flatten()
            .filter(|credit| {
                credit["artist"]["name"]
                    .as_str()
                    .is_some_and(|n| names_match(&query.artist, n))
            })
            .filter_map(|credit| credit["artist"]["id"].as_str().map(str::to_owned))
            .collect();
        intersect(&mut evidence, ids)?;
    }
    Ok(evidence
        .filter(|ids| ids.len() == 1)
        .and_then(|ids| ids.into_iter().next()))
}
