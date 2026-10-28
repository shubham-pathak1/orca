//! Explicit whole-library lookup with a durable cursor and bounded work steps.
use crate::{
    operation_types::{
        ArtworkBulkAction, ArtworkBulkProgress, CollectionIdentity, CollectionKind, OperationResult,
    },
    Backend,
};
use orca_core::db;
use std::sync::atomic::{AtomicBool, Ordering};

impl Backend {
    pub fn artwork_bulk_progress(&self) -> Result<ArtworkBulkProgress, String> {
        db::get_setting(&self.conn, "artwork_bulk_cursor_v1").map_or_else(
            || Ok(ArtworkBulkProgress::default()),
            |raw| serde_json::from_str(&raw).map_err(|error| error.to_string()),
        )
    }
    fn save_artwork_bulk(&self, progress: &ArtworkBulkProgress) -> Result<(), String> {
        db::set_setting(
            &self.conn,
            "artwork_bulk_cursor_v1",
            &serde_json::to_string(progress).map_err(|error| error.to_string())?,
        )
    }
    pub fn artwork_bulk_control(
        &self,
        action: &ArtworkBulkAction,
    ) -> Result<ArtworkBulkProgress, String> {
        let mut progress = self.artwork_bulk_progress()?;
        if matches!(action, ArtworkBulkAction::Start) {
            db::refresh_artist_index(&self.conn)?;
            progress = ArtworkBulkProgress {
                started: true,
                ..Default::default()
            };
            self.save_artwork_bulk(&progress)?;
        }
        Ok(progress)
    }
    pub fn artwork_bulk_step(
        &self,
        stop: &AtomicBool,
    ) -> Result<(ArtworkBulkProgress, bool), String> {
        let mut progress = self.artwork_bulk_progress()?;
        let mut changed = false;
        if !progress.started || progress.complete {
            return Ok((progress, false));
        }
        if !orca_core::online_artwork::retry_after().is_zero() {
            progress.waiting = true;
            progress.retry_after_ms = orca_core::online_artwork::retry_after().as_millis() as u64;
            return Ok((progress, false));
        }
        // Skip already-covered collections in short slices so manual commands
        // and current/visible artwork can interrupt a very large bulk request.
        for _ in 0..64 {
            if stop.load(Ordering::Relaxed) {
                return Ok((progress, changed));
            }
            let Some((key, secondary)) = db::next_artwork_collection(
                &self.conn,
                progress.phase == 0,
                &progress.key,
                &progress.secondary,
            )?
            else {
                if progress.phase == 0 {
                    progress.phase = 1;
                    progress.key.clear();
                    progress.secondary.clear();
                    continue;
                }
                progress.complete = true;
                break;
            };
            let identity = CollectionIdentity {
                kind: if progress.phase == 0 {
                    CollectionKind::Artists
                } else {
                    CollectionKind::Albums
                },
                key: key.clone(),
                secondary: secondary.clone(),
            };
            let job = self.artwork_jobs_for_collections([identity])?.pop();
            if let Some(job) = job {
                let result = self.execute_cancellable(&job, stop);
                // An interrupted lookup remains at the same cursor for Resume.
                if stop.load(Ordering::Relaxed) {
                    return Ok((progress, changed));
                }
                if result.is_err() && !orca_core::online_artwork::retry_after().is_zero() {
                    progress.waiting = true;
                    progress.retry_after_ms =
                        orca_core::online_artwork::retry_after().as_millis() as u64;
                    self.save_artwork_bulk(&progress)?;
                    return Ok((progress, changed));
                }
                progress.attempted += 1;
                changed = matches!(result, Ok(OperationResult::Artwork(_)));
                progress.found += u64::from(changed);
                progress.key = key;
                progress.secondary = secondary;
                break;
            }
            progress.key = key;
            progress.secondary = secondary;
        }
        self.save_artwork_bulk(&progress)?;
        Ok((progress, changed))
    }
}
