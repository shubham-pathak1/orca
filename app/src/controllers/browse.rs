//! Convert the current UI selection into a backend query.
use crate::{AppState, OrcaWindow};
use orca_services::types::Query;
use slint::ComponentHandle;
pub(crate) fn query(ui: &OrcaWindow) -> (Query, String) {
    let state = ui.global::<AppState>();
    let view = state.get_view().to_string();
    (
        Query {
            search: state.get_search().into(),
            sort: state.get_sort().into(),
            kind: if state.get_detail_key().is_empty()
                || !matches!(
                    view.as_str(),
                    "artists" | "albums" | "genres" | "playlists" | "folders"
                ) {
                String::new()
            } else {
                view.clone()
            },
            key: state.get_detail_key().into(),
            secondary: state.get_detail_secondary().into(),
        },
        view,
    )
}
