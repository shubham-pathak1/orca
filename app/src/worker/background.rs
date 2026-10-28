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
    pub bulk_cancel: Arc<AtomicBool>,
    pub artwork_rx: LatestReceiver<orca_services::operation_types::ArtworkDemand>,
    pub directory: PathBuf,
    pub stop: Arc<AtomicBool>,
    pub events: Sender<Event>,
    pub auto_artwork: Arc<AtomicBool>,
    pub analysis_generation: Arc<AtomicU64>,
    pub operation_rx: Receiver<OperationRequest>,
    pub analysis_rx: LatestReceiver<(u64, String, bool)>,
    pub lyrics_rx: LatestReceiver<(u64, String, bool)>,
}
pub(super) fn spawn(workers: BackgroundWorkers) -> Vec<thread::JoinHandle<()>> {
    let BackgroundWorkers {
        bulk_cancel,
        artwork_rx,
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
            let mut bulk_running = false;
            let mut bulk_retry_at: Option<std::time::Instant> = None;
            let mut artwork_published = std::time::Instant::now();
            let mut latest_demand = None;
            let mut last_enabled = false;
            let mut attempted = std::collections::HashSet::new();
            let mut refill = false;
            while !stop.load(Ordering::Relaxed) {
                let update = artwork_rx.try_receive();
                let enabled = auto_artwork.load(Ordering::Relaxed);
                let changed = update.is_some() || enabled != last_enabled;
                if let Some(demand) = update {
                    latest_demand = Some(demand);
                }
                last_enabled = enabled;
                if changed {
                    attempted.clear();
                    refill = true;
                    automatic_jobs.clear();
                    // The debounced Settings message can arrive after the UI's
                    // first viewport snapshot. Keep it until enabling commits.
                    if let (true, Some(demand)) = (enabled, latest_demand.as_ref()) {
                        if backend.is_none() {
                            backend = new_backend(&directory.to_string_lossy(), false).ok();
                        }
                        if let Some(backend) = backend.as_ref() {
                            automatic_jobs = backend
                                .artwork_demand_jobs_excluding(demand, &attempted)
                                .unwrap_or_default()
                                .into();
                        }
                    }
                }
                if !auto_artwork.load(Ordering::Relaxed) {
                    automatic_jobs.clear();
                }
                if enabled && refill && automatic_jobs.is_empty() {
                    refill = false;
                    if let (Some(backend), Some(demand)) =
                        (backend.as_ref(), latest_demand.as_ref())
                    {
                        automatic_jobs = backend
                            .artwork_demand_jobs_excluding(demand, &attempted)
                            .unwrap_or_default()
                            .into();
                    }
                }
                if (automatic_jobs.is_empty()
                    || artwork_published.elapsed() >= Duration::from_secs(3))
                    && artwork_batch.finish()
                {
                    let _ = events.send(Event::AutomaticArtworkUpdated);
                    artwork_published = std::time::Instant::now();
                }
                let (request, automatic) = match operation_rx.try_recv() {
                    Ok(request) => (request, false),
                    Err(_) if !automatic_jobs.is_empty() => {
                        (automatic_jobs.pop_front().unwrap(), true)
                    }
                    Err(_)
                        if bulk_running
                            && bulk_retry_at.is_none_or(|at| at <= std::time::Instant::now()) =>
                    {
                        let backend = backend.as_ref().unwrap();
                        match backend.artwork_bulk_step(&bulk_cancel) {
                            Ok((mut progress, changed)) => {
                                bulk_retry_at = progress.waiting.then(|| {
                                    std::time::Instant::now()
                                        + Duration::from_millis(progress.retry_after_ms.max(1))
                                });
                                bulk_running =
                                    !progress.complete && !bulk_cancel.load(Ordering::Relaxed);
                                progress.running = bulk_running;
                                if changed {
                                    let _ = events.send(Event::AutomaticArtworkUpdated);
                                }
                                let _ = events.send(Event::Operation(
                                    OperationRequest::ArtworkBulk(
                                        orca_services::operation_types::ArtworkBulkAction::Status,
                                    ),
                                    orca_services::operation_types::OperationResult::ArtworkBulk(
                                        progress,
                                    ),
                                ));
                            }
                            Err(error) => {
                                bulk_running = false;
                                let _ = events.send(Event::OperationFailed(
                                    OperationRequest::ArtworkBulk(
                                        orca_services::operation_types::ArtworkBulkAction::Pause,
                                    ),
                                    OperationError::Failed(error),
                                ));
                            }
                        }
                        continue;
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
                        if let OperationRequest::FetchCoverAuto(fetch) = &request {
                            attempted.insert(orca_services::operation_types::CollectionIdentity {
                                kind: fetch.kind.clone(),
                                key: if fetch.kind
                                    == orca_services::operation_types::CollectionKind::Albums
                                {
                                    fetch.album.clone()
                                } else {
                                    fetch.key.clone()
                                },
                                secondary: fetch.artist.clone(),
                            });
                            refill = true;
                        }
                    }
                    continue;
                }
                if let OperationRequest::ArtworkBulk(action) = &request {
                    use orca_services::operation_types::{ArtworkBulkAction, OperationResult};
                    match backend.as_ref().unwrap().artwork_bulk_control(action) {
                        Ok(mut progress) => {
                            if !matches!(action, ArtworkBulkAction::Status) {
                                bulk_retry_at = None;
                            }
                            match action {
                                ArtworkBulkAction::Start | ArtworkBulkAction::Resume => {
                                    bulk_running = progress.started && !progress.complete;
                                    bulk_cancel.store(!bulk_running, Ordering::Relaxed);
                                }
                                ArtworkBulkAction::Pause => {
                                    bulk_running = false;
                                }
                                ArtworkBulkAction::Status => {}
                            }
                            progress.running = bulk_running;
                            progress.waiting = bulk_running
                                && bulk_retry_at.is_some_and(|at| at > std::time::Instant::now());
                            let _ = events.send(Event::Operation(
                                request,
                                OperationResult::ArtworkBulk(progress),
                            ));
                        }
                        Err(error) => {
                            let _ = events.send(Event::OperationFailed(
                                request,
                                OperationError::Failed(error),
                            ));
                        }
                    }
                    continue;
                }
                operation_dispatch::execute(backend.as_ref().unwrap(), &events, request, &stop);
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
                let (revision, path, refresh) = job;
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
                    let lyrics = if refresh {
                        backend.refresh_lyrics_cancellable(&path, Some(&stop))
                    } else {
                        backend.fetch_missing_lyrics_cancellable(&path, Some(&stop))
                    };
                    if !stop.load(Ordering::Relaxed)
                        && revision == generation.load(Ordering::Relaxed)
                    {
                        let (text, status) = match lyrics {
                            Ok(text) if !text.is_empty() => (text, String::new()),
                            Ok(_) => (String::new(), "Oops, lyrics not found :(".into()),
                            Err(error)
                                if error == "Lyrics not found"
                                    || error == "No lyrics found for a matching recording" =>
                            {
                                (String::new(), "Oops, lyrics not found :(".into())
                            }
                            Err(_) => (
                                String::new(),
                                "Couldn't fetch lyrics. Please try again later.".into(),
                            ),
                        };
                        let _ = events.send(Event::Lyrics(revision, text, status, refresh));
                    }
                } else {
                    let text = backend.lyrics(&path).unwrap_or_default();
                    // An empty local result must not overwrite the network
                    // worker's final status if it finishes first.
                    if !text.is_empty()
                        && !stop.load(Ordering::Relaxed)
                        && revision == generation.load(Ordering::Relaxed)
                    {
                        let _ = events.send(Event::Lyrics(
                            revision,
                            text.clone(),
                            String::new(),
                            false,
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
