/// Last.fm API integration for scrobbling and now-playing updates.
///
/// Uses the Last.fm "Scrobbling API" (v2.0) with HMAC-free MD5 signature scheme.
/// Reference: https://www.last.fm/api/scrobbling
use std::collections::BTreeMap;
use url::form_urlencoded;

const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";

/// Build the MD5 API signature required by Last.fm write methods.
/// Params must be sorted alphabetically, concatenated as key+value pairs,
/// then the secret appended, and MD5 hashed.
fn api_sig(params: &BTreeMap<&str, String>, secret: &str) -> String {
    let mut sig_str = String::new();
    for (k, v) in params {
        sig_str.push_str(k);
        sig_str.push_str(v);
    }
    sig_str.push_str(secret);
    let digest = md5::compute(sig_str.as_bytes());
    format!("{:x}", digest)
}

/// Step 1 of the auth flow — get a request token.
pub fn get_token(api_key: &str) -> Result<String, String> {
    let url = format!(
        "{}?method=auth.gettoken&api_key={}&format=json",
        LASTFM_API_URL, api_key
    );
    let resp = ureq::get(&url)
        .set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)")
        .call()
        .map_err(|e| e.to_string())?;

    #[derive(serde::Deserialize)]
    struct TokenResponse { token: String }
    let data: TokenResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(data.token)
}

/// Build the URL that the user visits in a browser to authorize the token.
pub fn auth_url(api_key: &str, token: &str) -> String {
    format!(
        "http://www.last.fm/api/auth/?api_key={}&token={}",
        api_key, token
    )
}

/// Step 2 — after user approves in browser, exchange the token for a session key.
pub fn get_session(api_key: &str, secret: &str, token: &str) -> Result<String, String> {
    let mut params: BTreeMap<&str, String> = BTreeMap::new();
    params.insert("api_key", api_key.to_string());
    params.insert("method", "auth.getSession".to_string());
    params.insert("token", token.to_string());

    let sig = api_sig(&params, secret);

    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("method", "auth.getSession")
        .append_pair("api_key", api_key)
        .append_pair("token", token)
        .append_pair("api_sig", &sig)
        .append_pair("format", "json")
        .finish();

    let resp = ureq::post(LASTFM_API_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)")
        .send_string(&body)
        .map_err(|e| e.to_string())?;

    #[derive(serde::Deserialize)]
    struct Session { key: String }
    #[derive(serde::Deserialize)]
    struct SessionResponse { session: Session }

    let data: SessionResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(data.session.key)
}

/// Notify Last.fm of the currently playing track ("now playing" update).
/// This does NOT count as a scrobble.
pub fn update_now_playing(
    api_key: &str,
    secret: &str,
    session_key: &str,
    artist: &str,
    track: &str,
    album: &str,
    duration_secs: u64,
) -> Result<(), String> {
    let mut params: BTreeMap<&str, String> = BTreeMap::new();
    params.insert("api_key", api_key.to_string());
    params.insert("artist", artist.to_string());
    params.insert("track", track.to_string());
    params.insert("album", album.to_string());
    params.insert("duration", duration_secs.to_string());
    params.insert("method", "track.updateNowPlaying".to_string());
    params.insert("sk", session_key.to_string());

    let sig = api_sig(&params, secret);

    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("method", "track.updateNowPlaying")
        .append_pair("api_key", api_key)
        .append_pair("sk", session_key)
        .append_pair("artist", artist)
        .append_pair("track", track)
        .append_pair("album", album)
        .append_pair("duration", &duration_secs.to_string())
        .append_pair("api_sig", &sig)
        .append_pair("format", "json")
        .finish();

    ureq::post(LASTFM_API_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)")
        .send_string(&body)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Scrobble a track to Last.fm.
/// `timestamp` is Unix epoch seconds (when playback started).
/// Last.fm rules: scrobble after the track has been played for 50% of its duration
/// or for at least 4 minutes, whichever comes first.
pub fn scrobble(
    api_key: &str,
    secret: &str,
    session_key: &str,
    artist: &str,
    track: &str,
    album: &str,
    timestamp: u64,
) -> Result<(), String> {
    let mut params: BTreeMap<&str, String> = BTreeMap::new();
    params.insert("api_key", api_key.to_string());
    params.insert("artist[0]", artist.to_string());
    params.insert("track[0]", track.to_string());
    params.insert("album[0]", album.to_string());
    params.insert("timestamp[0]", timestamp.to_string());
    params.insert("method", "track.scrobble".to_string());
    params.insert("sk", session_key.to_string());

    let sig = api_sig(&params, secret);

    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("method", "track.scrobble")
        .append_pair("api_key", api_key)
        .append_pair("sk", session_key)
        .append_pair("artist[0]", artist)
        .append_pair("track[0]", track)
        .append_pair("album[0]", album)
        .append_pair("timestamp[0]", &timestamp.to_string())
        .append_pair("api_sig", &sig)
        .append_pair("format", "json")
        .finish();

    ureq::post(LASTFM_API_URL)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)")
        .send_string(&body)
        .map_err(|e| e.to_string())?;

    Ok(())
}
