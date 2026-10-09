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
