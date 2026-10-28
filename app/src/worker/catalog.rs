//! Tag catalog results by query generation and load the first page before navigation IDs.
use super::state::CatalogState;
use crate::protocol::Event;
use orca_services::Backend;
use orca_services::{
    operation_types::{OperationError, OperationRequest},
    types::Query,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::Sender,
};
impl CatalogState {
    pub(super) fn refresh_artwork(
        &self,
        backend: &Backend,
        events: &Sender<Event>,
        at: u64,
        offsets: &[u32],
        latest: &AtomicU64,
    ) -> Result<(), String> {
        if at != self.generation || at != latest.load(Ordering::Relaxed) {
            return Ok(());
        }
        if self.kind == "folders" {
            let _ = events.send(Event::Groups(
                at,
                backend.folder_groups(&self.query.key, &self.query.search)?,
            ));
        } else if self.query.kind.is_empty() && self.kind != "songs" && self.kind != "settings" {
            let _ = events.send(Event::Groups(
                at,
                backend.groups(&self.kind, &self.query.search)?,
            ));
        }
        if self.kind == "songs" || !self.query.kind.is_empty() {
            for offset in offsets.iter().take(8) {
                self.page(backend, events, at, *offset, latest)?;
            }
        }
        if !self.query.kind.is_empty() {
            let request = if self.query.kind == "artists" {
                OperationRequest::ArtistDetail {
                    key: self.query.key.clone(),
                    secondary: self.query.secondary.clone(),
                }
            } else {
                OperationRequest::GroupDetail(orca_services::operation_types::CollectionIdentity {
                    kind: self
                        .query
                        .kind
                        .as_str()
                        .try_into()
                        .map_err(|error: OperationError| error.to_string())?,
                    key: self.query.key.clone(),
                    secondary: self.query.secondary.clone(),
                })
            };
            let result = backend
                .execute(&request)
                .map_err(|error| error.to_string())?;
            if at == latest.load(Ordering::Relaxed) {
                let _ = events.send(Event::CatalogDetail(at, request, result));
            }
        }
        Ok(())
    }
    pub(super) fn browse(
        &mut self,
        backend: &Backend,
        events: &Sender<Event>,
        at: u64,
        new_query: Query,
        kind: String,
        latest: &AtomicU64,
    ) -> Result<(), String> {
        if at != latest.load(Ordering::Relaxed) {
            return Ok(());
        }
        let previous_count = if self.query.kind == new_query.kind
            && self.query.key == new_query.key
            && self.query.secondary == new_query.secondary
            && self.query.sort == new_query.sort
            && self.kind == kind
        {
            self.ids.len() as u32
        } else {
            0
        };
        self.generation = at;
        self.query = new_query;
        self.kind = kind.clone();
        self.ids = Vec::new();
        if !self.query.kind.is_empty() {
            let exists =
                backend.group_exists(&self.query.kind, &self.query.key, &self.query.secondary)?;
            if !exists {
                let _ = events.send(Event::GroupGone(
                    at,
                    self.query.kind.clone(),
                    self.query.key.clone(),
                    self.query.secondary.clone(),
                ));
                return Ok(());
            }
        }
        if at != latest.load(Ordering::Relaxed) {
            return Ok(());
        }
        if kind == "folders" && self.query.kind.is_empty() {
            let groups = backend.folder_groups(&self.query.key, &self.query.search)?;
            let _ = events.send(Event::Groups(at, groups));
        }
        if kind == "songs" || !self.query.kind.is_empty() {
            let preview = backend.page_tracks(&self.query, 0, 128)?;
            if at != latest.load(Ordering::Relaxed) {
                return Ok(());
            }
            // Expose the first screen immediately; expand the scroll extent
            // once the complete compact navigation order is available.
            let _ = events.send(Event::Page(
                at,
                0,
                if preview.is_empty() {
                    0
                } else {
                    previous_count.max(preview.len() as u32)
                },
                preview,
            ));
            if kind == "folders" && !self.query.kind.is_empty() {
                let groups = backend.folder_groups(&self.query.key, &self.query.search)?;
                if at != latest.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let _ = events.send(Event::Groups(at, groups));
            }
            self.ids = backend.context_ids(&self.query)?;
            if at != latest.load(Ordering::Relaxed) {
                return Ok(());
            }
            self.page(backend, events, at, 0, latest)?;
            if !self.query.kind.is_empty() {
                // Local header data must not wait behind network artwork jobs.
                let request = if self.query.kind == "artists" {
                    OperationRequest::ArtistDetail {
                        key: self.query.key.clone(),
                        secondary: self.query.secondary.clone(),
                    }
                } else {
                    OperationRequest::GroupDetail(
                        orca_services::operation_types::CollectionIdentity {
                            kind: self
                                .query
                                .kind
                                .as_str()
                                .try_into()
                                .map_err(|error: OperationError| error.to_string())?,
                            key: self.query.key.clone(),
                            secondary: self.query.secondary.clone(),
                        },
                    )
                };
                let result = match backend.execute(&request) {
                    Ok(result) => result,
                    Err(OperationError::Failed(error))
                        if self.query.kind == "artists" && error == "Artist not found" =>
                    {
                        let _ = events.send(Event::GroupGone(
                            at,
                            self.query.kind.clone(),
                            self.query.key.clone(),
                            self.query.secondary.clone(),
                        ));
                        return Ok(());
                    }
                    Err(error) => return Err(error.to_string()),
                };
                if at != latest.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let _ = events.send(Event::CatalogDetail(at, request, result));
            }
        } else if kind != "settings" && kind != "folders" {
            let groups = backend.groups(&kind, &self.query.search)?;
            let _ = events.send(Event::Groups(at, groups));
        }
        Ok(())
    }
    pub(super) fn page(
        &self,
        backend: &Backend,
        events: &Sender<Event>,
        at: u64,
        offset: u32,
        latest: &AtomicU64,
    ) -> Result<(), String> {
        if at != self.generation || at != latest.load(Ordering::Relaxed) {
            return Ok(());
        }
        let start = (offset as usize).min(self.ids.len());
        let end = start.saturating_add(128).min(self.ids.len());
        let tracks = backend.tracks_by_ids(&self.ids[start..end])?;
        let _ = events.send(Event::Page(at, offset, self.ids.len() as u32, tracks));
        Ok(())
    }
}
