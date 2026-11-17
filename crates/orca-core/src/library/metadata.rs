use super::{AdvancedMetadata, SongMetadataUpdate};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{Picture, PictureType},
    prelude::*,
    probe::Probe,
    tag::{ItemValue, Tag, TagItem, TagType},
};
use std::{fs, path::Path};

// Lofty 0.21 maps advisory to ID3/MP4, but leaves this Vorbis field unknown.
pub(super) fn advisory_value(tag: &Tag) -> Option<&str> {
    tag.get_string(&ItemKey::ParentalAdvisory).or_else(|| {
        tag.items()
            .find_map(|item| match (item.key(), item.value()) {
                (ItemKey::Unknown(key), ItemValue::Text(value))
                    if key.eq_ignore_ascii_case("ITUNESADVISORY") =>
                {
                    Some(value.as_str())
                }
                _ => None,
            })
    })
}
fn clear_advisory(tag: &mut Tag) {
    let keys: Vec<_> = tag
        .items()
        .filter_map(|item| match item.key() {
            ItemKey::ParentalAdvisory => Some(item.key().clone()),
            ItemKey::Unknown(key) if key.eq_ignore_ascii_case("ITUNESADVISORY") => {
                Some(item.key().clone())
            }
            _ => None,
        })
        .collect();
    for key in keys {
        tag.remove_key(&key);
    }
}
fn set_advisory(tag: &mut Tag, value: Option<&str>) -> Result<(), String> {
    clear_advisory(tag);
    if let Some(value) = value {
        if matches!(tag.tag_type(), TagType::VorbisComments | TagType::Ape) {
            tag.insert_unchecked(TagItem::new(
                ItemKey::Unknown("ITUNESADVISORY".into()),
                ItemValue::Text(value.into()),
            ));
        } else {
            tag.insert_text(ItemKey::ParentalAdvisory, value.into());
        }
    }
    if advisory_value(tag) != value {
        return Err(
            "This file format cannot store content advisory. No changes were saved.".into(),
        );
    }
    Ok(())
}

/// Read editor-only tags afresh without storing them in the library index.
pub fn read_advanced_metadata(path: &Path) -> Result<AdvancedMetadata, String> {
    let file = Probe::open(path)
        .and_then(|probe| Ok(probe.guess_file_type()?))
        .and_then(|probe| probe.read())
        .map_err(|error| error.to_string())?;
    let tag = file.primary_tag().or_else(|| file.first_tag());
    Ok(AdvancedMetadata {
        composer: tag
            .and_then(|tag| tag.get_string(&ItemKey::Composer))
            .map(str::to_owned),
        comment: tag
            .and_then(|tag| tag.get_string(&ItemKey::Comment))
            .map(str::to_owned),
        explicit: super::scan::explicit_advisory(tag.and_then(advisory_value)),
    })
}

pub fn update_song_metadata(update: SongMetadataUpdate) -> Result<(), String> {
    update_song_metadata_with_cover(update, None)
}

/// Save edited fields and an optional front cover in the same tag write.
pub fn update_song_metadata_with_cover(
    update: SongMetadataUpdate,
    cover_path: Option<&Path>,
) -> Result<(), String> {
    update_song_metadata_cover_action(update, cover_path, false)
}

pub fn update_song_metadata_cover_action(
    update: SongMetadataUpdate,
    cover_path: Option<&Path>,
    remove_cover: bool,
) -> Result<(), String> {
    update_song_metadata_checked(update, cover_path, remove_cover, None)
}

