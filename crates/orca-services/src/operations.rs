//! Exhaustive operation routing. Domains own SQL, provider work and file mutations.
mod artwork;
mod catalog;
mod collection_names;
mod media;
mod mutations;
mod playlists;
use super::{operation_types::*, Backend};
use std::path::Path;
pub(super) fn run_with_stop(
    backend: &Backend,
    operation: &OperationRequest,
    stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    match operation {
        OperationRequest::CollectionEdit(_) => mutations::run(backend, operation, stop),
        OperationRequest::LibrarySources => mutations::run(backend, operation, stop),
        OperationRequest::RemoveSource { .. } => mutations::run(backend, operation, stop),
        OperationRequest::MissingArtwork => artwork::run(backend, operation, stop),
        OperationRequest::ArtistDetail { .. } => catalog::run(backend, operation, stop),
        OperationRequest::CollectionPreview(_) => catalog::run(backend, operation, stop),
        OperationRequest::GroupDetail(_) => catalog::run(backend, operation, stop),
        OperationRequest::ArtistQueue { .. } | OperationRequest::GroupQueue(_) => {
            catalog::run(backend, operation, stop)
        }
        OperationRequest::FetchCover(_) | OperationRequest::FetchCoverAuto(_) => {
            artwork::run(backend, operation, stop)
        }
        OperationRequest::FetchLyrics(_) => media::run(backend, operation, stop),
        OperationRequest::Cover { .. } | OperationRequest::RemoveCover { .. } => {
            media::run(backend, operation, stop)
        }
        OperationRequest::ReadLyrics { .. } => media::run(backend, operation, stop),
        OperationRequest::ImportPlaylist { .. } => playlists::run(backend, operation, stop),
        OperationRequest::ExportPlaylist { .. } => playlists::run(backend, operation, stop),
    }
}

fn path_key(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let value = path.to_string_lossy().replace('/', "\\");
    if cfg!(target_os = "windows") {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn path_in_root(path: &str, root: &str) -> bool {
    let path = path_key(Path::new(path));
    path == root || path.starts_with(&format!("{}\\", root.trim_end_matches('\\')))
}
