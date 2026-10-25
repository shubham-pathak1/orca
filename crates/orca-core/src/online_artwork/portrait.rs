//! Portrait sources attached to an already verified MusicBrainz artist.
use super::{
    matching::names_match,
    providers::{array, encode, urls},
    transport::Transport,
    LookupError,
};
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn from_relations(
    detail: &Value,
    artist: &str,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    let relations = array(detail, "relations")?;
    let mut failure = None;
    match wikimedia(relations, client) {
        Ok(images) if !images.is_empty() => return Ok(images),
        Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
        Err(error) => super::record_failure(&mut failure, error),
        _ => {}
    }
    match spotify(relations, artist, client) {
        Ok(images) if !images.is_empty() => return Ok(images),
        Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
        Err(error) => super::record_failure(&mut failure, error),
        _ => {}
    }
    let mut seen = HashSet::new();
    for id in relations
        .iter()
        .filter_map(|r| r["url"]["resource"].as_str())
        .filter_map(super::apple::linked_artist_id)
        .filter(|id| seen.insert(*id))
        .take(1)
    {
        match super::apple::portrait(artist, id, client) {
            Ok(images) if !images.is_empty() => return Ok(images),
            Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
            Err(LookupError::NotFound) => {}
            Err(error) => super::record_failure(&mut failure, error),
            _ => {}
        }
    }
    seen.clear();
    for id in relations
        .iter()
        .filter_map(|r| r["url"]["resource"].as_str())
        .filter_map(deezer_id)
        .filter(|id| seen.insert(*id))
        .take(2)
    {
        if client.cancelled() {
            return Err(LookupError::Cancelled);
        }
        match client.json(&format!("https://api.deezer.com/artist/{id}")) {
            Ok(found) => {
                if found["id"].as_u64() != Some(id)
                    || !found["name"]
                        .as_str()
                        .is_some_and(|name| names_match(artist, name))
                {
                    continue;
                }
                let images = urls(
                    [found["picture_xl"].as_str(), found["picture_big"].as_str()]
                        .into_iter()
                        .flatten()
                        .filter(|url| !url.contains("/images/artist//"))
                        .map(str::to_owned),
                );
                if !images.is_empty() {
                    return Ok(images);
                }
            }
            Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
            Err(error) => super::record_failure(&mut failure, error),
        }
    }
    Err(failure.unwrap_or(LookupError::PortraitNotFound))
}

/// A Spotify page linked from the verified MusicBrainz identity can supply an
/// artist-specific image. Album images use a different image identifier.
pub(super) fn spotify(
    relations: &[Value],
    artist: &str,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    let Some(page_url) = relations
        .iter()
        .filter_map(|relation| relation["url"]["resource"].as_str())
        .find(|link| spotify_artist_url(link))
    else {
        return Ok(Vec::new());
    };
    if client.cancelled() {
        return Err(LookupError::Cancelled);
    }
    let page = client.text(page_url)?;
    if !meta_content(&page, "og:title").is_some_and(|name| names_match(artist, name)) {
        return Ok(Vec::new());
    }
    let Some(image) = meta_content(&page, "og:image") else {
        return Ok(Vec::new());
    };
    let Ok(url) = url::Url::parse(image) else {
        return Ok(Vec::new());
    };
    let Some(asset) = url.path().strip_prefix("/image/") else {
        return Ok(Vec::new());
    };
    if url.scheme() != "https"
        || url.host_str() != Some("i.scdn.co")
        || !asset.starts_with("ab676161")
        || asset.len() < 32
        || !asset.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(Vec::new());
    }
    Ok(vec![image.to_string()])
}

fn spotify_artist_url(link: &str) -> bool {
    let Ok(url) = url::Url::parse(link) else {
        return false;
    };
    if url.scheme() != "https" || url.host_str() != Some("open.spotify.com") {
        return false;
    }
    let Some(parts) = url.path_segments() else {
        return false;
    };
    let parts: Vec<_> = parts.collect();
    matches!(parts.as_slice(), ["artist", id] if id.len() == 22 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

fn meta_content<'a>(page: &'a str, property: &str) -> Option<&'a str> {
    for tag in page
        .split("<meta")
        .skip(1)
        .filter_map(|tag| tag.split_once('>').map(|(attributes, _)| attributes))
    {
        if !tag.contains(&format!("property=\"{property}\"")) {
            continue;
        }
        let (_, content) = tag.split_once("content=\"")?;
        return content.split_once('"').map(|(value, _)| value);
    }
    None
}

fn deezer_id(link: &str) -> Option<u64> {
    let url = url::Url::parse(link).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !matches!(url.host_str(), Some("deezer.com" | "www.deezer.com"))
    {
        return None;
    }
    let parts: Vec<_> = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect();
    let id = match parts.as_slice() {
        ["artist", id] => *id,
        [locale, "artist", id]
            if locale.len() == 2 && locale.bytes().all(|c| c.is_ascii_lowercase()) =>
        {
            *id
        }
        _ => return None,
    };
    id.parse::<u64>().ok().filter(|id| *id > 0)
}

fn wikimedia(relations: &[Value], client: &impl Transport) -> Result<Vec<String>, LookupError> {
    let Some(link) = relations
        .iter()
        .find(|r| r["type"].as_str() == Some("wikidata"))
        .and_then(|r| r["url"]["resource"].as_str())
    else {
        return Ok(vec![]);
    };
    let entity = link.rsplit('/').next().unwrap_or_default();
    if !entity.starts_with('Q')
        || entity.len() < 2
        || !entity[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Ok(vec![]);
    }
    let data = client.json(&format!(
        "https://www.wikidata.org/wiki/Special:EntityData/{entity}.json"
    ))?;
    let Some(image) =
        data["entities"][entity]["claims"]["P18"][0]["mainsnak"]["datavalue"]["value"].as_str()
    else {
        return Ok(vec![]);
    };
    let title = format!("File:{image}");
    let commons = client.json(&format!(
        "https://commons.wikimedia.org/w/api.php?{}",
        encode(&[
            ("action", "query"),
            ("format", "json"),
            ("titles", &title),
            ("prop", "imageinfo"),
            ("iiprop", "url")
        ])
    ))?;
    Ok(urls(
        commons["query"]["pages"]
            .as_object()
            .ok_or(LookupError::Unavailable)?
            .values()
            .filter_map(|p| p["imageinfo"][0]["url"].as_str().map(str::to_owned)),
    ))
}
