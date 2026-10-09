//! Artwork lookup facade. Matching, HTTP providers and cache policy stay separate.
mod apple;
mod artist;
mod cache;
mod matching;
mod portrait;
mod providers;
mod transport;

use crate::artwork_cache::ArtworkPaths;
use serde::{Deserialize, Serialize};
use std::{fmt, path::Path, time::Duration};
use transport::{Http, Transport};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtworkKind {
    Album,
    Artist,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtistRecording {
    pub title: String,
    pub album: String,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtworkQuery {
    pub kind: ArtworkKind,
    pub artist: String,
    pub album: String,
    pub title: Option<String>,
    pub track_artist: Option<String>,
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artist_recordings: Vec<ArtistRecording>,
}
impl ArtworkQuery {
    pub fn artist(artist: &str) -> Self {
        Self {
            kind: ArtworkKind::Artist,
            artist: artist.trim().into(),
            album: String::new(),
            title: None,
            track_artist: None,
            duration_ms: None,
            artist_recordings: Vec::new(),
        }
    }
    pub fn album(artist: &str, album: &str, title: Option<&str>) -> Self {
        Self {
            kind: ArtworkKind::Album,
            artist: artist.trim().into(),
            album: album.trim().into(),
            title: title
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().into()),
            track_artist: None,
            duration_ms: None,
            artist_recordings: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupMode {
    Manual,
    Automatic,
}

#[derive(Debug, PartialEq, Eq)]
enum LookupError {
    NotFound,
    AmbiguousArtist,
    PortraitNotFound,
    Unavailable,
    TimedOut,
    InvalidImage,
    Storage(String),
    Cancelled,
}
impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Artwork search cancelled"),
            Self::NotFound => {
                f.write_str("Artwork not found online; check the artist and album tags")
            }
            Self::AmbiguousArtist => {
                f.write_str("Several artists share this name; choose an artist image manually")
            }
            Self::PortraitNotFound => f.write_str(
                "Artist portrait not found; the artist was identified but no image is available",
            ),
            Self::Unavailable => f.write_str(
                "Artwork providers are unavailable; check your connection and try again",
            ),
            Self::TimedOut => f.write_str("Artwork lookup timed out; please try again"),
            Self::InvalidImage => {
                f.write_str("Artwork providers returned invalid images; try choosing an image")
            }
            Self::Storage(error) => write!(f, "Could not cache artwork: {error}"),
        }
    }
}

pub fn fetch(
    query: &ArtworkQuery,
    directory: &Path,
    mode: LookupMode,
) -> Result<ArtworkPaths, String> {
    lookup(query, directory, mode, |provider| {
        Http::new(provider.budget())
    })
    .map_err(|e| e.to_string())
}

pub fn fetch_cancellable(
    query: &ArtworkQuery,
    directory: &Path,
    mode: LookupMode,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<ArtworkPaths, String> {
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(LookupError::Cancelled.to_string());
    }
    lookup(query, directory, mode, |provider| {
        Http::with_stop(provider.budget(), stop)
    })
    .map_err(|e| e.to_string())
}

fn lookup<T: Transport>(
    query: &ArtworkQuery,
    directory: &Path,
    mode: LookupMode,
    transport: impl Fn(providers::Provider) -> T,
) -> Result<ArtworkPaths, LookupError> {
    if query.artist.trim().is_empty() {
        return Err(LookupError::NotFound);
    }
    if let Some(hit) = cache::read(directory, query, mode) {
        return hit;
    }
    // iTunes identifies a recording and its artist; its album art is never
    // mistaken for a portrait. Other providers independently verify the recording.
    let mut identified = query.clone();
    let mut failure = None;
    if query.kind == ArtworkKind::Artist
        && (query.title.is_some() || !query.artist_recordings.is_empty())
    {
        let client = transport(providers::Provider::Itunes);
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        match artist::identify(query, &client) {
            Ok(Some((found, id))) => {
                identified = found;
                let portrait_client = transport(providers::Provider::AppleMusic);
                match apple::portrait(&identified.artist, id, &portrait_client) {
                    Ok(urls) => {
                        for url in urls {
                            match transport::download(&portrait_client, &url, directory) {
                                Ok(paths) => {
                                    if portrait_client.cancelled() {
                                        return Err(LookupError::Cancelled);
                                    }
                                    cache::save(directory, query, Some(&paths));
                                    return Ok(paths);
                                }
                                Err(error @ (LookupError::Cancelled | LookupError::Storage(_))) => {
                                    return Err(error)
                                }
                                Err(error) => record_failure(&mut failure, error),
                            }
                        }
                    }
                    Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
                    Err(LookupError::NotFound) => {}
                    Err(error) => record_failure(&mut failure, error),
                }
            }
            Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
            _ => {} // Identity enrichment must not prevent portrait fallbacks.
        }
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
    }
    let order: &[providers::Provider] = match query.kind {
        ArtworkKind::Album => &[
            providers::Provider::Itunes,
            providers::Provider::Deezer,
            providers::Provider::MusicBrainz,
        ],
        ArtworkKind::Artist => &[
            providers::Provider::Deezer,
            providers::Provider::MusicBrainz,
        ],
    };
    for &provider in order {
        // Each stage reserves its own budget. A stalled provider cannot consume
        // the time reserved for every remaining fallback.
        let client = transport(provider);
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        match providers::candidates(provider, &identified, &client) {
            Ok(urls) => {
                for url in urls {
                    match transport::download(&client, &url, directory) {
                        Ok(paths) => {
                            if client.cancelled() {
                                return Err(LookupError::Cancelled);
                            }
                            cache::save(directory, query, Some(&paths));
                            return Ok(paths);
                        }
                        Err(error @ (LookupError::Storage(_) | LookupError::Cancelled)) => {
                            return Err(error)
                        }
                        Err(error) => record_failure(&mut failure, error),
                    }
                }
            }
            Err(LookupError::NotFound) => {}
            Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
            Err(error) => record_failure(&mut failure, error),
        }
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
    }
    // Completed searches without a safe match are cacheable misses. Timeouts, rate limits,
    // invalid images and connection errors must remain immediately retryable.
    if matches!(
        failure,
        None | Some(LookupError::AmbiguousArtist | LookupError::PortraitNotFound)
    ) {
        cache::save(directory, query, None);
    }
    Err(failure.unwrap_or(LookupError::NotFound))
}

fn record_failure(current: &mut Option<LookupError>, next: LookupError) {
    let priority = |error: &LookupError| match error {
        LookupError::Cancelled | LookupError::Storage(_) => 5,
        LookupError::TimedOut | LookupError::Unavailable | LookupError::InvalidImage => 4,
        LookupError::PortraitNotFound => 2,
        LookupError::AmbiguousArtist => 1,
        LookupError::NotFound => 0,
    };
    if current
        .as_ref()
        .is_none_or(|error| priority(&next) >= priority(error))
    {
        *current = Some(next);
    }
}

// URL-only lookup helpers return release artwork or an artist portrait.
pub fn fetch_itunes_artist_image(artist: &str) -> Option<String> {
    fetch_artist_image(artist)
}
pub fn fetch_artist_image(artist: &str) -> Option<String> {
    let query = ArtworkQuery::artist(artist);
    [
        providers::Provider::Deezer,
        providers::Provider::MusicBrainz,
    ]
    .into_iter()
    .find_map(|p| {
        providers::candidates(p, &query, &Http::new(p.budget()))
            .ok()?
            .into_iter()
            .next()
    })
}
pub fn fetch_itunes_album_art(artist: &str, album: &str) -> Option<String> {
    let query = ArtworkQuery::album(artist, album, None);
    [
        providers::Provider::Itunes,
        providers::Provider::Deezer,
        providers::Provider::MusicBrainz,
    ]
    .into_iter()
    .find_map(|p| {
        providers::candidates(p, &query, &Http::new(p.budget()))
            .ok()?
            .into_iter()
            .next()
    })
}
pub fn fetch_deezer_artist_image(artist: &str) -> Option<String> {
    providers::candidates(
        providers::Provider::Deezer,
        &ArtworkQuery::artist(artist),
        &Http::new(Duration::from_secs(4)),
    )
    .ok()?
    .into_iter()
    .next()
}
pub fn fetch_deezer_album_art(artist: &str, album: &str) -> Option<String> {
    providers::candidates(
        providers::Provider::Deezer,
        &ArtworkQuery::album(artist, album, None),
        &Http::new(Duration::from_secs(4)),
    )
    .ok()?
    .into_iter()
    .next()
}
pub fn fetch_track_art(artist: &str, title: &str) -> Option<String> {
    providers::candidates(
        providers::Provider::Itunes,
        &ArtworkQuery::album(artist, "", Some(title)),
        &Http::new(Duration::from_secs(4)),
    )
    .ok()?
    .into_iter()
    .next()
}
pub fn download_and_cache(
    url: &str,
    directory: &Path,
    _prefix: &str,
) -> Result<ArtworkPaths, String> {
    transport::download(&Http::new(Duration::from_secs(4)), url, directory)
        .map_err(|e| e.to_string())
}
pub fn fetch_album_and_cache(
    artist: &str,
    album: &str,
    title: Option<&str>,
    directory: &Path,
) -> Result<ArtworkPaths, String> {
    fetch(
        &ArtworkQuery::album(artist, album, title),
        directory,
        LookupMode::Manual,
    )
}
pub fn fetch_artist_and_cache(artist: &str, directory: &Path) -> Result<ArtworkPaths, String> {
    fetch(&ArtworkQuery::artist(artist), directory, LookupMode::Manual)
}

#[cfg(test)]
mod tests;
