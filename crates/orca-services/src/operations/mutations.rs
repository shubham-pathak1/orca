use super::{path_in_root, path_key};
use crate::{
    operation_types::{LibrarySource, OperationRequest, OperationResult, SourceRemoved, Sources},
    Backend,
};
use orca_core::db;
use serde_json::json;
use std::{fs, path::Path};

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    let mut response = OperationResult::CollectionEdited;
    match operation {
        OperationRequest::CollectionEdit(edit) => {
            let identity = &edit.identity;
            let kind = identity.kind.as_str();
            let key = identity.key.as_str();
            let secondary = if kind == "folders" {
                ""
            } else {
                identity.secondary.as_str()
            };
            if kind == "folders" {
                backend.folder_group(key)?;
            } else if !backend
                .raw_groups(kind, "")?
                .iter()
                .any(|g| g.key == key && g.secondary == secondary)
            {
                return Err("Collection no longer exists".into());
            }
            let name = edit.name.as_deref().map(str::trim);
            if name.is_some_and(str::is_empty) {
                return Err("Please enter a name".into());
            }
            if kind == "folders" && name.is_some() {
                return Err("Folder names come from the directory. Change its cover here, or rename the directory in Explorer.".into());
            }
            // Validate and cache the image before updating names or settings.
            let cover = match edit.cover.as_deref() {
                Some("") => Some((String::new(), String::new())),
                Some(file) => {
                    image::image_dimensions(file)
                        .map_err(|e| format!("Could not read cover: {e}"))?;
                    let bytes = fs::read(file).map_err(|e| e.to_string())?;
                    let format = image::guess_format(&bytes).map_err(|e| e.to_string())?;
                    let paths = orca_core::artwork_cache::persist_artwork(
                        &backend.data_dir.join("artwork"),
                        &bytes,
                        Some(format.to_mime_type()),
                    )?;
                    Some((paths.full, paths.thumb))
                }
                None => None,
            };
            let mut overrides = backend.collection_overrides()?;
            let rename =
                name.filter(|name| *name != key && matches!(kind, "artists" | "albums" | "genres"));
            let new_key = rename.unwrap_or(key);
            let old_identity = json!([kind, key, secondary]).to_string();
            let new_identity = json!([kind, new_key, secondary]).to_string();
            let renamed_albums: Vec<String> = if kind == "artists" && rename.is_some() {
                let mut statement = backend
                    .conn
                    .prepare("SELECT DISTINCT album FROM songs WHERE artist=?1 AND album_artist=?1")
                    .map_err(|e| e.to_string())?;
                let albums = statement
                    .query_map([key], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                albums
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?
            } else {
                Vec::new()
            };
            if overrides
                .get(&old_identity)
                .is_some_and(|entry| !entry.is_object())
            {
                return Err("Invalid collection settings; the edit was not saved".into());
            }
            if let Some(name) = rename {
                if backend
                    .raw_groups(kind, "")?
                    .iter()
                    .any(|g| g.key == name && g.secondary == secondary)
                {
                    return Err(
                        "A collection with this name already exists. Choose a different name."
                            .into(),
                    );
                }
                // Never move UI identity until all source-file writes succeed.
                super::collection_names::rename(backend, identity, name, stop)?;
                if let Some(entry) = overrides.remove(&old_identity) {
                    overrides.insert(new_identity.clone(), entry);
                }
                for album in &renamed_albums {
                    if let Some(entry) = overrides
                        .get(&json!(["albums", album, key]).to_string())
                        .cloned()
                    {
                        overrides
                            .entry(json!(["albums", album, new_key]).to_string())
                            .or_insert(entry);
                    }
                }
            }
            let entry = overrides.entry(new_identity).or_insert_with(|| json!({}));
            if !entry.is_object() {
                return Err("Invalid collection settings; the edit was not saved".into());
            }
            let tx = backend
                .conn
                .unchecked_transaction()
                .map_err(|e| e.to_string())?;
            if let Some(name) = name {
                if kind == "playlists" {
                    db::rename_playlist(&tx, key.parse().map_err(|_| "Invalid playlist")?, name)?;
                    entry.as_object_mut().unwrap().remove("name");
                } else {
                    entry.as_object_mut().unwrap().remove("name");
                }
            }
            if rename.is_some() {
                match kind {
                    "artists" => {
                        tx.execute(
                            "UPDATE artist_artworks SET artist_name=?1 WHERE artist_name=?2",
                            [new_key, key],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    "albums" => {
                        tx.execute(
                            "UPDATE album_artworks SET album_key=?1 WHERE album_key=?2",
                            [
                                format!("{secondary}:{new_key}"),
                                format!("{secondary}:{key}"),
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    _ => {}
                }
            }
            for album in renamed_albums {
                tx.execute("INSERT OR IGNORE INTO album_artworks(album_key,artwork_path,artwork_thumb_path) SELECT ?1,artwork_path,artwork_thumb_path FROM album_artworks WHERE album_key=?2",
                    [format!("{new_key}:{album}"), format!("{key}:{album}")]).map_err(|e| e.to_string())?;
            }
            if let Some((full, thumb)) = cover {
                // Removal applies to fetched artwork too; an older database cover
                // must not reappear underneath an empty collection override.
                if full.is_empty() {
                    match kind {
                        "artists" => db::remove_artist_artwork(&tx, new_key)?,
                        "albums" => {
                            db::remove_album_artwork(&tx, &format!("{secondary}:{new_key}"))?
                        }
                        _ => {}
                    }
                }
                entry["cover"] = json!(full);
                entry["thumb"] = json!(thumb);
            }
            db::set_setting(
                &tx,
                "collection_overrides",
                &serde_json::to_string(&overrides).map_err(|e| e.to_string())?,
            )?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        OperationRequest::LibrarySources => {
            let index = db::get_song_path_index(&backend.conn)?;
            response = OperationResult::Sources(Sources {
                sources: backend
                    .statistics()?
                    .roots
                    .into_iter()
                    .map(|root| {
                        let available = Path::new(&root).is_dir();
                        let key = path_key(Path::new(&root));
                        let count = index
                            .iter()
                            .filter(|(path, _)| path_in_root(path, &key))
                            .count();
                        LibrarySource {
                            path: root,
                            available,
                            song_count: count,
                        }
                    })
                    .collect(),
            });
        }
        OperationRequest::RemoveSource { key } => {
            let mut roots = backend.statistics()?.roots;
            let target = path_key(Path::new(key));
            if !roots.iter().any(|root| path_key(Path::new(root)) == target) {
                return Err("Folder is no longer part of the library".into());
            }
            roots.retain(|root| path_key(Path::new(root)) != target);
            let remaining: Vec<_> = roots.iter().map(|root| path_key(Path::new(root))).collect();
            let tx = backend
                .conn
                .unchecked_transaction()
                .map_err(|e| e.to_string())?;
            let mut removed = Vec::new();
            for (path, id) in db::get_song_path_index(&tx)? {
                if path_in_root(&path, &target)
                    && !remaining.iter().any(|root| path_in_root(&path, root))
                {
                    tx.execute("DELETE FROM playlist_songs WHERE song_id=?1", [id])
                        .map_err(|e| e.to_string())?;
                    db::delete_song_by_path(&tx, &path)?;
                    removed.push(path);
                }
            }
            db::set_setting(
                &tx,
                "library_scan_roots",
                &serde_json::to_string(&roots).map_err(|e| e.to_string())?,
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            response = OperationResult::SourceRemoved(SourceRemoved {
                removed_paths: removed,
            });
        }
        _ => return Err("Invalid mutations operation routing".into()),
    }
    Ok(response)
}
