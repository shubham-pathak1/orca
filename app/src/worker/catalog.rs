//! Catalog requests retain their generation and send headers before song pages.
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
        self.generation = at;
        self.query = new_query;
        self.kind = kind.clone();
        if !self.query.kind.is_empty() {
            let exists = if self.query.kind == "folders" {
                backend.folder_exists(&self.query.key)?
            } else {
                backend
                    .groups(&self.query.kind, "")?
                    .iter()
                    .any(|g| g.key == self.query.key && g.secondary == self.query.secondary)
            };
            if !exists {
                let _ = events.send(Event::GroupGone(
                    self.query.kind.clone(),
                    self.query.key.clone(),
                    self.query.secondary.clone(),
                ));
                return Ok(());
            }
        }
        if kind == "folders" {
            let groups = backend.folder_groups(&self.query.key, &self.query.search)?;
            let _ = events.send(Event::Groups(at, groups));
        }
        if kind == "songs" || !self.query.kind.is_empty() {
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
                let result = backend
                    .execute(&request)
                    .map_err(|error| error.to_string())?;
                let _ = events.send(Event::Operation(request, result));
            }
            let page = backend.page(&self.query, 0, 128)?;
            let _ = events.send(Event::Page(at, 0, page.total, page.tracks));
        } else if kind != "settings" && kind != "folders" {
            let groups = backend.groups(&kind, &self.query.search)?;
            let _ = events.send(Event::Groups(at, groups));
        }
        let _ = events.send(Event::Statistics(backend.statistics()?));
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
        let page = backend.page(&self.query, offset, 128)?;
        let _ = events.send(Event::Page(at, offset, page.total, page.tracks));
        Ok(())
    }
}
