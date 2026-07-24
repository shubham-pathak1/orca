use std::fs;
use std::io::Read;
use std::path::Path;
use serde::Deserialize;
use url::form_urlencoded;
use crate::library::{write_webp_derivative, ArtworkPaths};

#[derive(Deserialize)]
struct ItunesResponse {
    results: Vec<ItunesResult>,
}

#[derive(Deserialize)]
struct ItunesResult {
    #[serde(rename = "artworkUrl100")]
    artwork_url_100: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(rename = "wrapperType")]
    wrapper_type: Option<String>,
}

#[derive(Deserialize)]
struct ItunesArtistSearchResponse {
    results: Vec<ItunesArtistResult>,
}

#[derive(Deserialize)]
struct ItunesArtistResult {
    #[serde(rename = "artistId")]
    artist_id: Option<i64>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "wrapperType")]
    wrapper_type: Option<String>,
}

pub fn fetch_itunes_artist_image(artist: &str) -> Option<String> {
    // Step 1: Search for the artist to get their ID
    let mut url = String::from("https://itunes.apple.com/search?");
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("term", artist)
            .append_pair("entity", "musicArtist")
            .append_pair("limit", "5")
            .finish(),
    );

    let req = ureq::get(&url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp = req.call().ok()?;
    if resp.status() != 200 { return None; }
    
    let search_result: ItunesArtistSearchResponse = resp.into_json().ok()?;
    
    // Find the best matching artist by name
    let artist_id = search_result.results.into_iter()
        .filter(|r| r.wrapper_type.as_deref() == Some("artist"))
        .find(|r| {
            r.artist_name.as_deref().map(|n| names_match(artist, n)).unwrap_or(false)
        })
        .and_then(|r| r.artist_id)?;
    
    // Step 2: Lookup albums for this artist
    let mut lookup_url = String::from("https://itunes.apple.com/lookup?");
    lookup_url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("id", &artist_id.to_string())
            .append_pair("entity", "album")
            .append_pair("limit", "3")
            .finish(),
    );
    
    let req2 = ureq::get(&lookup_url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp2 = req2.call().ok()?;
    if resp2.status() != 200 { return None; }
    
    let lookup_result: ItunesResponse = resp2.into_json().ok()?;
    
    // Pick the first album result (skip the artist entry at index 0)
    for item in lookup_result.results {
        if item.wrapper_type.as_deref() != Some("collection") { continue; }
        if let Some(url_100) = item.artwork_url_100 {
            return Some(url_100.replace("100x100bb", "600x600bb"));
        }
    }
    
    None
}

pub fn fetch_itunes_album_art(artist: &str, album: &str) -> Option<String> {
    let term = format!("{} {}", artist, album);
    let mut url = String::from("https://itunes.apple.com/search?");
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("term", &term)
            .append_pair("entity", "album")
            .append_pair("limit", "1")
            .finish(),
    );

    let req = ureq::get(&url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp = req.call().ok()?;
    
    if resp.status() != 200 {
        return None;
    }

    let result: ItunesResponse = resp.into_json().ok()?;
    
    for item in result.results {
        // Validate result
        if let Some(ref found_artist) = item.artist_name {
            if !names_match(artist, found_artist) {
                continue;
            }
        }
        if let Some(ref found_album) = item.collection_name {
            if !names_match(album, found_album) {
                continue;
            }
        }
        
        if let Some(url_100) = item.artwork_url_100 {
            // iTunes returns a 100x100 URL, but we can replace '100x100bb' with '600x600bb' to get higher quality
            let hi_res = url_100.replace("100x100bb", "600x600bb");
            return Some(hi_res);
        }
    }
    
    // Fallback to Deezer if iTunes failed or was rejected
    fetch_deezer_album_art(artist, album)
}

#[derive(Deserialize)]
struct DeezerSearchResponse {
    data: Vec<DeezerArtist>,
}

#[derive(Deserialize)]
struct DeezerArtist {
    name: Option<String>,
    nb_fan: Option<i64>,
    picture_xl: Option<String>,
    picture_big: Option<String>,
}

#[derive(Deserialize)]
struct DeezerAlbumSearchResponse {
    data: Vec<DeezerAlbum>,
}

#[derive(Deserialize)]
struct DeezerAlbum {
    title: Option<String>,
    cover_xl: Option<String>,
    artist: Option<DeezerArtistSmall>,
}

#[derive(Deserialize)]
struct DeezerArtistSmall {
    name: Option<String>,
}

