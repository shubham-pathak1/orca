use crate::{
    operation_types::{
        ArtworkResult, CollectionIdentity, CollectionKind, OperationRequest, OperationResult,
    },
    Backend,
};
use orca_core::db;

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    let response = match operation {
        OperationRequest::ArtworkBulk(action) => {
            OperationResult::ArtworkBulk(backend.artwork_bulk_control(action)?)
        }
        OperationRequest::FetchCover(fetch) | OperationRequest::FetchCoverAuto(fetch) => {
            let automatic = matches!(operation, OperationRequest::FetchCoverAuto(_));
            let identity = CollectionIdentity {
                kind: fetch.kind.clone(),
                key: if fetch.kind == CollectionKind::Albums {
                    fetch.album.clone()
                } else {
                    fetch.key.clone()
                },
                secondary: fetch.artist.clone(),
            };
            if automatic
                && backend
                    .artwork_jobs_for_collections([identity.clone()])?
                    .is_empty()
            {
                return Ok(OperationResult::ArtworkSkipped);
            }
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
            if automatic && backend.artwork_jobs_for_collections([identity])?.is_empty() {
                return Ok(OperationResult::ArtworkSkipped);
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
