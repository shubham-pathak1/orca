//! Transactional navigation/preload decisions and deduplicated audio failures.
use crate::navigation::Navigation;
use orca_services::types::Snapshot;
// Commit queue/history only after the backend accepts the file and command.
pub(crate) fn submit_play(
    navigation: &mut Navigation,
    requested: &mut String,
    mut planned: Navigation,
    target: String,
    play: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    play(&target)?;
    planned.record(&target);
    *navigation = planned;
    *requested = target;
    Ok(())
}

pub(crate) fn cancel_preload(
    preloaded: &mut String,
    planned: &mut Option<Navigation>,
    clear: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !preloaded.is_empty() {
        clear()?;
        preloaded.clear();
    }
    *planned = None;
    Ok(())
}

#[derive(Default)]
pub(crate) struct PlaybackErrors {
    revision: u64,
    output_error: String,
}
impl PlaybackErrors {
    pub(crate) fn collect(&mut self, snapshot: &Snapshot) -> Vec<String> {
        let mut errors = Vec::new();
        if snapshot.playback_error_revision != self.revision {
            self.revision = snapshot.playback_error_revision;
            if !snapshot.playback_error.is_empty() {
                errors.push(snapshot.playback_error.clone());
            }
        }
        let error = &snapshot.output.error;
        if error != &self.output_error {
            self.output_error.clone_from(error);
            if !error.is_empty() {
                let message = format!("output:{error}");
                if !errors.contains(&message) {
                    errors.push(message);
                }
            }
        }
        errors
    }
}