/// Simple similarity: normalize both strings and check if they share enough tokens
fn names_match(query: &str, result: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let q = normalize(query);
    let r = normalize(result);
    if q.is_empty() || r.is_empty() { return false; }
    
    let q_str = q.join(" ");
    let r_str = r.join(" ");
    
    // Exact match after normalization
    if q_str == r_str {
        return true;
    }
    
    // If one is a single word and the other is multiple words, reject it 
    // (e.g. "Zeph" != "Zeph & Azeem")
    if q.len() != r.len() {
        // Only accept if the lengths differ by at most 1, and the extra word is something like "the"
        // But to be safe, just reject it.
        return false;
    }
    
    // Accept if at least 75% of the tokens match exactly
    let matches = q.iter().filter(|t| r.contains(t)).count();
    matches as f32 / q.len().max(1) as f32 >= 0.75
}

pub fn fetch_deezer_artist_image(artist: &str) -> Option<String> {
    let mut url = String::from("https://api.deezer.com/search/artist?");
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("q", artist)
            .append_pair("limit", "5")
            .finish(),
    );

    let req = ureq::get(&url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp = req.call().ok()?;
    
    if resp.status() != 200 {
        return None;
    }

    let result: DeezerSearchResponse = resp.into_json().ok()?;

    for candidate in result.data {
        // Reject if the returned artist name doesn't closely match the query
        if let Some(found_name) = &candidate.name {
            if !names_match(artist, found_name) {
                continue;
            }
        }
        // Reject artists with very few fans — Deezer has garbage images for obscure entries
        let fans = candidate.nb_fan.unwrap_or(0);
        if fans < 50 {
            continue;
        }
        if let Some(xl) = candidate.picture_xl {
            if !xl.is_empty() && !xl.contains("/images/artist//") { return Some(xl); }
        }
        if let Some(big) = candidate.picture_big {
            if !big.is_empty() && !big.contains("/images/artist//") { return Some(big); }
        }
    }
    
    None
}

pub fn fetch_deezer_album_art(artist: &str, album: &str) -> Option<String> {
    let mut url = String::from("https://api.deezer.com/search/album?");
    let query = format!("artist:\"{}\" album:\"{}\"", artist, album);
    url.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .append_pair("q", &query)
            .append_pair("limit", "5")
            .finish(),
    );

    let req = ureq::get(&url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp = req.call().ok()?;
    
    if resp.status() != 200 {
        return None;
    }

    let result: DeezerAlbumSearchResponse = resp.into_json().ok()?;
    
    for item in result.data {
        if let Some(ref found_artist) = item.artist {
            if let Some(ref name) = found_artist.name {
                if !names_match(artist, name) {
                    continue;
                }
            }
        }
        if let Some(ref found_title) = item.title {
            if !names_match(album, found_title) {
                continue;
            }
        }
        
        if let Some(xl) = item.cover_xl {
            if !xl.is_empty() && !xl.contains("/images/cover//") { return Some(xl); }
        }
    }
    
    None
}

pub fn download_and_cache(url: &str, cache_dir: &Path, prefix: &str) -> Result<ArtworkPaths, String> {
    let req = ureq::get(url).set("User-Agent", "Orca/0.1.3 (https://github.com/shubham-pathak1/orca)");
    let resp = req.call().map_err(|e| e.to_string())?;
    
    if resp.status() != 200 {
        return Err(format!("Failed to download image, status: {}", resp.status()));
    }
    
    let mut bytes = Vec::new();
    resp.into_reader().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    
    if !cache_dir.exists() {
        fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    }
    
    let base_path = cache_dir.join(prefix);
    let original_path = base_path.with_extension("jpg"); // We don't know exact extension, jpg is fallback
    let thumb_path = base_path.with_extension("thumb.webp");
    let preview_path = base_path.with_extension("preview.webp");
    
    fs::write(&original_path, &bytes).map_err(|e| e.to_string())?;
    
    let mut thumb_written = false;
    let mut preview_written = false;
    if let Ok(img) = image::load_from_memory(&bytes) {
        if write_webp_derivative(&img, &thumb_path, 80).is_ok() {
            thumb_written = true;
        }
        if write_webp_derivative(&img, &preview_path, 256).is_ok() {
            preview_written = true;
        }
    }
    
    Ok(ArtworkPaths {
        full: original_path.to_string_lossy().to_string(),
        thumb: if thumb_written { thumb_path } else { original_path.clone() }.to_string_lossy().to_string(),
        preview: if preview_written { preview_path } else { original_path.clone() }.to_string_lossy().to_string(),
    })
}
