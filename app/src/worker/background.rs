//! Background worker ownership: bounded network queue and latest-only analysis.
use crate::jobs::LatestReceiver;
use crate::{operation_dispatch, protocol::Event};
use orca_services::{
    new_backend,
    operation_types::{OperationError, OperationRequest},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};
pub(super) struct BackgroundWorkers {
    pub directory: PathBuf,
    pub stop: Arc<AtomicBool>,
    pub events: Sender<Event>,
    pub auto_artwork: Arc<AtomicBool>,
    pub analysis_generation: Arc<AtomicU64>,
    pub operation_rx: Receiver<OperationRequest>,
    pub analysis_rx: LatestReceiver<(u64, String)>,
    pub lyrics_rx: LatestReceiver<(u64, String)>,
}
pub(super) fn spawn(workers: BackgroundWorkers) -> Vec<thread::JoinHandle<()>> {
    let BackgroundWorkers {
        directory,
        stop,
        events,
        auto_artwork,
        analysis_generation,
        operation_rx,
        analysis_rx,
        lyrics_rx,
    } = workers;
    let mut handles = Vec::new();
    {
        let directory = directory.clone();
        let stop = stop.clone();
        let events = events.clone();
        let auto_artwork = auto_artwork.clone();
        handles.push(thread::spawn(move || {
            let mut backend = None;
            let mut automatic_jobs = std::collections::VecDeque::new();
            let mut artwork_batch = operation_dispatch::AutomaticArtworkBatch::default();
            while !stop.load(Ordering::Relaxed) {
                if !auto_artwork.load(Ordering::Relaxed) {
                    automatic_jobs.clear();
                }
                if automatic_jobs.is_empty() && artwork_batch.finish() {
                    let _ = events.send(Event::AutomaticArtworkUpdated);
                }
                let (request, automatic) = match operation_rx.try_recv() {
                    Ok(request) => (request, false),
                    Err(_) if !automatic_jobs.is_empty() => {
                        (automatic_jobs.pop_front().unwrap(), true)
                    }
                    Err(_) => match operation_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => (request, false),
                        Err(_) => continue,
                    },
                };
                if backend.is_none() {
                    match new_backend(&directory.to_string_lossy(), false) {
                        Ok(value) => backend = Some(value),
                        Err(error) => {
                            let _ = events.send(Event::OperationFailed(
                                request,
                                OperationError::Unavailable(error),
                            ));
                            continue;
                        }
                    }
                }
                if automatic {
                    let result = backend
                        .as_ref()
                        .unwrap()
                        .execute_cancellable(&request, &stop);
                    if !stop.load(Ordering::Relaxed) {
                        artwork_batch.record(&result);
                    }
                    continue;
                }
                let collecting_jobs = matches!(&request, OperationRequest::MissingArtwork);
                let jobs = operation_dispatch::execute(
                    backend.as_ref().unwrap(),
                    &events,
                    request,
                    &stop,
                    &auto_artwork,
                );
                if collecting_jobs {
                    // A post-scan collection supersedes the startup list;
                    // don't fetch the same pending covers twice.
                    automatic_jobs = jobs.into();
                }
            }
        }));
    }
    for (jobs, network) in [(analysis_rx, false), (lyrics_rx, true)] {
        let directory = directory.clone();
        let events = events.clone();
        let stop = stop.clone();
        let generation = analysis_generation.clone();
        handles.push(thread::spawn(move || {
            let mut backend = None;
            while !stop.load(Ordering::Relaxed) {
                let Some(job) = jobs.receive() else {
                    continue;
                };
                let (revision, path) = job;
                if revision != generation.load(Ordering::Relaxed) {
                    continue;
                }
                if backend.is_none() {
                    match new_backend(&directory.to_string_lossy(), false) {
                        Ok(value) => backend = Some(value),
                        Err(error) => {
                            let _ = events.send(Event::Error(error));
                            continue;
                        }
                    }
                }
                let backend = backend.as_ref().unwrap();
                if network {
                    let lyrics = backend.fetch_missing_lyrics_cancellable(&path, Some(&stop));
                    if !stop.load(Ordering::Relaxed)
                        && revision == generation.load(Ordering::Relaxed)
                    {
                        let (text, status) = match lyrics {
                            Ok(text) if !text.is_empty() => (text, String::new()),
                            Ok(_) => (String::new(), "No lyrics found".into()),
                            Err(_) => (String::new(), "Could not fetch lyrics from LRCLIB".into()),
                        };
                        let _ = events.send(Event::Lyrics(revision, text, status));
                    }
                } else {
                    let text = backend.lyrics(&path).unwrap_or_default();
                    if !stop.load(Ordering::Relaxed)
                        && revision == generation.load(Ordering::Relaxed)
                    {
                        let _ = events.send(Event::Lyrics(
                            revision,
                            text.clone(),
                            if text.is_empty() {
                                "Fetching lyrics…".into()
                            } else {
                                String::new()
                            },
                        ));
                    }
                    let peaks = backend
                        .waveform_cancellable(&path, 720, || {
                            stop.load(Ordering::Relaxed)
                                || revision != generation.load(Ordering::Relaxed)
                        })
                        .unwrap_or_default();
                    if !stop.load(Ordering::Relaxed)
                        && revision == generation.load(Ordering::Relaxed)
                    {
                        let _ = events.send(Event::Analysis(revision, path, text, peaks));
                    }
                }
            }
        }));
    }
    handles
}
