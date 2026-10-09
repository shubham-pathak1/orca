//! Service-owned state; unrelated catalog changes cannot mutate playback plans.
use crate::{navigation::Navigation, playback_flow::PlaybackErrors};
use orca_services::types::Query;
#[derive(Default)]
pub(super) struct PlaybackState {
    pub navigation: Navigation,
    pub path: String,
    pub requested: String,
    pub preloaded: String,
    pub planned_navigation: Option<Navigation>,
    pub ended: u64,
    pub transitioned: u64,
    pub output_revision: u64,
    pub errors: PlaybackErrors,
    pub stopping_removed: String,
}
pub(super) struct CatalogState {
    pub query: Query,
    pub generation: u64,
    pub kind: String,
}
impl Default for CatalogState {
    fn default() -> Self {
        Self {
            query: Query::default(),
            generation: 0,
            kind: "songs".into(),
        }
    }
}
