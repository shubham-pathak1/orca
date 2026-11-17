//! Song metadata editor callbacks. Validation stays in the metadata domain helper.
use crate::{
    metadata,
    protocol::{MetadataAssetKind, MetadataRequest, Request},
    OrcaWindow,
};
use orca_services::operation_types::{CollectionKind, CoverFetch, LyricsFetch, OperationRequest};
use slint::ComponentHandle;
use std::sync::mpsc::Sender;

pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    let weak = ui.as_weak();
    ui.dialogs().on_metadata_action(move |action, value| {
        if let Some(ui) = weak.upgrade() {
            handle_action(&ui, &tx, action.as_str(), value.as_str());
        }
    });
}

fn handle_action(ui: &OrcaWindow, tx: &Sender<Request>, action: &str, value: &str) {
    if ui.dialogs().get_editor_busy() && !matches!(action, "load" | "load-fetch-cover") {
        return;
    }
    let draft = ui.dialogs().get_draft();
    match action {
        "save" => match metadata::edit(&draft) {
            Ok(value) => {
                ui.dialogs().set_editor_busy(true);
                ui.dialogs().set_editor_operation("save".into());
                if tx
                    .send(Request::Metadata(MetadataRequest::Save {
                        edit: Box::new(value),
                        generation: ui.dialogs().get_editor_generation(),
                    }))
                    .is_err()
                {
                    ui.dialogs().set_editor_busy(false);
                    ui.dialogs()
                        .set_editor_status("Please restart Orca and try again.".into());
                }
            }
            Err(error) => ui.dialogs().set_editor_status(error.into()),
        },
        "load" | "load-fetch-cover" => {
            ui.dialogs()
                .set_editor_generation(ui.dialogs().get_editor_generation().wrapping_add(1));
            ui.dialogs().set_editor_status("".into());
            let _ = tx.send(Request::Metadata(MetadataRequest::Load {
                path: value.into(),
                fetch_cover: action == "load-fetch-cover",
                generation: ui.dialogs().get_editor_generation(),
            }));
        }
        "fetch-cover" | "fetch-lyrics" => {
            ui.dialogs().set_editor_busy(true);
            ui.dialogs().set_editor_operation(action.into());
            ui.dialogs().set_editor_status(
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
                    editor_generation: Some(ui.dialogs().get_editor_generation()),
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
                    editor_generation: Some(ui.dialogs().get_editor_generation()),
                })
            };
            if tx.send(Request::Operation(fetch)).is_err() {
                ui.dialogs().set_editor_busy(false);
                ui.dialogs()
                    .set_editor_status("Please restart Orca and try again.".into());
            }
        }
        "remove-cover" => {
            let mut draft = draft;
            draft.remove_cover = true;
            draft.artwork = "".into();
            draft.cover_to_embed = "".into();
            ui.dialogs().set_draft(draft);
            ui.dialogs().set_editor_cover(slint::Image::default());
            ui.dialogs().set_editor_cover_missing(true);
            ui.dialogs()
                .set_editor_status("Cover removed - Save metadata to apply it".into());
        }
        "pick-cover" | "pick-lyrics" => {
            let tx = tx.clone();
            let cover = action == "pick-cover";
            let target = draft.path.to_string();
            let generation = ui.dialogs().get_editor_generation();
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
