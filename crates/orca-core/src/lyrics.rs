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
#[derive(Debug, Deserialize)]
struct LrcRedResponse {
    hits: Vec<LrcRedHit>,
}
#[derive(Debug, Deserialize)]
struct LrcRedHit {
    isrc: String,
    title: String,
    artist: String,
    album: String,
    duration: Option<f64>,
}
struct RecordingIdentity<'a> {
    title: &'a str,
    artist: &'a str,
    album: &'a str,
    duration: Option<f64>,
}
struct RequestedIdentity<'a> {
    title: &'a str,
    artist: &'a str,
    album: &'a str,
    duration_ms: u64,
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
    check_stop(stop)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(6);
    let finished = std::sync::atomic::AtomicBool::new(false);
    let agent = ureq::AgentBuilder::new().build();
    let request = |url: &str| -> Result<ureq::Response, String> {
        for attempt in 0..2 {
            check_stop(stop)?;
            if finished.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Lyrics search cancelled".into());
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("Lyrics search timed out".into());
            }
            let response = agent
                .get(url)
                .set(
                    "User-Agent",
                    concat!(
                        "Orca/",
                        env!("CARGO_PKG_VERSION"),
                        " (https://github.com/shubham-pathak1/orca)"
                    ),
                )
                .timeout(remaining.min(Duration::from_secs(2)))
                .call();
            match response {
                Ok(value) => return Ok(value),
                Err(ureq::Error::Status(404, _)) => return Err("Lyrics not found".into()),
                Err(error) => {
                    let transient = matches!(
                        &error,
                        ureq::Error::Transport(_) | ureq::Error::Status(500..=504, _)
                    );
                    if attempt == 1 || !transient {
                        return Err(error.to_string());
                    }
                }
            }
        }
        unreachable!()
    };
    // Only two provider workers exist per lookup. Scoped threads always join,
    // and share the same deadline and cancellation boundary.
    let (red, lib) = provider_pair(
        || {
            let result = lookup_lrc_red(
                title,
                artist,
                album,
                duration_ms,
                stop,
                |url| read_response(request(url)?),
                |url| read_text_response(request(url)?),
            );
            if matches!(&result, Ok(Some(_))) {
                finished.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            result
        },
        || {
            lookup(title, artist, album, duration_ms, stop, |url| {
                read_response(request(url)?)
            })
        },
    );
    check_stop(stop)?;
    choose_result(red, lib)
}
fn provider_pair(
    red: impl FnOnce() -> Result<Option<String>, String> + Send,
    lib: impl FnOnce() -> Result<String, String> + Send,
) -> (Result<Option<String>, String>, Result<String, String>) {
    std::thread::scope(|scope| {
        let red = scope.spawn(red);
        let lib = scope.spawn(lib);
        (
            red.join()
                .unwrap_or_else(|_| Err("Lyrics provider failed".into())),
            lib.join()
                .unwrap_or_else(|_| Err("Lyrics provider failed".into())),
        )
    })
}
fn choose_result(
    red: Result<Option<String>, String>,
    lib: Result<String, String>,
) -> Result<String, String> {
    if let Ok(Some(lyrics)) = red {
        return Ok(lyrics);
    }
    lib.map_err(|error| {
        if error == "No lyrics found for a matching recording" {
            red.err().unwrap_or(error)
        } else {
            error
        }
    })
}
#[cfg(test)]
fn lookup_with_providers(
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
    stop: Option<&std::sync::atomic::AtomicBool>,
    mut request_json: impl FnMut(&str) -> Result<serde_json::Value, String>,
    mut request_text: impl FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    check_stop(stop)?;
    let red = lookup_lrc_red(
        title,
        artist,
        album,
        duration_ms,
        stop,
        &mut request_json,
        &mut request_text,
    );
    check_stop(stop)?;
    if let Ok(Some(lyrics)) = &red {
        return Ok(lyrics.clone());
    }
    lookup(title, artist, album, duration_ms, stop, request_json).map_err(|error| {
        if error == "No lyrics found for a matching recording" {
            red.err().unwrap_or(error)
        } else {
            error
        }
    })
}
fn lookup_lrc_red(
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
    stop: Option<&std::sync::atomic::AtomicBool>,
    mut request_json: impl FnMut(&str) -> Result<serde_json::Value, String>,
    mut request_text: impl FnMut(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    let album = if placeholder_album(album) {
        ""
    } else {
        album.trim()
    };
    let primary_artist = artist.split(';').next().unwrap_or(artist).trim();
    let short_title = title_without_feature(title).unwrap_or(title);
    let fallback =
        (short_title != title || primary_artist != artist).then_some((short_title, primary_artist));
    let mut downloads = 0;
    for (search_title, search_artist) in [Some((title, artist)), fallback].into_iter().flatten() {
        check_stop(stop)?;
        let mut params = form_urlencoded::Serializer::new(String::new());
        params
            .append_pair("track", search_title)
            .append_pair("artist", search_artist);
        if !album.is_empty() {
            params.append_pair("album", album);
        }
        if duration_ms > 0 {
            params.append_pair("duration", &(duration_ms / 1000).to_string());
        }
        let value = request_json(&format!("https://lrc.red/match.json?{}", params.finish()))?;
        check_stop(stop)?;
        let mut hits = serde_json::from_value::<LrcRedResponse>(value)
            .map_err(|error| error.to_string())?
            .hits;
        hits.retain(|hit| {
            valid_isrc(&hit.isrc)
                && matches_identity_fields(
                    RecordingIdentity {
                        title: &hit.title,
                        artist: &hit.artist,
                        album: &hit.album,
                        duration: hit.duration,
                    },
                    RequestedIdentity {
                        title,
                        artist,
                        album,
                        duration_ms,
                    },
                )
        });
        hits.sort_by(|a, b| {
            let distance = |hit: &LrcRedHit| {
                hit.duration
                    .map(|d| (d * 1000.0 - duration_ms as f64).abs())
                    .unwrap_or(f64::MAX)
            };
            distance(a).total_cmp(&distance(b))
        });
        for hit in hits {
            if downloads >= 2 {
                break;
            }
            check_stop(stop)?;
            downloads += 1;
            let lyrics = request_text(&format!("https://lrc.red/s/{}.lrc", hit.isrc));
            check_stop(stop)?;
            if let Ok(lyrics) = lyrics {
                if has_timed_lines(&lyrics) {
                    return Ok(Some(lyrics));
                }
            }
        }
        if downloads >= 2 {
            break;
        }
    }
    Ok(None)
}
fn valid_isrc(value: &str) -> bool {
    value.len() == 12 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}
fn has_timed_lines(value: &str) -> bool {
    value.lines().any(|line| {
        let bytes = line.as_bytes();
        bytes.len() >= 7
            && bytes[0] == b'['
            && bytes[1..3].iter().all(u8::is_ascii_digit)
            && bytes[3] == b':'
            && bytes[4..6].iter().all(u8::is_ascii_digit)
            && matches!(bytes[6], b'.' | b']')
    })
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
    let album = if placeholder_album(album) {
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
    // Plain-only /get does not prove that synced lyrics are absent. A featured
    // credit in a local title may also be absent from the provider's title.
    let primary_artist = artist.split(';').next().unwrap_or(artist).trim();
    let short_title = title_without_feature(title).unwrap_or(title);
    let fallback =
        (short_title != title || primary_artist != artist).then_some((short_title, primary_artist));
    for (search_title, search_artist) in [Some((title, artist)), fallback].into_iter().flatten() {
        check_stop(stop)?;
        let params = form_urlencoded::Serializer::new(String::new())
            .append_pair("track_name", search_title)
            .append_pair("artist_name", search_artist)
            .finish();
        let search =
            request(&format!("https://lrclib.net/api/search?{params}")).and_then(|value| {
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
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    plain
        .ok_or_else(|| failure.unwrap_or_else(|| "No lyrics found for a matching recording".into()))
}
fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}
fn placeholder_album(album: &str) -> bool {
    matches!(
        album.trim().to_ascii_lowercase().as_str(),
        "" | "unknown album" | "single" | "ep"
    )
}
fn provider_artist(artist: &str) -> &str {
    let Some((name, roles)) = artist.trim().rsplit_once(" (") else {
        return artist;
    };
    let Some(roles) = roles.strip_suffix(')') else {
        return artist;
    };
    if roles.split(',').all(|role| {
        matches!(
            role.trim().to_ascii_lowercase().as_str(),
            "composer" | "lyricist" | "performer" | "vocalist"
        )
    }) {
        name
    } else {
        artist
    }
}
fn title_without_feature(title: &str) -> Option<&str> {
    let lower = title.to_ascii_lowercase();
    if !lower.ends_with(')') {
        return None;
    }
    [" (feat.", " (feat ", " (ft.", " (ft ", " (featuring "]
        .iter()
        .filter_map(|marker| lower.rfind(marker))
        .map(|at| title[..at].trim_end())
        .find(|base| !base.is_empty())
}
fn matches_identity(
    result: &LrclibResponse,
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u64,
) -> bool {
    matches_identity_fields(
        RecordingIdentity {
            title: &result.track_name,
            artist: &result.artist_name,
            album: &result.album_name,
            duration: result.duration,
        },
        RequestedIdentity {
            title,
            artist,
            album,
            duration_ms,
        },
    )
}
fn matches_identity_fields(result: RecordingIdentity<'_>, request: RequestedIdentity<'_>) -> bool {
    use crate::media_identity::{album_matches, names_match};
    let result_artist = provider_artist(result.artist);
    let same_album = !request.album.is_empty() && album_matches(request.album, result.album);
    let soundtrack_title = |value: &str| {
        let lower = value.to_ascii_lowercase();
        if !lower.ends_with(')') {
            return None;
        }
        [" (uta from ", " (from "]
            .iter()
            .find_map(|suffix| lower.rfind(suffix).map(|at| value[..at].trim().to_owned()))
    };
    (names_match(request.title, result.title)
        || same_album
            && (soundtrack_title(result.title)
                .is_some_and(|base| names_match(request.title, &base))
                || soundtrack_title(request.title)
                    .is_some_and(|base| names_match(&base, result.title)))
        || title_without_feature(request.title).is_some_and(|base| names_match(base, result.title))
        || title_without_feature(result.title).is_some_and(|base| names_match(request.title, base)))
        && (names_match(request.artist, result_artist)
            || request
                .artist
                .split(';')
                .any(|a| names_match(a, result_artist)))
        && (request.album.is_empty()
            || album_matches(request.album, result.album)
            || title_without_feature(request.album)
                .is_some_and(|base| album_matches(base, result.album)))
        && (request.duration_ms == 0
            || result.duration.is_some_and(|d| {
                d.is_finite()
                    && d > 0.0
                    && (d * 1000.0 - request.duration_ms as f64).abs() <= 3000.0
            }))
}
fn check_stop(stop: Option<&std::sync::atomic::AtomicBool>) -> Result<(), String> {
    if stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err("Lyrics search cancelled".into());
    }
    Ok(())
}
fn read_response<T: serde::de::DeserializeOwned>(response: ureq::Response) -> Result<T, String> {
    let bytes = read_bounded(response)?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn read_text_response(response: ureq::Response) -> Result<String, String> {
    String::from_utf8(read_bounded(response)?).map_err(|e| e.to_string())
}
fn read_bounded(response: ureq::Response) -> Result<Vec<u8>, String> {
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
    Ok(bytes)
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
    #[ignore = "contacts public providers; no user files or audio devices"]
    fn live_lrc_lookup_budget_and_short_soundtrack_title() {
        for (title, artist, album, duration) in [
            ("Backlight", "Ado", "UTA'S SONGS ONE PIECE FILM RED", 237667),
            ("Lovely Day", "Bill Withers", "", 257000),
        ] {
            let start = std::time::Instant::now();
            let lyrics =
                fetch_lyrics_with_album_cancellable(title, artist, album, duration, None).unwrap();
            eprintln!("{title}: {:?}", start.elapsed());
            assert!(has_timed_lines(&lyrics));
            assert!(!lyrics.contains("<tt "));
            assert!(start.elapsed() < Duration::from_secs(9));
        }
    }
    #[test]
    fn providers_start_together_and_validated_red_result_keeps_priority() {
        let (red_ready, red_rx) = std::sync::mpsc::channel();
        let (lib_ready, lib_rx) = std::sync::mpsc::channel();
        let (red, lib) = provider_pair(
            move || {
                red_ready.send(()).unwrap();
                lib_rx.recv_timeout(Duration::from_secs(1)).unwrap();
                Ok(Some("Red lyrics".into()))
            },
            move || {
                lib_ready.send(()).unwrap();
                red_rx.recv_timeout(Duration::from_secs(1)).unwrap();
                Ok("LRCLIB lyrics".into())
            },
        );
        assert_eq!(choose_result(red, lib).unwrap(), "Red lyrics");
        assert_eq!(
            choose_result(Err("offline".into()), Ok("Fallback".into())).unwrap(),
            "Fallback"
        );
        let stop = AtomicBool::new(true);
        assert!(
            fetch_lyrics_cancellable("Song", "Artist", 1000, Some(&stop))
                .unwrap_err()
                .contains("cancelled")
        );
    }
    #[test]
    fn soundtrack_descriptors_match_only_with_same_album_artist_and_duration() {
        let mut value = LrclibResponse {
            track_name: "Backlight (UTA from ONE PIECE FILM RED)".into(),
            artist_name: "Ado".into(),
            album_name: "UTA'S SONGS ONE PIECE FILM RED".into(),
            duration: Some(237.667),
            synced_lyrics: None,
            plain_lyrics: None,
        };
        assert!(matches_identity(
            &value,
            "Backlight",
            "Ado",
            "UTA'S SONGS ONE PIECE FILM RED",
            237667
        ));
        assert!(!matches_identity(
            &value,
            "Backlight",
            "Ado",
            "Other album",
            237667
        ));
        assert!(!matches_identity(
            &value,
            "Backlight",
            "Ado",
            "UTA'S SONGS ONE PIECE FILM RED",
            250000
        ));
        value.track_name = "Backlight (Live)".into();
        assert!(!matches_identity(
            &value,
            "Backlight",
            "Ado",
            "UTA'S SONGS ONE PIECE FILM RED",
            237667
        ));
        assert!(has_timed_lines("[00:01]Lyrics"));
    }
    #[test]
    fn placeholder_album_and_provider_roles_keep_recording_checks_strict() {
        assert!(placeholder_album("Single"));
        assert!(placeholder_album("Unknown Album"));
        assert!(!placeholder_album("The Singles"));
        let value = LrclibResponse {
            track_name: "Song".into(),
            artist_name: "Artist (Composer, Lyricist)".into(),
            album_name: "Album".into(),
            duration: Some(120.0),
            synced_lyrics: None,
            plain_lyrics: None,
        };
        assert!(matches_identity(&value, "Song", "Artist", "Album", 120000));
        assert!(!matches_identity(
            &value,
            "Song (Live)",
            "Artist",
            "Album",
            120000
        ));
        assert!(!matches_identity(
            &value,
            "Song",
            "Other artist",
            "Album",
            120000
        ));
        assert!(!matches_identity(
            &value,
            "Song",
            "Artist",
            "Other album",
            120000
        ));
        assert_eq!(
            provider_artist("Artist (feat. Guest)"),
            "Artist (feat. Guest)"
        );
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
    fn featured_title_and_multiple_artists_retry_without_losing_identity_checks() {
        let mut calls = Vec::new();
        let result = json!({"trackName":"visa","artistName":"again&again","albumName":"visa - Single","duration":166.0,"syncedLyrics":"[00:01.00]Matched"});
        let replies = vec![
            Err("Lyrics not found".into()),
            Ok(json!([])),
            Ok(json!([result])),
        ];
        let mut replies = replies.into_iter();
        let lyrics = lookup(
            "visa (feat. Abhi The Nomad)",
            "again&again; Abhi The Nomad",
            "visa (feat. Abhi The Nomad)",
            166_000,
            None,
            |url| {
                calls.push(url.to_owned());
                replies.next().unwrap()
            },
        )
        .unwrap();
        assert_eq!(lyrics, "[00:01.00]Matched");
        assert!(calls[2].contains("track_name=visa&artist_name=again%26again"));
        let wrong = LrclibResponse {
            synced_lyrics: Some("wrong".into()),
            plain_lyrics: None,
            track_name: "visa".into(),
            artist_name: "someone else".into(),
            album_name: "visa - Single".into(),
            duration: Some(166.0),
        };
        assert!(!matches_identity(
            &wrong,
            "visa (feat. Abhi The Nomad)",
            "again&again; Abhi The Nomad",
            "visa (feat. Abhi The Nomad)",
            166_000,
        ));
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
    fn lrc_red_word_timing_wins_when_recording_matches() {
        let mut urls = Vec::new();
        let lyrics = lookup_with_providers(
            "Backslide",
            "twenty one pilots",
            "Clancy",
            180_000,
            None,
            |url| {
                urls.push(url.to_owned());
                Ok(json!({"hits":[
                    {"isrc":"USAT22402064","title":"Backslide","artist":"twenty one pilots","album":"Other album","duration":180},
                    {"isrc":"USAT22401698","title":"Backslide","artist":"twenty one pilots","album":"Clancy","duration":180}
                ]}))
            },
            |url| {
                assert_eq!(url, "https://lrc.red/s/USAT22401698.lrc");
                Ok("[00:00.05]<00:00.05>Word <00:00.43>timing".into())
            },
        )
        .unwrap();
        assert!(lyrics.contains("<00:00.43>"));
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("album=Clancy&duration=180"));
    }
    #[test]
    fn lrc_red_rejects_wrong_recordings_and_uses_lrclib() {
        let mut urls = Vec::new();
        let lyrics = lookup_with_providers(
            "Song",
            "Artist",
            "Album",
            120_000,
            None,
            |url| {
                urls.push(url.to_owned());
                if url.contains("lrc.red") {
                    Ok(json!({"hits":[
                        {"isrc":"USAT22401698","title":"Song","artist":"Other","album":"Album","duration":120},
                        {"isrc":"USAT22401699","title":"Song","artist":"Artist","album":"Album","duration":160},
                        {"isrc":"../../unsafe","title":"Song","artist":"Artist","album":"Album","duration":120}
                    ]}))
                } else {
                    Ok(result("Song", 120.0, Some("[00:01.00]LRCLIB")))
                }
            },
            |_| panic!("no LRC.RED file should be requested"),
        )
        .unwrap();
        assert_eq!(lyrics, "[00:01.00]LRCLIB");
        assert_eq!(urls.len(), 2);
    }
    #[test]
    fn lrc_red_skips_unavailable_candidate_before_falling_back() {
        let mut downloads = 0;
        let lyrics = lookup_with_providers(
            "Song",
            "Artist",
            "Album",
            120_000,
            None,
            |url| {
                assert!(url.contains("lrc.red"), "LRCLIB should not be needed");
                Ok(json!({"hits":[
                    {"isrc":"USAT22401698","title":"Song","artist":"Artist","album":"Album","duration":120},
                    {"isrc":"USAT22401699","title":"Song","artist":"Artist","album":"Album","duration":121}
                ]}))
            },
            |url| {
                downloads += 1;
                if url.contains("USAT22401698") {
                    Err("Lyrics not found".into())
                } else {
                    Ok("[00:01.00]Matched".into())
                }
            },
        )
        .unwrap();
        assert_eq!(lyrics, "[00:01.00]Matched");
        assert_eq!(downloads, 2);
    }
    #[test]
    fn lrc_red_failure_falls_back_and_cancellation_stops_before_lrclib() {
        let lyrics = lookup_with_providers(
            "Song",
            "Artist",
            "Album",
            120_000,
            None,
            |url| {
                if url.contains("lrc.red") {
                    Err("unavailable".into())
                } else {
                    Ok(result("Song", 120.0, Some("[00:01.00]LRCLIB")))
                }
            },
            |_| panic!("no LRC.RED file should be requested"),
        )
        .unwrap();
        assert_eq!(lyrics, "[00:01.00]LRCLIB");
        let stop = AtomicBool::new(false);
        let error = lookup_with_providers(
            "Song",
            "Artist",
            "Album",
            120_000,
            Some(&stop),
            |_| {
                stop.store(true, Ordering::Relaxed);
                Ok(json!({"hits":[]}))
            },
            |_| panic!("cancelled lookup must not download lyrics"),
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
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
