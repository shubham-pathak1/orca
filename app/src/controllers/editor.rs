//! Song metadata editor callbacks. Validation stays in the metadata domain helper.
use crate::{
    metadata,
    protocol::{MetadataAssetKind, MetadataRequest, Request},
    AppState, OrcaWindow,
};
use orca_services::operation_types::{CollectionKind, CoverFetch, LyricsFetch, OperationRequest};
use slint::ComponentHandle;
use std::sync::mpsc::Sender;

pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    let weak = ui.as_weak();
    ui.global::<AppState>()
        .on_metadata_action(move |action, value| {
            if let Some(ui) = weak.upgrade() {
                handle_action(&ui, &tx, action.as_str(), value.as_str());
            }
        });
}

fn handle_action(ui: &OrcaWindow, tx: &Sender<Request>, action: &str, value: &str) {
    let state = ui.global::<AppState>();
    if state.get_editor_busy() && !matches!(action, "load" | "load-fetch-cover") {
        return;
    }
    let draft = state.get_draft();
    match action {
        "save" => match metadata::edit(&draft) {
            Ok(value) => {
                state.set_editor_busy(true);
                state.set_editor_operation("save".into());
                if tx
                    .send(Request::Metadata(MetadataRequest::Save {
                        edit: value,
                        generation: state.get_editor_generation(),
                    }))
                    .is_err()
                {
                    state.set_editor_busy(false);
                    state.set_editor_status("Please restart Orca and try again.".into());
                }
            }
            Err(error) => state.set_editor_status(error.into()),
        },
        "load" | "load-fetch-cover" => {
            state.set_editor_generation(state.get_editor_generation().wrapping_add(1));
            state.set_editor_status("".into());
            let _ = tx.send(Request::Metadata(MetadataRequest::Load {
                path: value.into(),
                fetch_cover: action == "load-fetch-cover",
                generation: state.get_editor_generation(),
            }));
        }
        "fetch-cover" | "fetch-lyrics" => {
            state.set_editor_busy(true);
            state.set_editor_operation(action.into());
            state.set_editor_status(
                if action == "fetch-cover" {
                    "Searching artwork providers…"
                } else {
                    "Fetching lyrics…"
                }
                .into(),
            );
            let fetch = if action == "fetch-cover" {
                OperationRequest::FetchCover(CoverFetch {
                    kind: CollectionKind::Albums,
                    key: format!("{}:{}", draft.album_artist, draft.album),
                    artist: metadata::artwork_album_artist(&draft),
                    album: draft.album.to_string(),
                    title: Some(draft.title.to_string()),
                    track_artist: Some(draft.artist.to_string()),
                    duration_ms: None,
                    editor_generation: Some(state.get_editor_generation()),
                    editor_path: Some(draft.path.to_string()),
                    collection_draft: false,
                    collection_seed: String::new(),
                })
            } else {
                OperationRequest::FetchLyrics(LyricsFetch {
                    key: draft.path.to_string(),
                    title: Some(draft.title.to_string()),
                    artist: Some(draft.artist.to_string()),
                    album: Some(draft.album.to_string()),
                    cache: false,
                    editor_path: Some(draft.path.to_string()),
                    editor_generation: Some(state.get_editor_generation()),
                })
            };
            if tx.send(Request::Operation(fetch)).is_err() {
                state.set_editor_busy(false);
                state.set_editor_status("Please restart Orca and try again :(".into());
            }
        }
        "remove-cover" => {
            let mut draft = draft;
            draft.remove_cover = true;
            draft.artwork = "".into();
            draft.cover_to_embed = "".into();
            state.set_draft(draft);
            state.set_editor_cover(slint::Image::default());
            state.set_editor_cover_missing(true);
            state.set_editor_status("Cover removed - Save metadata to apply it".into());
        }
        "pick-cover" | "pick-lyrics" => {
            let tx = tx.clone();
            let cover = action == "pick-cover";
            let target = draft.path.to_string();
            let generation = state.get_editor_generation();
            std::thread::spawn(move || {
                let dialog = rfd::FileDialog::new();
                let dialog = if cover {
                    dialog.add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                } else {
                    dialog.add_filter("Lyrics", &["lrc", "txt"])
                };
                if let Some(path) = dialog.pick_file() {
                    let _ = tx.send(Request::Metadata(MetadataRequest::SelectAsset {
                        kind: if cover {
                            MetadataAssetKind::Cover
                        } else {
                            MetadataAssetKind::Lyrics
                        },
                        target,
                        file: path.to_string_lossy().into(),
                        generation,
                    }));
                }
            });
        }
        _ => {}
    }
}
