//! Fresh online lookups prefer synced, identity-checked recordings.
use serde::Deserialize;
use std::time::Duration;
use url::form_urlencoded;

#[derive(Debug, Deserialize)]
pub struct LrclibResponse {
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
    #[serde(rename = "trackName", default)]
    pub track_name: String,
    #[serde(rename = "artistName", default)]
    pub artist_name: String,
    #[serde(rename = "albumName", default)]
    pub album_name: String,
    pub duration: Option<f64>,
}
pub fn fetch_lyrics(title: &str, artist: &str, duration_ms: u64) -> Result<String, String> {
    fetch_lyrics_cancellable(title, artist, duration_ms, None)
}
pub fn fetch_lyrics_cancellable(
    title: &str,
    artist: &str,
    duration_ms: u64,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    fetch_lyrics_with_album_cancellable(title, artist, "", duration_ms, stop)
}
pub fn fetch_lyrics_with_album_cancellable(
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    lookup(
        title,
        artist,
        album,
        duration_ms,
        stop,
        |url| match ureq::get(url)
            .set(
                "User-Agent",
                "Orca/0.1.7 (https://github.com/shubham-pathak1/orca)",
            )
            .timeout(Duration::from_secs(5))
            .call()
        {
            Ok(response) => read_response(response),
            Err(ureq::Error::Status(404, _)) => Err("Lyrics not found".into()),
            Err(error) => Err(error.to_string()),
        },
    )
}
fn lookup(
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
    stop: Option<&std::sync::atomic::AtomicBool>,
    mut request: impl FnMut(&str) -> Result<serde_json::Value, String>,
) -> Result<String, String> {
    check_stop(stop)?;
    let album = if album.trim().eq_ignore_ascii_case("Unknown Album") {
        ""
    } else {
        album.trim()
    };
    let mut params = form_urlencoded::Serializer::new(String::new());
    params
        .append_pair("track_name", title)
        .append_pair("artist_name", artist);
    if !album.trim().is_empty() {
        params.append_pair("album_name", album);
    }
    if duration_ms > 0 {
        params.append_pair("duration", &(duration_ms / 1000).to_string());
    }
    let get =
        request(&format!("https://lrclib.net/api/get?{}", params.finish())).and_then(|value| {
            serde_json::from_value::<LrclibResponse>(value).map_err(|e| e.to_string())
        });
    check_stop(stop)?;
    let mut plain = None;
    let mut failure = None;
    match get {
        Ok(result) if matches_identity(&result, title, artist, album, duration_ms) => {
            if let Some(synced) = nonempty(result.synced_lyrics) {
                return Ok(synced);
            }
            plain = nonempty(result.plain_lyrics);
        }
        Err(error) => failure = Some(error),
        _ => {}
    }
    // Plain-only /get does not prove that synced lyrics are absent.
    check_stop(stop)?;
    let params = form_urlencoded::Serializer::new(String::new())
        .append_pair("track_name", title)
        .append_pair("artist_name", artist)
        .finish();
    let search = request(&format!("https://lrclib.net/api/search?{params}")).and_then(|value| {
        serde_json::from_value::<Vec<LrclibResponse>>(value).map_err(|e| e.to_string())
    });
    check_stop(stop)?;
    match search {
        Ok(mut results) => {
            results.retain(|r| matches_identity(r, title, artist, album, duration_ms));
            results.sort_by(|a, b| {
                let distance = |r: &LrclibResponse| {
                    r.duration
                        .map(|d| (d * 1000.0 - duration_ms as f64).abs())
                        .unwrap_or(f64::MAX)
                };
                distance(a).total_cmp(&distance(b))
            });
            for result in results {
                if let Some(synced) = nonempty(result.synced_lyrics) {
                    return Ok(synced);
                }
                if plain.is_none() {
                    plain = nonempty(result.plain_lyrics);
                }
            }
        }
        Err(error) => failure = Some(error),
    }
    plain
        .ok_or_else(|| failure.unwrap_or_else(|| "No lyrics found for a matching recording".into()))
}
fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}
fn matches_identity(
    result: &LrclibResponse,
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
) -> bool {
    use crate::media_identity::{album_matches, names_match};
    names_match(title, &result.track_name)
        && (names_match(artist, &result.artist_name)
            || artist
                .split(';')
                .any(|a| names_match(a, &result.artist_name)))
        && (album.is_empty() || album_matches(album, &result.album_name))
        && (duration_ms == 0
            || result.duration.is_some_and(|d| {
                d.is_finite() && d > 0.0 && (d * 1000.0 - duration_ms as f64).abs() <= 3000.0
            }))
}
fn check_stop(stop: Option<&std::sync::atomic::AtomicBool>) -> Result<(), String> {
    if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err("Lyrics search cancelled".into());
    }
    Ok(())
}
fn read_response<T: serde::de::DeserializeOwned>(response: ureq::Response) -> Result<T, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Lyrics response exceeds 1 MB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    fn result(title: &str, duration: f64, synced: Option<&str>) -> serde_json::Value {
        json!({"trackName":title,"artistName":"Artist","albumName":"Album","duration":duration,"syncedLyrics":synced,"plainLyrics":"Plain text"})
    }
    #[test]
    fn plain_get_searches_all_recordings_and_prefers_matching_synced_lyrics() {
        let mut replies = vec![
            Ok(result("Song", 120.0, None)),
            Ok(json!([
                result("Different song", 120.0, Some("wrong title")),
                result("Song", 180.0, Some("wrong duration")),
                result("Song", 120.0, None),
                result("Song", 121.0, Some("[00:01.00]Synced text"))
            ])),
        ]
        .into_iter();
        let mut calls = Vec::new();
        assert_eq!(
            lookup("Song", "Artist", "Album", 120000, None, |url| {
                calls.push(url.to_owned());
                replies.next().unwrap()
            })
            .unwrap(),
            "[00:01.00]Synced text"
        );
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains("duration=120") && calls[0].contains("album_name=Album"));
    }
    #[test]
    fn synced_get_avoids_search_and_plain_get_survives_search_failure() {
        let mut calls = 0;
        assert_eq!(
            lookup("Song", "Artist", "Album", 120000, None, |_| {
                calls += 1;
                Ok(result("Song", 120.0, Some("[00:01.00]Synced")))
            })
            .unwrap(),
            "[00:01.00]Synced"
        );
        assert_eq!(calls, 1);
        let mut replies = vec![Ok(result("Song", 120.0, None)), Err("offline".into())].into_iter();
        assert_eq!(
            lookup("Song", "Artist", "Album", 120000, None, |_| replies
                .next()
                .unwrap())
            .unwrap(),
            "Plain text"
        );
    }
    #[test]
    fn unknown_album_placeholder_does_not_reject_a_real_release() {
        let lyrics = lookup("Song", "Artist", "Unknown Album", 120000, None, |url| {
            assert!(!url.contains("album_name="));
            Ok(result("Song", 120.0, Some("[00:01.00]Synced")))
        })
        .unwrap();
        assert_eq!(lyrics, "[00:01.00]Synced");
    }
    #[test]
    fn cancellation_never_contacts_lrclib_or_returns_a_partial_result() {
        let stop = AtomicBool::new(true);
        assert!(lookup("Song", "Artist", "", 0, Some(&stop), |_| panic!(
            "no network"
        ))
        .unwrap_err()
        .contains("cancelled"));
        stop.store(false, Ordering::Relaxed);
        assert!(lookup("Song", "Artist", "Album", 120000, Some(&stop), |_| {
            stop.store(true, Ordering::Relaxed);
            Ok(result("Song", 120.0, None))
        })
        .unwrap_err()
        .contains("cancelled"));
    }
    #[test]
    fn search_rejects_other_artists_albums_and_missing_identity() {
        let mut wrong = result("Song", 120.0, Some("wrong artist"));
        wrong["artistName"] = json!("Other artist");
        let mut wrong_album = result("Song", 120.0, Some("wrong album"));
        wrong_album["albumName"] = json!("Album Live");
        let mut replies = vec![
            Err("Lyrics not found".into()),
            Ok(json!([wrong, wrong_album, {"syncedLyrics":"no identity"}])),
        ]
        .into_iter();
        assert!(lookup("Song", "Artist", "Album", 120000, None, |_| replies
            .next()
            .unwrap())
        .is_err());
    }
}