pub fn update_song_metadata_checked(
    update: SongMetadataUpdate,
    cover_path: Option<&Path>,
    remove_cover: bool,
    expected_version: Option<&str>,
) -> Result<(), String> {
    let cover = cover_path.map(read_cover).transpose()?;
    crate::atomic_file::edit_checked(Path::new(&update.path), expected_version, |staged| {
        let mut tagged_file = Probe::open(staged)
            .and_then(|probe| Ok(probe.guess_file_type()?))
            .and_then(|probe| probe.read())
            .map_err(|e| e.to_string())?;
        if remove_cover || cover.is_some() {
            clear_all_pictures(&mut tagged_file);
        }
        if update
            .lyrics
            .as_deref()
            .is_none_or(|lyrics| lyrics.trim().is_empty())
        {
            let types: Vec<_> = tagged_file.tags().iter().map(Tag::tag_type).collect();
            for kind in types {
                if let Some(tag) = tagged_file.tag_mut(kind) {
                    tag.remove_key(&ItemKey::Lyrics);
                }
            }
        }
        if update.advanced.is_some() {
            let types: Vec<_> = tagged_file.tags().iter().map(Tag::tag_type).collect();
            for kind in types {
                if let Some(tag) = tagged_file.tag_mut(kind) {
                    clear_advisory(tag);
                    for key in [
                        ItemKey::Composer,
                        ItemKey::Comment,
                        ItemKey::ParentalAdvisory,
                    ] {
                        tag.remove_key(&key);
                    }
                }
            }
        }
        let tag = writable_tag_mut(&mut tagged_file)?;
        set_text(tag, ItemKey::TrackTitle, &update.title);
        set_text(tag, ItemKey::TrackArtist, &update.artist);
        set_text(tag, ItemKey::AlbumTitle, &update.album);
        set_text(tag, ItemKey::AlbumArtist, &update.album_artist);
        match update.year.filter(|year| *year > 0) {
            Some(year) => tag.set_year(year as u32),
            None => tag.remove_year(),
        }
        set_optional_number(tag, ItemKey::TrackNumber, update.track_number);
        set_optional_number(tag, ItemKey::DiscNumber, update.disc_number);
        set_optional_text(tag, ItemKey::Genre, update.genre.as_deref());
        set_optional_text(tag, ItemKey::Lyrics, update.lyrics.as_deref());
        if let Some(advanced) = &update.advanced {
            let advisory = advanced
                .explicit
                .map(|explicit| if explicit { "1" } else { "2" });
            for (key, value) in [
                (ItemKey::Composer, advanced.composer.as_deref()),
                (ItemKey::Comment, advanced.comment.as_deref()),
            ] {
                let value = value.map(str::trim).filter(|value| !value.is_empty());
                set_optional_text(tag, key.clone(), value);
                if tag.get_string(&key) != value {
                    return Err("This file format cannot store the requested advanced tag. No changes were saved.".into());
                }
            }
            set_advisory(tag, advisory)?;
        }
        if let Some(picture) = cover {
            tag.push_picture(picture);
        }
        save_and_verify(staged, &tagged_file)
    })
}

/// Change only a collection tag. Pictures, lyrics and unrelated tag items are
/// preserved; replacement and read-back verification use the song editor path.
pub fn update_collection_name_checked(
    path: &Path,
    kind: &str,
    old_name: &str,
    name: &str,
    expected_version: &str,
) -> Result<(), String> {
    let key = match kind {
        "albums" => ItemKey::AlbumTitle,
        "artists" => ItemKey::TrackArtist,
        "genres" => ItemKey::Genre,
        _ => return Err("This collection does not have an embedded name".into()),
    };
    crate::atomic_file::edit_checked(path, Some(expected_version), |staged| {
        let mut file = Probe::open(staged)
            .and_then(|probe| Ok(probe.guess_file_type()?))
            .and_then(|probe| probe.read())
            .map_err(|e| e.to_string())?;
        let renamed_artist = if kind == "artists" {
            let source = file.primary_tag().or_else(|| file.first_tag());
            let raw = source
                .and_then(|tag| tag.get_string(&ItemKey::TrackArtist))
                .unwrap_or(old_name);
            let title = source
                .and_then(|tag| tag.get_string(&ItemKey::TrackTitle))
                .unwrap_or_default();
            Some(
                crate::artist_credits::rename(raw, title, old_name, name).ok_or_else(|| {
                    "The artist credit changed. Reopen the collection before retrying.".to_string()
                })?,
            )
        } else {
            None
        };
        let types: Vec<_> = file.tags().iter().map(Tag::tag_type).collect();
        for tag_type in types {
            if let Some(tag) = file.tag_mut(tag_type) {
                set_text(tag, key.clone(), renamed_artist.as_deref().unwrap_or(name));
                // Keep an existing matching album credit consistent. Guest or
                // compilation album credits must retain their original value.
                if kind == "artists" && tag.get_string(&ItemKey::AlbumArtist) == Some(old_name) {
                    set_text(tag, ItemKey::AlbumArtist, name);
                }
            }
        }
        let tag = writable_tag_mut(&mut file)?;
        set_text(tag, key, renamed_artist.as_deref().unwrap_or(name));
        save_and_verify(staged, &file)
    })
}

fn read_cover(path: &Path) -> Result<Picture, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Cover image exceeds 16 MB".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Invalid cover image: {e}"))?;
    let mut picture =
        Picture::from_reader(&mut std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    picture.set_pic_type(PictureType::CoverFront);
    Ok(picture)
}

