use crate::{
    operation_types::{
        ArtworkResult, CollectionKind, CoverFetch, MissingArtwork, OperationRequest,
        OperationResult,
    },
    Backend,
};
use orca_core::db;
use serde_json::json;
use std::collections::HashSet;

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    let response = match operation {
        OperationRequest::MissingArtwork => {
            let overrides = backend.collection_overrides()?;
            let mut jobs = Vec::new();
            let mut candidates = HashSet::new();
            for name in db::get_artists_needing_artwork(&backend.conn)? {
                candidates.insert(("artists".to_string(), name, String::new()));
            }
            for (_, title, artist) in db::get_albums_needing_artwork(&backend.conn)? {
                candidates.insert(("albums".to_string(), title, artist));
            }
            for kind in ["artists", "albums"] {
                for group in backend.groups(kind, "")? {
                    let identity = json!([kind, group.key, group.secondary]).to_string();
                    if overrides.get(&identity).and_then(|v| v["cover"].as_str()) == Some("") {
                        candidates.insert((kind.into(), group.key, group.secondary));
                    }
                }
            }
            for (kind, key, secondary) in candidates {
                let identity = json!([kind, key, secondary]).to_string();
                if overrides
                    .get(&identity)
                    .and_then(|v| v["cover"].as_str())
                    .is_some_and(|cover| !cover.is_empty())
                {
                    continue;
                }
                jobs.push(OperationRequest::FetchCoverAuto(CoverFetch {
                    kind: CollectionKind::try_from(kind.as_str()).map_err(|e| e.to_string())?,
                    key: if kind == "albums" {
                        format!("{secondary}:{key}")
                    } else {
                        key.clone()
                    },
                    artist: secondary,
                    album: if kind == "albums" { key } else { String::new() },
                    title: None,
                    track_artist: None,
                    duration_ms: None,
                    editor_generation: None,
                    editor_path: None,
                    collection_draft: false,
                    collection_seed: String::new(),
                }));
            }
            OperationResult::MissingArtwork(MissingArtwork { jobs })
        }
        OperationRequest::FetchCover(fetch) | OperationRequest::FetchCoverAuto(fetch) => {
            let kind = fetch.kind.as_str();
            let key = fetch.key.as_str();
            let artist = fetch.artist.as_str();
            let album = fetch.album.as_str();
            let directory = backend.data_dir.join("artwork/online");
            let track = fetch
                .editor_path
                .as_deref()
                .map(|path| backend.track(path))
                .transpose()?;
            let query = if fetch.kind == CollectionKind::Artists {
                let tracks = backend
                    .page(
                        &crate::types::Query {
                            kind: "artists".into(),
                            key: fetch.key.clone(),
                            ..Default::default()
                        },
                        0,
                        32,
                    )?
                    .tracks;
                crate::artwork::artist_query(fetch, &tracks)
            } else {
                crate::artwork::query(fetch, track.as_ref())
            };
            let mode = if matches!(operation, OperationRequest::FetchCoverAuto(_)) {
                orca_core::online_artwork::LookupMode::Automatic
            } else {
                orca_core::online_artwork::LookupMode::Manual
            };
            let paths = match stop {
                Some(stop) => {
                    orca_core::online_artwork::fetch_cancellable(&query, &directory, mode, stop)?
                }
                None => orca_core::online_artwork::fetch(&query, &directory, mode)?,
            };
            if stop.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
                return Err("Artwork search cancelled".into());
            }
            let staged = fetch.editor_path.is_some() || fetch.collection_draft;
            if !staged {
                if kind == "artists" {
                    db::update_artist_artwork(
                        &backend.conn,
                        key,
                        Some(&paths.full),
                        Some(&paths.thumb),
                    )?;
                } else {
                    db::update_album_artwork(
                        &backend.conn,
                        key,
                        Some(&paths.full),
                        Some(&paths.thumb),
                    )?;
                }
                backend.clear_collection_cover_override(
                    kind,
                    if kind == "albums" { album } else { key },
                    if kind == "albums" { artist } else { "" },
                )?;
            }
            OperationResult::Artwork(ArtworkResult {
                key: key.into(),
                kind: fetch.kind.clone(),
                artwork: paths.full,
                thumbnail: paths.thumb,
            })
        }
        _ => return Err("Invalid artwork operation routing".into()),
    };
    Ok(response)
}
