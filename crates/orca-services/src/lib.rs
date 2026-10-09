//! Rust-native application service. The frontend owns UI state; this crate owns
//! database access and joined scan/audio lifecycles; orca-core owns media work.
mod artwork;
mod bootstrap;
mod catalog;
mod folders;
mod media;
pub mod operation_types;
mod operations;
mod playback;
mod scanning;
pub mod types;
pub use bootstrap::{import_library, new_backend, prepare_data_dir};
pub use orca_core::atomic_file::write as write_state_file;
// Historical DTO alias; no foreign-function boundary is involved.
pub use types as ffi;
#[cfg(test)]
mod tests;

use orca_core::audio_engine::{AudioCommand, PlaybackState};
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
};
#[derive(Default)]
struct ScanState {
    active: AtomicBool,
    cancel: AtomicBool,
    count: AtomicU64,
    revision: AtomicU64,
    error: Mutex<String>,
}

pub struct Backend {
    conn: Connection,
    data_dir: PathBuf,
    scan: Arc<ScanState>,
    scan_thread: Option<JoinHandle<()>>,
    audio_tx: Option<mpsc::Sender<AudioCommand>>,
    playback: Arc<Mutex<PlaybackState>>,
    audio_thread: Option<JoinHandle<()>>,
    ended: Arc<AtomicU64>,
    transitioned: Arc<AtomicU64>,
}

impl Backend {
    pub fn operation(&self, request: &str) -> Result<String, String> {
        let request = serde_json::from_str(request)
            .map_err(|error| format!("Invalid library operation: {error}"))?;
        let request = operation_types::OperationRequest::from_value(request)
            .map_err(|error| error.to_string())?;
        let result = self.execute(&request).map_err(|error| error.to_string())?;
        if matches!(
            result,
            operation_types::OperationResult::CollectionEdited
                | operation_types::OperationResult::CoverUpdated(None)
        ) {
            return Ok("{}".into());
        }
        serde_json::to_string(&result).map_err(|error| error.to_string())
    }
    pub fn execute(
        &self,
        request: &operation_types::OperationRequest,
    ) -> Result<operation_types::OperationResult, operation_types::OperationError> {
        self.execute_with_stop(request, None)
    }
    pub fn execute_cancellable(
        &self,
        request: &operation_types::OperationRequest,
        stop: &AtomicBool,
    ) -> Result<operation_types::OperationResult, operation_types::OperationError> {
        self.execute_with_stop(request, Some(stop))
    }
    fn execute_with_stop(
        &self,
        request: &operation_types::OperationRequest,
        stop: Option<&AtomicBool>,
    ) -> Result<operation_types::OperationResult, operation_types::OperationError> {
        request.validate()?;
        if stop.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err("Operation cancelled".to_string().into());
        }
        operations::run_with_stop(self, request, stop).map_err(Into::into)
    }
    pub fn shutdown(&mut self) {
        self.cancel_scan();
        if let Some(tx) = self.audio_tx.take() {
            let _ = tx.send(AudioCommand::Shutdown);
        }
        if let Some(handle) = self.scan_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.audio_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.shutdown();
    }
}
