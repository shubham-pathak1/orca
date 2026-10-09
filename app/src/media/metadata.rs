use crate::MetadataDraft;
use orca_services::operation_types::CoverFetch;
#[cfg(test)]
use serde_json::{json, Value};

pub(crate) fn artwork_album_artist(draft: &MetadataDraft) -> String {
    if draft.album_artist.trim().is_empty() {
        draft
            .artist
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .into()
    } else {
        draft.album_artist.trim().into()
    }
}
pub(crate) fn artwork_target_matches(
    fetch: &CoverFetch,
    draft: &MetadataDraft,
    generation: i32,
) -> bool {
    fetch.editor_path.as_deref() == Some(draft.path.as_str())
        && fetch
            .editor_generation
            .is_none_or(|expected| expected == generation)
}
pub(crate) fn artwork_tags_match(fetch: &CoverFetch, draft: &MetadataDraft) -> bool {
    fetch
        .title
        .as_deref()
        .is_none_or(|title| title.trim() == draft.title.trim())
        && fetch
            .track_artist
            .as_deref()
            .is_none_or(|artist| artist.trim() == draft.artist.trim())
        && fetch.album.trim() == draft.album.trim()
        && fetch.artist.trim() == artwork_album_artist(draft)
}

#[cfg(test)]
pub fn parse(text: &str) -> Result<MetadataDraft, String> {
    let document = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok(from_document(document))
}
pub fn from_document(document: orca_services::types::MetadataDocument) -> MetadataDraft {
    let value = document.fields;
    let number = |value: Option<i32>| value.map(|v| v.to_string()).unwrap_or_default().into();
    MetadataDraft {
        path: value.path.into(),
        title: value.title.into(),
        artist: value.artist.into(),
        album: value.album.into(),
        album_artist: value.album_artist.into(),
        year: number(value.year),
        track_number: number(value.track_number),
        disc_number: number(value.disc_number),
        genre: value.genre.unwrap_or_default().into(),
        lyrics: value.lyrics.unwrap_or_default().into(),
        artwork: document.artwork.unwrap_or_default().into(),
        cover_to_embed: Default::default(),
        remove_cover: false,
        file_version: document.file_version.into(),
    }
}
#[cfg(test)]
pub fn save(draft: &MetadataDraft) -> Result<String, String> {
    serde_json::to_string(&edit(draft)?).map_err(|e| e.to_string())
}
pub fn edit(draft: &MetadataDraft) -> Result<orca_services::types::MetadataEdit, String> {
    if [
        draft.path.as_str(),
        draft.title.as_str(),
        draft.artist.as_str(),
        draft.album.as_str(),
        draft.album_artist.as_str(),
    ]
    .iter()
    .any(|v| v.trim().is_empty())
    {
        return Err("Enter a title, artist, album, and album artist".into());
    }
    let number = |text: &str| -> Result<Option<i32>, String> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            text.trim()
                .parse::<i32>()
                .ok()
                .filter(|v| *v > 0)
                .map(Some)
                .ok_or_else(|| "Year, track, and disc must be positive whole numbers".into())
        }
    };
    fn optional(text: &str) -> Option<&str> {
        if text.trim().is_empty() {
            None
        } else {
            Some(text.trim())
        }
    }
    use orca_services::types::{MetadataEdit, MetadataFields};
    Ok(MetadataEdit {
        fields: MetadataFields {
            path: draft.path.to_string(),
            title: draft.title.trim().into(),
            artist: draft.artist.trim().into(),
            album: draft.album.trim().into(),
            album_artist: draft.album_artist.trim().into(),
            year: number(&draft.year)?,
            track_number: number(&draft.track_number)?,
            disc_number: number(&draft.disc_number)?,
            genre: optional(&draft.genre).map(str::to_owned),
            lyrics: optional(&draft.lyrics).map(str::to_owned),
        },
        cover_to_embed: optional(&draft.cover_to_embed).map(str::to_owned),
        remove_cover: draft.remove_cover,
        file_version: optional(&draft.file_version).map(str::to_owned),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_fetches_cannot_update_reopened_editors_or_changed_search_tags() {
        let mut draft = parse(r#"{"path":"song.flac","title":"Title","artist":"Artist","album":"Album","album_artist":"Artist"}"#).unwrap();
        let fetch: CoverFetch = serde_json::from_value(json!({"kind":"albums","key":"Artist:Album","artist":"Artist","album":"Album","title":"Title","track_artist":"Artist","editor_path":"song.flac","editor_generation":1})).unwrap();
        assert!(artwork_target_matches(&fetch, &draft, 1));
        assert!(!artwork_target_matches(&fetch, &draft, 2));
        assert!(artwork_tags_match(&fetch, &draft));
        draft.title = "Changed title".into();
        assert!(!artwork_tags_match(&fetch, &draft));
    }
    #[test]
    fn staged_cover_and_optional_fields_survive_save() {
        let mut draft=parse(r#"{"path":"song.flac","title":"Title","artist":"Artist","album":"Album","album_artist":"Artist"}"#).unwrap();
        draft.cover_to_embed = "cover.jpg".into();
        draft.file_version = "100:200:300".into();
        let value: Value = serde_json::from_str(&save(&draft).unwrap()).unwrap();
        assert_eq!(value["cover_to_embed"], "cover.jpg");
        assert_eq!(value["file_version"], "100:200:300");
        assert!(value["year"].is_null());
    }
    #[test]
    fn invalid_numeric_fields_prevent_writing() {
        let mut draft=parse(r#"{"path":"song.flac","title":"Title","artist":"Artist","album":"Album","album_artist":"Artist"}"#).unwrap();
        draft.year = "NaN".into();
        assert!(save(&draft).is_err());
    }
}
