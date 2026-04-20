use serde::Deserialize;
use url::form_urlencoded;

#[derive(Debug, Deserialize)]
pub struct LrclibResponse {
    #[serde(rename = "syncedLyrics")]
    pub synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    pub plain_lyrics: Option<String>,
}

pub fn fetch_lyrics(title: &str, artist: &str, _duration_ms: u64) -> Result<String, String> {
    let mut url = String::from("https://lrclib.net/api/get?");
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("track_name", title)
            .append_pair("artist_name", artist)
            // Lrclib expects duration in seconds, but matching is strict. Sometimes duration is slightly off.
            // .append_pair("duration", &(duration_ms / 1000).to_string())
            .finish(),
    );

    let req = ureq::get(&url)
        .set("User-Agent", "Orca/0.1.0 (https://github.com/shubhampathak1/orca-desktop)");
        
    let resp = req.call().map_err(|e| e.to_string())?;

    if resp.status() == 404 {
        // Fallback to search if exact match fails
        return fetch_lyrics_search(title, artist);
    }
    
    if resp.status() != 200 {
        return Err(format!("Lrclib returned status: {}", resp.status()));
    }

    let result: LrclibResponse = resp.into_json().map_err(|e| e.to_string())?;

    if let Some(synced) = result.synced_lyrics {
        if !synced.is_empty() {
            return Ok(synced);
        }
    }
    
    if let Some(plain) = result.plain_lyrics {
        if !plain.is_empty() {
            return Ok(plain);
        }
    }

    Err("Lyrics not found in response".to_string())
}

fn fetch_lyrics_search(title: &str, artist: &str) -> Result<String, String> {
    let mut url = String::from("https://lrclib.net/api/search?");
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("track_name", title)
            .append_pair("artist_name", artist)
            .finish(),
    );

    let req = ureq::get(&url)
        .set("User-Agent", "Orca/0.1.0 (https://github.com/shubhampathak1/orca-desktop)");
        
    let resp = req.call().map_err(|e| e.to_string())?;
    
    if resp.status() != 200 {
        return Err(format!("Lrclib search returned status: {}", resp.status()));
    }

    let results: Vec<LrclibResponse> = resp.into_json().map_err(|e| e.to_string())?;

    if let Some(result) = results.into_iter().next() {
        if let Some(synced) = result.synced_lyrics {
            if !synced.is_empty() {
                return Ok(synced);
            }
        }
        
        if let Some(plain) = result.plain_lyrics {
            if !plain.is_empty() {
                return Ok(plain);
            }
        }
    }

    Err("No lyrics found in search results".to_string())
}
