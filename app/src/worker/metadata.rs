//! Editor work is local and independent of the network operation queue.
use crate::protocol::{Event, MetadataAssetKind, MetadataRequest};
use orca_services::Backend;
use std::sync::mpsc::Sender;

/// A committed edit returns its path so playback analysis can be refreshed.
pub(super) fn handle(
    backend: &Backend,
    events: &Sender<Event>,
    request: MetadataRequest,
) -> Option<String> {
    let generation = request.generation();
    let result = (|| -> Result<Option<String>, String> {
        match request {
            MetadataRequest::Load {
                path,
                fetch_cover,
                generation,
            } => {
                let document = backend.read_metadata(&path)?;
                let _ = events.send(Event::Metadata(document, fetch_cover, generation));
                let _ = events.send(Event::LibraryChanged);
            }
            MetadataRequest::Save { edit, generation } => {
                let path = edit.fields.path.clone();
                backend.write_metadata(edit)?;
                let _ = events.send(Event::MetadataSaved(path.clone(), generation));
                return Ok(Some(path));
            }
            MetadataRequest::SelectAsset {
                kind,
                target,
                file,
                generation,
            } => {
                let content = if kind == MetadataAssetKind::Lyrics {
                    backend.read_lyrics_file(&file)?
                } else {
                    file
                };
                let _ = events.send(Event::MetadataAsset(kind, target, content, generation));
            }
        }
        Ok(None)
    })();
    match result {
        Ok(path) => path,
        Err(error) => {
            let _ = events.send(Event::MetadataFailed(generation, error));
            None
        }
    }
}
