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
            || release_cover_url(&url)
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

// Apple sometimes puts release artwork in an otherwise correctly identified
// artist's structured `image` field. Music* assets include Taaruk Raina's
// soundtrack cover, Bharat Chandak's I-POPSTAR image, and Alys's single cover.
fn release_cover_url(url: &url::Url) -> bool {
    url.path_segments().is_some_and(|segments| {
        segments.filter(|part| !part.is_empty()).any(|part| {
            let lower = part.to_ascii_lowercase();
            lower.starts_with("music")
                && lower[5..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit())
                || lower.ends_with("_cover.jpg")
                || lower == "artwork.jpg"
        })
    })
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
    fn rejects_release_cover_on_a_verified_artist_page() {
        let artist = json!({
            "@type": "MusicGroup",
            "name": "Taaruk Raina",
            "url": "https://music.apple.com/us/artist/taaruk-raina/1368308173",
            "image": "https://is1-ssl.mzstatic.com/image/thumb/Music221/v4/2f/2c/80/2f2c8010-7b0e-2439-27f5-65c02c89875d/8903431095619_cover.jpg/486x486bb.png"
        });
        assert!(images(&page(artist), "Taaruk Raina", 1368308173).is_empty());
    }
    #[test]
    fn rejects_promotional_cover_on_an_artist_page() {
        let artist = json!({
            "@type":"MusicGroup", "name":"Bharat Chandak",
            "url":"https://music.apple.com/us/artist/bharat-chandak/1828231642",
            "image":"https://is1-ssl.mzstatic.com/image/thumb/Music221/v4/a3/bf/9c/a3bf9ca2-e841-6657-f1e4-6f5a2e396843/885288782271.jpg/486x486bb.png"
        });
        assert!(images(&page(artist), "Bharat Chandak", 1828231642).is_empty());
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
