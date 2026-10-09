//! Artist-page structured data, fetched only after recording-based ID resolution.
use super::{matching::names_match, transport::Transport, LookupError};
use serde_json::Value;

pub(super) fn portrait(
    artist: &str,
    id: u64,
    client: &impl Transport,
) -> Result<Vec<String>, LookupError> {
    if client.cancelled() {
        return Err(LookupError::Cancelled);
    }
    let page = client.text(&format!("https://music.apple.com/us/artist/-/{id}"))?;
    if client.cancelled() {
        return Err(LookupError::Cancelled);
    }
    Ok(images(&page, artist, id))
}

fn images(page: &str, artist: &str, id: u64) -> Vec<String> {
    let mut images = Vec::new();
    // Only structured artist entities can supply portraits. Album, playlist and
    // Open Graph images on the same page must not become an artist picture.
    for script in page.split("<script").skip(1) {
        let Some((attributes, body)) = script.split_once('>') else {
            continue;
        };
        if !attributes.contains("application/ld+json") {
            continue;
        }
        let Some((json, _)) = body.split_once("</script>") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            continue;
        };
        collect(&value, artist, id, &mut images, 0);
        if images.len() >= 3 {
            break;
        }
    }
    images.truncate(3);
    images
}

fn collect(value: &Value, artist: &str, id: u64, images: &mut Vec<String>, depth: usize) {
    if depth > 8 || images.len() >= 3 {
        return;
    }
    if let Some(array) = value.as_array() {
        for item in array {
            collect(item, artist, id, images, depth + 1);
        }
        return;
    }
    if let Some(graph) = value.get("@graph") {
        collect(graph, artist, id, images, depth + 1);
    }
    let is_artist = value["@type"]
        .as_str()
        .is_some_and(|kind| matches!(kind, "MusicGroup" | "Person"));
    if !is_artist
        || !value["name"]
            .as_str()
            .is_some_and(|name| names_match(artist, name))
        || !value["url"]
            .as_str()
            .is_some_and(|url| artist_url_matches(url, id))
    {
        return;
    }
    let candidates: Vec<&Value> = match &value["image"] {
        Value::Array(array) => array.iter().collect(),
        value => vec![value],
    };
    for candidate in candidates {
        let Some(link) = candidate
            .as_str()
            .or_else(|| candidate["url"].as_str())
            .or_else(|| candidate["contentUrl"].as_str())
        else {
            continue;
        };
        let Ok(url) = url::Url::parse(link) else {
            continue;
        };
        if url.scheme() != "https"
            || !url
                .host_str()
                .is_some_and(|host| host.ends_with(".mzstatic.com"))
        {
            continue;
        }
        if !images.iter().any(|image| image == link) {
            images.push(link.to_owned());
        }
        if images.len() == 3 {
            break;
        }
    }
}

fn artist_url_matches(link: &str, id: u64) -> bool {
    linked_artist_id(link) == Some(id)
}

pub(super) fn linked_artist_id(link: &str) -> Option<u64> {
    let Ok(url) = url::Url::parse(link) else {
        return None;
    };
    if url.scheme() != "https" || url.host_str() != Some("music.apple.com") {
        return None;
    }
    let parts = url.path_segments()?;
    let parts: Vec<_> = parts.filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [_, "artist", found] | [_, "artist", _, found] => {
            found.parse::<u64>().ok().filter(|id| *id > 0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(value: Value) -> String {
        format!("<script type=\"application/ld+json\">{value}</script>")
    }
    fn artist() -> Value {
        json!({"@type":"MusicGroup","name":"Cafuné","url":"https://music.apple.com/us/artist/cafune/947078210","image":"https://is1-ssl.mzstatic.com/image/portrait.png"})
    }

    #[test]
    fn verified_artist_structured_data_supplies_portraits() {
        assert_eq!(
            images(&page(artist()), "CAFUNÉ", 947078210),
            ["https://is1-ssl.mzstatic.com/image/portrait.png"]
        );
        assert_eq!(
            images(&page(json!({"@graph":[artist()]})), "Cafuné", 947078210).len(),
            1
        );
    }
    #[test]
    fn rejects_album_art_wrong_identity_and_untrusted_image_hosts() {
        for (field, value) in [
            ("@type", "MusicAlbum"),
            ("name", "Other artist"),
            ("url", "https://music.apple.com/us/artist/cafune/2"),
            ("image", "https://mzstatic.com.evil.test/portrait.png"),
        ] {
            let mut data = artist();
            data[field] = json!(value);
            assert!(images(&page(data), "Cafuné", 947078210).is_empty());
        }
        assert!(images(
            "<meta property=\"og:image\" content=\"https://is1-ssl.mzstatic.com/album.jpg\">",
            "Cafuné",
            947078210
        )
        .is_empty());
    }
    #[test]
    fn malformed_scripts_do_not_hide_later_valid_artist_data() {
        let html = format!(
            "<script type='application/ld+json'>invalid</script>{}",
            page(artist())
        );
        assert_eq!(images(&html, "Cafuné", 947078210).len(), 1);
    }
}