fn clear_all_pictures(file: &mut lofty::file::TaggedFile) {
    let types: Vec<_> = file.tags().iter().map(Tag::tag_type).collect();
    for kind in types {
        if let Some(tag) = file.tag_mut(kind) {
            clear_pictures(tag);
        }
    }
}
fn save_and_verify(path: &Path, expected: &lofty::file::TaggedFile) -> Result<(), String> {
    expected
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())?;
    let saved = Probe::open(path)
        .and_then(|probe| Ok(probe.guess_file_type()?))
        .and_then(|probe| probe.read())
        .map_err(|e| format!("Metadata verification failed: {e}"))?;
    if saved.file_type() != expected.file_type()
        || saved.properties().sample_rate() != expected.properties().sample_rate()
        || saved.properties().channels() != expected.properties().channels()
    {
        return Err("Metadata verification failed: audio properties changed".into());
    }
    let actual = saved.primary_tag().or_else(|| saved.first_tag());
    let expected_tag = expected.primary_tag().or_else(|| expected.first_tag());
    if let (Some(actual), Some(expected_tag)) = (actual, expected_tag) {
        for key in [
            ItemKey::TrackTitle,
            ItemKey::TrackArtist,
            ItemKey::AlbumTitle,
            ItemKey::AlbumArtist,
            ItemKey::Genre,
            ItemKey::Lyrics,
            ItemKey::Composer,
            ItemKey::Comment,
            ItemKey::ParentalAdvisory,
        ] {
            if actual.get_string(&key) != expected_tag.get_string(&key) {
                return Err("Metadata verification failed: the file format did not preserve the edited fields".into());
            }
        }
        if advisory_value(actual) != advisory_value(expected_tag) {
            return Err("Metadata verification failed: content advisory was not preserved".into());
        }
        if actual.year() != expected_tag.year()
            || actual.track() != expected_tag.track()
            || actual.disk() != expected_tag.disk()
        {
            return Err("Metadata verification failed: numeric fields were not preserved".into());
        }
        if actual.pictures().len() != expected_tag.pictures().len()
            || actual
                .pictures()
                .iter()
                .zip(expected_tag.pictures())
                .any(|(saved, expected)| saved.data() != expected.data())
        {
            return Err("Metadata verification failed: cover changes were not preserved".into());
        }
    } else {
        return Err("Metadata verification failed: saved tags could not be read".into());
    }
    Ok(())
}

pub fn replace_song_cover(song_path: &Path, image_path: &Path) -> Result<(), String> {
    let picture = read_cover(image_path)?;
    crate::atomic_file::edit(song_path, |staged| {
        let mut file = Probe::open(staged)
            .and_then(|probe| Ok(probe.guess_file_type()?))
            .and_then(|probe| probe.read())
            .map_err(|e| e.to_string())?;
        clear_all_pictures(&mut file);
        writable_tag_mut(&mut file)?.push_picture(picture);
        save_and_verify(staged, &file)
    })
}
pub fn remove_song_cover(song_path: &Path) -> Result<(), String> {
    crate::atomic_file::edit(song_path, |staged| {
        let mut file = Probe::open(staged)
            .and_then(|probe| Ok(probe.guess_file_type()?))
            .and_then(|probe| probe.read())
            .map_err(|e| e.to_string())?;
        writable_tag_mut(&mut file)?;
        clear_all_pictures(&mut file);
        save_and_verify(staged, &file)
    })
}

fn writable_tag_mut(tagged_file: &mut lofty::file::TaggedFile) -> Result<&mut Tag, String> {
    ensure_primary_tag(tagged_file)?;
    let primary_type = tagged_file.primary_tag_type();

    if tagged_file.contains_tag_type(primary_type) {
        return tagged_file
            .primary_tag_mut()
            .ok_or_else(|| "Could not access the primary metadata tag".to_string());
    }

    tagged_file
        .first_tag_mut()
        .ok_or_else(|| "Could not create a writable metadata tag".to_string())
}

fn ensure_primary_tag(tagged_file: &mut lofty::file::TaggedFile) -> Result<(), String> {
    if tagged_file.primary_tag().is_some() {
        return Ok(());
    }

    let tag_type = tagged_file.primary_tag_type();
    tagged_file.insert_tag(Tag::new(tag_type));
    Ok(())
}

fn set_text(tag: &mut Tag, key: ItemKey, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        tag.remove_key(&key);
    } else {
        tag.insert_text(key, trimmed.to_string());
    }
}

fn set_optional_text(tag: &mut Tag, key: ItemKey, value: Option<&str>) {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            tag.insert_text(key, value.to_string());
        }
        None => tag.remove_key(&key),
    }
}

fn set_optional_number(tag: &mut Tag, key: ItemKey, value: Option<i32>) {
    match value.filter(|value| *value > 0) {
        Some(value) => {
            tag.insert_text(key, value.to_string());
        }
        None => tag.remove_key(&key),
    }
}

fn clear_pictures(tag: &mut Tag) {
    while !tag.pictures().is_empty() {
        tag.remove_picture(0);
    }
}
