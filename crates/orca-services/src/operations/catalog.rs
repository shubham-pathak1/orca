use crate::{
    operation_types::{
        AlbumDetail, CollectionDetail, CollectionIdentity, CollectionKind, OperationRequest,
        OperationResult, QueueSong, Songs,
    },
    Backend,
};
use orca_core::db;
use serde_json::json;

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    if stop.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err("Operation cancelled".into());
    }
    let mut response;
    match operation {
        OperationRequest::ArtistDetail { key, .. } => {
            let key = key.as_str();
            let artist = db::get_artists(&backend.conn)?
                .into_iter()
                .find(|a| a.name == key)
                .ok_or_else(|| "Artist not found".to_string())?;
            let (count, duration): (i64, i64) = backend
                .conn
                .query_row(
                    "SELECT COUNT(*), COALESCE(SUM(duration),0) FROM songs WHERE artist=?1",
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| e.to_string())?;
            let mut statement = backend
                .conn
                .prepare("SELECT DISTINCT album_artist,album FROM songs WHERE artist=?1")
                .map_err(|e| e.to_string())?;
            let memberships = statement
                .query_map([key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|e| e.to_string())?;
            let albums: Vec<_> = db::get_albums(&backend.conn)?
                .into_iter()
                .filter(|a| memberships.contains(&(a.artist.clone(), a.title.clone())))
                .collect();
            let artwork = artist
                .artwork
                .filter(|cover| !cover.is_empty())
                .or(artist.artwork_thumb.filter(|cover| !cover.is_empty()))
                .or(artist.song_artwork.filter(|cover| !cover.is_empty()))
                .or(artist.song_artwork_thumb.filter(|cover| !cover.is_empty()));
            let backdrop = backend.small_artwork(artwork.as_deref())?;
            response = OperationResult::Detail(CollectionDetail {
                identity: CollectionIdentity {
                    kind: CollectionKind::Artists,
                    key: key.into(),
                    secondary: String::new(),
                },
                title: key.into(),
                artwork,
                backdrop,
                count: count.max(0) as u64,
                duration: duration.max(0) as u64,
                album_count: albums.len() as u64,
                albums: albums.into_iter().take(6).map(album_detail).collect(),
            });
        }
        OperationRequest::CollectionPreview(preview) => {
            let identity = &preview.identity;
            let detail = if identity.kind == crate::operation_types::CollectionKind::Artists {
                OperationRequest::ArtistDetail {
                    key: identity.key.clone(),
                    secondary: identity.secondary.clone(),
                }
            } else {
                OperationRequest::GroupDetail(identity.clone())
            };
            let OperationResult::Detail(mut response) = run(backend, &detail, stop)? else {
                return Err("Expected collection detail".into());
            };
            let overridden = backend
                .collection_overrides()?
                .get(&json!([identity.kind.as_str(), identity.key, identity.secondary]).to_string())
                .is_some_and(|entry| {
                    entry.get("cover").is_some()
                        && (identity.kind != crate::operation_types::CollectionKind::Albums
                            || entry["cover"]
                                .as_str()
                                .is_some_and(|cover| !cover.is_empty()))
                });
            if identity.kind == crate::operation_types::CollectionKind::Albums && !overridden {
                use rusqlite::OptionalExtension;
                let artwork: Option<String> = backend.conn.query_row(
                    "SELECT COALESCE(NULLIF(a.artwork_path,'DELETED'),NULLIF(s.artwork_url,''),NULLIF(s.artwork_preview_url,''),NULLIF(s.artwork_thumb_url,'')) AS cover FROM songs s LEFT JOIN album_artworks a ON a.album_key=s.album_artist || ':' || s.album WHERE s.album=?1 AND s.album_artist=?2 AND cover IS NOT NULL ORDER BY s.disc_number,s.track_number,s.id LIMIT 1",
                    rusqlite::params![identity.key, identity.secondary], |row| row.get(0),
                ).optional().map_err(|error| error.to_string())?;
                response.artwork = artwork;
            }
            return Ok(OperationResult::Detail(response));
        }
        OperationRequest::GroupDetail(identity) => {
            let kind = identity.kind.as_str();
            let key = identity.key.as_str();
            let secondary = identity.secondary.as_str();
            let (title, artwork, albums) = match kind {
                "folders" => {
                    let selected = backend.folder_group(key)?;
                    (
                        selected.title,
                        Some(selected.artwork).filter(|cover| !cover.is_empty()),
                        Vec::new(),
                    )
                }
                "albums" => {
                    let all = db::get_albums(&backend.conn)?;
                    let selected = all
                        .iter()
                        .find(|a| a.title == key && a.artist == secondary)
                        .ok_or_else(|| "Album not found".to_string())?;
                    let artwork = selected.artwork.clone();
                    let related: Vec<_> = all
                        .into_iter()
                        .filter(|a| a.artist == secondary && a.title != key)
                        .take(6)
                        .collect();
                    (key.to_string(), artwork, related)
                }
                "genres" => {
                    let selected = db::get_genres(&backend.conn)?
                        .into_iter()
                        .find(|g| g.name == key)
                        .ok_or_else(|| "Genre not found".to_string())?;
                    (selected.name, selected.song_artwork, Vec::new())
                }
                "playlists" => {
                    let playlist_id = key.parse::<i64>().map_err(|_| "Invalid playlist id")?;
                    let selected = db::get_playlists(&backend.conn)?
                        .into_iter()
                        .find(|p| p.id == playlist_id)
                        .ok_or_else(|| "Playlist not found".to_string())?;
                    let artwork = backend.playlist_artwork(playlist_id, selected.cover_path)?;
                    (selected.name, artwork, Vec::new())
                }
                _ => return Err("Unknown detail type".into()),
            };
            let sql = format!("SELECT COUNT(*),COALESCE(SUM(s.duration),0) FROM songs s WHERE ({}) AND (?3 IS NOT NULL)", crate::catalog::predicate(kind));
            let (count, duration): (i64, i64) = backend
                .conn
                .query_row(&sql, rusqlite::params!["", key, secondary], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(|e| e.to_string())?;
            let backdrop = backend.small_artwork(artwork.as_deref())?;
            response = OperationResult::Detail(CollectionDetail {
                identity: identity.clone(),
                title,
                artwork,
                backdrop,
                count: count.max(0) as u64,
                duration: duration.max(0) as u64,
                album_count: 0,
                albums: albums.into_iter().map(album_detail).collect(),
            });
        }
        OperationRequest::ArtistQueue { .. } | OperationRequest::GroupQueue(_) => {
            let (kind, key, secondary) = match operation {
                OperationRequest::ArtistQueue { key, secondary } => ("artists", key, secondary),
                OperationRequest::GroupQueue(identity) => {
                    (identity.kind.as_str(), &identity.key, &identity.secondary)
                }
                _ => unreachable!(),
            };
            if !["artists", "albums", "genres", "playlists", "folders"].contains(&kind) {
                return Err("Unknown queue group type".into());
            }
            let query = crate::types::Query {
                kind: kind.into(),
                key: key.clone(),
                secondary: secondary.clone(),
                sort: "title".into(),
                ..Default::default()
            };
            let mut songs = Vec::new();
            let mut offset = 0;
            loop {
                let page = backend.page(&query, offset, 512)?;
                if page.tracks.is_empty() {
                    break;
                }
                offset += page.tracks.len() as u32;
                songs.extend(page.tracks.into_iter().map(|s| QueueSong {
                    id: s.id,
                    path: s.path,
                    title: s.title,
                    artist: s.artist,
                    album: s.album,
                    album_artist: s.album_artist,
                    artwork: s.artwork_thumb,
                    duration: s.duration_ms,
                }));
                if offset >= page.total {
                    break;
                }
            }
            response = OperationResult::Songs(Songs { songs });
        }
        _ => return Err("Invalid catalog operation routing".into()),
    }

    let detail_identity = match operation {
        OperationRequest::ArtistDetail { key, secondary } => {
            Some(("artists", key.as_str(), secondary.as_str()))
        }
        OperationRequest::GroupDetail(identity) => Some((
            identity.kind.as_str(),
            identity.key.as_str(),
            identity.secondary.as_str(),
        )),
        _ => None,
    };
    if let (OperationResult::Detail(detail), Some((kind, key, secondary))) = (
        &mut response,
        detail_identity.filter(|(kind, _, _)| *kind != "folders"),
    ) {
        if let Some(group) = backend
            .groups(kind, "")?
            .into_iter()
            .find(|g| g.key == key && g.secondary == secondary)
        {
            detail.title = group.title;
            let overrides = backend.collection_overrides()?;
            if let Some(cover) = overrides
                .get(&json!([kind, key, secondary]).to_string())
                .and_then(|v| v["cover"].as_str())
                .filter(|cover| {
                    !matches!(kind, "albums" | "artists" | "folders") || !cover.is_empty()
                })
            {
                detail.artwork = Some(cover.into());
                detail.backdrop = overrides[&json!([kind, key, secondary]).to_string()]["thumb"]
                    .as_str()
                    .map(str::to_owned);
            }
        }
    }
    if let OperationResult::Detail(detail) = &mut response {
        let overrides = backend.collection_overrides()?;
        for album in &mut detail.albums {
            let key = album.title.clone();
            let artist = album.artist.clone();
            album.navigation_title = key.clone();
            if let Some(entry) = overrides.get(&json!(["albums", key, artist]).to_string()) {
                if let Some(name) = entry["name"].as_str() {
                    album.title = name.into();
                }
                if let Some(cover) = entry["cover"].as_str().filter(|cover| !cover.is_empty()) {
                    album.artwork = Some(cover.into());
                }
            }
        }
    }
    Ok(response)
}

fn album_detail(album: db::AlbumEntry) -> AlbumDetail {
    AlbumDetail {
        navigation_title: album.title.clone(),
        key: album.key,
        title: album.title,
        artist: album.artist,
        artwork: album.artwork,
        artwork_thumb: album.artwork_thumb,
        song_count: album.song_count.max(0) as u32,
        duration: album.duration.max(0) as u64,
    }
}
