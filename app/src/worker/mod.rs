//! Own the application worker threads, typed channels and joined shutdown.
mod background;
mod catalog;
mod library;
mod metadata;
mod playback;
mod state;
use crate::jobs::{latest_channel, LatestJob};
use crate::protocol::Settings;
pub use crate::protocol::{Event, Request};
#[cfg(test)]
use crate::{
    navigation::Navigation,
    persistence::Session,
    playback_flow::{cancel_preload, submit_play, PlaybackErrors},
    protocol::PlayerAction,
};
#[cfg(test)]
use orca_services::{new_backend, operation_types::OperationResult, types::Query};
use orca_services::{operation_types::OperationRequest, types::Group};
#[cfg(test)]
use std::{
    fs,
    time::{Duration, Instant},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

// Catalog navigation uses collection titles, including playlists whose display
// order can differ from alphabetical song order.
fn catalog_letter_index(groups: &[Group], letter: &str) -> u32 {
    if letter == "#" {
        return 0;
    }
    let letter = letter.to_uppercase();
    let titles: Vec<_> = groups.iter().map(|g| g.title.to_uppercase()).collect();
    titles
        .iter()
        .position(|title| title.starts_with(&letter))
        .or_else(|| {
            titles
                .iter()
                .enumerate()
                .filter(|(_, title)| title.as_str() >= letter.as_str())
                .min_by(|a, b| a.1.cmp(b.1))
                .map(|(index, _)| index)
        })
        .unwrap_or(groups.len().saturating_sub(1)) as u32
}

fn shutdown_state(requests: &Receiver<Request>, mut volume: f32) -> (f32, Option<Settings>) {
    let mut settings = None;
    while let Ok(request) = requests.try_recv() {
        match request {
            Request::Volume(value) if value.is_finite() => volume = value.clamp(0.0, 1.0) as f32,
            Request::Settings(value) => settings = Some(value),
            _ => {}
        }
    }
    (volume, settings)
}
pub struct AppWorker {
    pub romanization_generation: Arc<AtomicU64>,
    romanization_jobs: LatestJob<(u64, Vec<String>, orca_services::types::RomanizationPolicy)>,
    bulk_cancel: Arc<AtomicBool>,
    pub(crate) artwork_demand:
        crate::jobs::LatestJob<orca_services::operation_types::ArtworkDemand>,
    pub tx: Sender<Request>,
    pub rx: Receiver<Event>,
    pub latest_query: Arc<AtomicU64>,
    pub analysis_generation: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
}
impl AppWorker {
    #[cfg(test)]
    pub(crate) fn test_channels() -> (Self, Sender<Event>, Receiver<Request>) {
        let (tx, requests) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        (
            Self {
                romanization_generation: Arc::new(AtomicU64::new(0)),
                romanization_jobs: latest_channel().0,
                bulk_cancel: Arc::new(AtomicBool::new(true)),
                artwork_demand: latest_channel().0,
                tx,
                rx,
                latest_query: Arc::new(AtomicU64::new(0)),
                analysis_generation: Arc::new(AtomicU64::new(0)),
                stop: Arc::new(AtomicBool::new(false)),
                handles: Vec::new(),
            },
            events,
            requests,
        )
    }
    pub fn start(directory: PathBuf, audio: bool) -> Self {
        let romanization_generation = Arc::new(AtomicU64::new(0));
        let (romanization_jobs, romanization_rx) = latest_channel();
        let bulk_cancel = Arc::new(AtomicBool::new(true));
        let (artwork_demand, artwork_rx) = latest_channel();
        let (tx, requests) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let (analysis_jobs, analysis_rx) = latest_channel::<(u64, String, bool)>();
        let (lyrics_jobs, lyrics_rx) = latest_channel::<(u64, String, bool)>();
        let (operations, operation_rx) = mpsc::sync_channel::<OperationRequest>(8);
        let stop = Arc::new(AtomicBool::new(false));
        let auto_artwork = Arc::new(AtomicBool::new(false));
        let analysis_generation = Arc::new(AtomicU64::new(0));
        let latest_query = Arc::new(AtomicU64::new(0));
        let mut handles = background::spawn_background_workers(background::BackgroundWorkers {
            romanization_generation: romanization_generation.clone(),
            romanization_rx,
            bulk_cancel: bulk_cancel.clone(),
            artwork_rx,
            directory: directory.clone(),
            stop: stop.clone(),
            events: events.clone(),
            auto_artwork: auto_artwork.clone(),
            analysis_generation: analysis_generation.clone(),
            operation_rx,
            analysis_rx,
            lyrics_rx,
        });
        let worker_stop = stop.clone();
        let revision = analysis_generation.clone();
        let query_revision = latest_query.clone();
        handles.push(thread::spawn(move || {
            library::run_library_loop(library::LibraryWorker {
                directory,
                audio,
                worker_stop,
                revision,
                query_revision,
                auto_artwork,
                events,
                requests,
                operations,
                analysis_jobs,
                lyrics_jobs,
            })
        }));

        Self {
            romanization_generation,
            romanization_jobs,
            bulk_cancel,
            artwork_demand,
            tx,
            rx,
            latest_query,
            analysis_generation,
            stop,
            handles,
        }
    }
    pub(crate) fn romanize(
        &self,
        lines: Vec<String>,
        policy: orca_services::types::RomanizationPolicy,
    ) {
        let generation = self
            .romanization_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        // An empty job cancels both queued and currently running conversion.
        self.romanization_jobs.submit((generation, lines, policy));
    }
    pub(crate) fn artwork_bulk(&self, action: orca_services::operation_types::ArtworkBulkAction) {
        use orca_services::operation_types::ArtworkBulkAction;
        if matches!(action, ArtworkBulkAction::Pause) {
            self.bulk_cancel.store(true, Ordering::Relaxed);
        }
        let _ = self
            .tx
            .send(Request::Operation(OperationRequest::ArtworkBulk(action)));
    }
}
impl Drop for AppWorker {
    fn drop(&mut self) {
        self.bulk_cancel.store(true, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
        self.romanization_jobs.wake();
        let _ = self.tx.send(Request::Shutdown);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}
#[cfg(test)]
mod tests;
