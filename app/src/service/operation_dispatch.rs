//! Operation routing and completion policy, independent of UI handles.
use crate::protocol::{Event, OperationError, OperationRequest, OperationResult};
use orca_services::Backend;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{Sender, SyncSender, TrySendError},
};

/// Publish one refresh for a completed automatic batch, and none for misses.
#[derive(Default)]
pub(crate) struct AutomaticArtworkBatch {
    changed: bool,
}
impl AutomaticArtworkBatch {
    pub(crate) fn record(&mut self, result: &Result<OperationResult, OperationError>) {
        self.changed |= matches!(result, Ok(OperationResult::Artwork(_)));
    }
    pub(crate) fn finish(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

pub(crate) fn enqueue(
    queue: &SyncSender<OperationRequest>,
    events: &Sender<Event>,
    request: OperationRequest,
) -> bool {
    match queue.try_send(request) {
        Ok(()) => true,
        Err(error) => {
            let (request, error) = match error {
                TrySendError::Full(request) => (request, OperationError::Busy),
                TrySendError::Disconnected(request) => (
                    request,
                    OperationError::Unavailable(
                        "Library worker stopped. Please restart Orca.".into(),
                    ),
                ),
            };
            let _ = events.send(Event::OperationFailed(request, error));
            false
        }
    }
}
pub(crate) fn execute(
    backend: &Backend,
    events: &Sender<Event>,
    request: OperationRequest,
    stop: &AtomicBool,
    auto_artwork: &AtomicBool,
) -> Vec<OperationRequest> {
    if stop.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let result = backend.execute_cancellable(&request, stop);
    if stop.load(Ordering::Relaxed) {
        return Vec::new();
    }
    match result {
        Ok(OperationResult::MissingArtwork(jobs)) => {
            if auto_artwork.load(Ordering::Relaxed) {
                return jobs.jobs;
            }
        }
        Ok(result) => {
            let _ = events.send(Event::Operation(request, result));
        }
        Err(error) => {
            let _ = events.send(Event::OperationFailed(request, error));
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    #[test]
    fn automatic_misses_never_refresh_and_successes_refresh_once_per_batch() {
        let mut batch = AutomaticArtworkBatch::default();
        for _ in 0..10 {
            batch.record(&Err(OperationError::Failed("Artwork not found".into())));
        }
        assert!(!batch.finish());
        let result = Ok(OperationResult::Artwork(
            orca_services::operation_types::ArtworkResult {
                key: "Artist:Album".into(),
                kind: orca_services::operation_types::CollectionKind::Albums,
                artwork: "cover.png".into(),
                thumbnail: "thumb.png".into(),
            },
        ));
        batch.record(&result);
        batch.record(&result);
        assert!(batch.finish());
        assert!(
            !batch.finish(),
            "unchanged idle loops must not refresh again"
        );
    }
    #[test]
    fn shutdown_skips_pending_file_work_and_its_completion() {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../target/slint/operation-cancel-test-{}",
            std::process::id()
        ));
        let backend = orca_services::new_backend(&directory.to_string_lossy(), false).unwrap();
        let (events, receiver) = mpsc::channel();
        let request = OperationRequest::ReadLyrics {
            key: "song".into(),
            file: directory.join("absent.lrc").to_string_lossy().into_owned(),
        };
        execute(
            &backend,
            &events,
            request.clone(),
            &AtomicBool::new(true),
            &AtomicBool::new(false),
        );
        assert!(
            receiver.try_recv().is_err(),
            "cancelled work must not publish a completion"
        );
        execute(
            &backend,
            &events,
            request.clone(),
            &AtomicBool::new(false),
            &AtomicBool::new(false),
        );
        assert!(
            matches!(receiver.recv().unwrap(), Event::OperationFailed(actual, OperationError::Failed(_)) if actual == request)
        );
    }
    #[test]
    fn full_queue_preserves_failed_request_identity() {
        let (queue, _receiver) = mpsc::sync_channel(1);
        let (events, receiver) = mpsc::channel();
        queue.send(OperationRequest::LibrarySources).unwrap();
        let request = OperationRequest::RemoveSource {
            key: "music".into(),
        };
        assert!(!enqueue(&queue, &events, request.clone()));
        assert!(
            matches!(receiver.recv().unwrap(), Event::OperationFailed(actual, OperationError::Busy) if actual == request)
        );
    }
    #[test]
    fn stopped_worker_reports_failure_instead_of_success() {
        let (queue, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let (events, receiver) = mpsc::channel();
        assert!(!enqueue(&queue, &events, OperationRequest::LibrarySources));
        assert!(matches!(
            receiver.recv().unwrap(),
            Event::OperationFailed(
                OperationRequest::LibrarySources,
                OperationError::Unavailable(_)
            )
        ));
    }
}
