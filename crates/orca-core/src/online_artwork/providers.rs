//! Provider response parsing; identity decisions are delegated to matching.
use super::{
    matching::{album_matches, credit_matches, lead_artist, names_match, track_matches},
    transport::Transport,
    ArtworkKind, ArtworkQuery, LookupError,
};
use serde_json::Value;
use std::{collections::HashSet, time::Duration};
use url::form_urlencoded;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Provider {
    Itunes,
    AppleMusic,
    Deezer,
    MusicBrainz,
}
impl Provider {
    pub fn budget(self) -> Duration {
        Duration::from_secs(match self {
            Self::Itunes | Self::AppleMusic | Self::Deezer => 4,
            Self::MusicBrainz => 8,
        })
    }
}
pub(super) fn candidates(
    provider: Provider,
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    match (provider, &query.kind) {
        (Provider::Itunes, ArtworkKind::Album) => itunes(query, client),
        (Provider::Deezer, ArtworkKind::Album) => deezer_album(query, client),
        (Provider::Deezer, ArtworkKind::Artist) => super::artist::deezer(query, client),
        (Provider::MusicBrainz, ArtworkKind::Album) => musicbrainz_album(query, client),
        (Provider::MusicBrainz, ArtworkKind::Artist) => musicbrainz_artist(query, client),
        _ => Ok(vec![]),
    }
}
pub(super) fn encode(pairs: &[(&str, &str)]) -> String {
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish()
}
pub(super) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, LookupError> {
    value[key].as_array().ok_or(LookupError::Unavailable)
}
pub(super) fn urls(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .filter(|url| !url.is_empty() && seen.insert(url.clone()))
        .take(3)
        .collect()
}
fn itunes(query: &ArtworkQuery, client: &impl Transport) -> Result<Vec<String>, LookupError> {
    if let Some(title) = &query.title {
        let artist = lead_artist(query.track_artist.as_deref().unwrap_or(&query.artist));
        let term = format!("{artist} {title}");
        let response = client.json(&format!(
            "https://itunes.apple.com/search?{}",
            encode(&[
                ("term", &term),
                ("entity", "song"),
                ("media", "music"),
                ("limit", "50")
            ])
        ))?;
        let matches: Vec<_> = array(&response, "results")?
            .iter()
            .filter(|t| track_matches(t, query))
            .collect();
        // Without an album tag, multiple release identities need user input.
        let identities: HashSet<_> = matches
            .iter()
            .filter_map(|t| t["collectionId"].as_u64())
            .collect();
        if !query.album.is_empty() || identities.len() <= 1 {
            let found = urls(matches.into_iter().filter_map(|t| {
                t["artworkUrl100"]
                    .as_str()
                    .map(|url| url.replace("100x100bb", "600x600bb"))
            }));
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    if query.album.is_empty() {
        return Ok(vec![]);
    }
    let term = format!("{} {}", query.artist, query.album);
    let response = client.json(&format!(
        "https://itunes.apple.com/search?{}",
        encode(&[
            ("term", &term),
            ("entity", "album"),
            ("media", "music"),
            ("limit", "50")
        ])
    ))?;
    Ok(urls(
        array(&response, "results")?
            .iter()
            .filter(|album| {
                album["artistName"]
                    .as_str()
                    .is_some_and(|a| names_match(&query.artist, a))
                    && album["collectionName"]
                        .as_str()
                        .is_some_and(|a| album_matches(&query.album, a))
            })
            .filter_map(|a| {
                a["artworkUrl100"]
                    .as_str()
                    .map(|url| url.replace("100x100bb", "600x600bb"))
            }),
    ))
}
fn deezer_album(query: &ArtworkQuery, client: &impl Transport) -> Result<Vec<String>, LookupError> {
    if query.album.is_empty() {
        return Ok(vec![]);
    }
    let q = format!(
        "artist:\"{}\" album:\"{}\"",
        quoted(&query.artist),
        quoted(&query.album)
    );
    let response = client.json(&format!(
        "https://api.deezer.com/search/album?{}",
        encode(&[("q", &q), ("limit", "25")])
    ))?;
    Ok(urls(
        array(&response, "data")?
            .iter()
            .filter(|album| {
                album["artist"]["name"]
                    .as_str()
                    .is_some_and(|a| names_match(&query.artist, a))
                    && album["title"]
                        .as_str()
                        .is_some_and(|a| album_matches(&query.album, a))
            })
            .filter_map(|a| {
                a["cover_xl"]
                    .as_str()
                    .filter(|url| !url.contains("/images/cover//"))
                    .map(str::to_owned)
            }),
    ))
}
pub(super) fn deezer_artist(
    artist: &str,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    let response = client.json(&format!(
        "https://api.deezer.com/search/artist?{}",
        encode(&[("q", artist), ("limit", "25")])
    ))?;
    let matches: Vec<_> = array(&response, "data")?
        .iter()
        .filter(|a| {
            a["name"]
                .as_str()
                .is_some_and(|name| names_match(artist, name))
        })
        .collect();
    let identities: HashSet<_> = matches
        .iter()
        .filter_map(|artist| artist["id"].as_i64())
        .collect();
    if identities.len() > 1 {
        return Err(LookupError::AmbiguousArtist);
    }
    if identities.len() != 1 || matches.is_empty() {
        return Ok(vec![]);
    }
    Ok(urls(
        matches
            .into_iter()
            .flat_map(|a| {
                [a["picture_xl"].as_str(), a["picture_big"].as_str()]
                    .into_iter()
                    .flatten()
            })
            .filter(|url| !url.contains("/images/artist//"))
            .map(str::to_owned),
    ))
}
pub(super) fn quoted(s: &str) -> String {
    s.replace(['\\', '"'], " ")
}
pub(super) fn mb_search(
    entity: &str,
    query: &str,
    client: &impl Transport,
) -> Result<Value, LookupError> {
    client.json(&format!(
        "https://musicbrainz.org/ws/2/{entity}?{}",
        encode(&[("query", query), ("fmt", "json"), ("limit", "25")])
    ))
}
fn musicbrainz_album(
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    if query.album.is_empty() {
        return Ok(vec![]);
    }
    let result = mb_search(
        "release-group",
        &format!(
            "releasegroup:\"{}\" AND artist:\"{}\"",
            quoted(&query.album),
            quoted(&query.artist)
        ),
        client,
    )?;
    let identities: HashSet<_> = array(&result, "release-groups")?
        .iter()
        .filter(|a| {
            a["title"]
                .as_str()
                .is_some_and(|album| album_matches(&query.album, album))
                && credit_matches(a, &query.artist)
        })
        .filter_map(|a| a["id"].as_str())
        .collect();
    if identities.len() != 1 {
        return Ok(vec![]);
    }
    let id = identities.into_iter().next().unwrap();
    let covers = client.json(&format!("https://coverartarchive.org/release-group/{id}"))?;
    Ok(urls(
        array(&covers, "images")?
            .iter()
            .filter(|a| a["front"].as_bool() == Some(true))
            .filter_map(|a| {
                [
                    a["thumbnails"]["1200"].as_str(),
                    a["thumbnails"]["large"].as_str(),
                    a["image"].as_str(),
                ]
                .into_iter()
                .flatten()
                .next()
                .map(str::to_owned)
            }),
    ))
}
fn musicbrainz_artist(
    query: &ArtworkQuery,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    let artist = &query.artist;
    let result = mb_search("artist", &format!("artist:\"{}\"", quoted(artist)), client)?;
    let mut identities: HashSet<_> = array(&result, "artists")?
        .iter()
        .filter(|a| {
            a["name"]
                .as_str()
                .is_some_and(|name| names_match(artist, name))
        })
        .filter_map(|a| a["id"].as_str())
        .collect();
    let contextual;
    if identities.len() > 1 {
        contextual = super::artist::musicbrainz_identity(query, client)?;
        identities.retain(|id| contextual.as_deref() == Some(*id));
        if identities.len() != 1 {
            return Err(LookupError::AmbiguousArtist);
        }
    }
    if identities.len() != 1 {
        return Ok(vec![]);
    }
    let id = identities.into_iter().next().unwrap();
    let detail = client.json(&format!(
        "https://musicbrainz.org/ws/2/artist/{id}?inc=url-rels&fmt=json"
    ))?;
    super::portrait::from_relations(&detail, artist, client)
}
