//! Collection editor callbacks and draft identity checks for late async results.
use crate::{protocol::Request, AppState, OrcaWindow};
use orca_services::operation_types::{
    CollectionEdit, CollectionKind, CollectionPreview, CoverFetch, OperationRequest,
};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::sync::mpsc::Sender;

pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    let weak = ui.as_weak();
    ui.global::<AppState>().on_collection_menu({
        let weak = weak.clone();
        move |group, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let state = ui.global::<AppState>();
            state.set_menu_open(false);
            state.set_collection_kind(state.get_view());
            state.set_collection_menu_x(x);
            state.set_collection_menu_y(y);
            state.set_collection_request(
                json!({
                    "action": "collection-edit", "kind": state.get_view().as_str(),
                    "key": group.key.as_str(), "secondary": group.secondary.as_str()
                })
                .to_string()
                .into(),
            );
            state.set_collection_group(group);
            state.set_collection_menu_open(true);
        }
    });
    ui.global::<AppState>().on_collection_action(move |action| {
        if let Some(ui) = weak.upgrade() {
            handle_action(&ui, &tx, action.as_str());
        }
    });
}

fn handle_action(ui: &OrcaWindow, tx: &Sender<Request>, action: &str) {
    let state = ui.global::<AppState>();
    let staged = state.get_collection_dialog();
    if state.get_collection_saving() || (staged && state.get_collection_fetching()) {
        return;
    }
    let from_menu = state.get_collection_menu_open();
    let mut request = if staged || from_menu {
        serde_json::from_str::<Value>(&state.get_collection_request()).unwrap_or_default()
    } else {
        json!({"action": "collection-edit", "kind": state.get_view().as_str(),
            "key": state.get_detail_key().as_str(), "secondary": state.get_detail_secondary().as_str()})
    };
    let edit = match OperationRequest::from_value(request.clone()) {
        Ok(OperationRequest::CollectionEdit(edit)) => edit,
        _ => {
            state.set_error(
                "Oops, this collection is no longer available. Please open it again.".into(),
            );
            return;
        }
    };
    state.set_collection_menu_open(false);
    if action == "open-location" && edit.identity.kind == CollectionKind::Folders {
        #[cfg(windows)]
        if let Err(error) = std::process::Command::new("explorer.exe")
            .arg(super::preferences::clean_folder_path(
                &edit.identity.key.replace('/', "\\"),
            ))
            .spawn()
        {
            state.set_error(format!("Could not open folder: {error}").into());
        }
        return;
    }
    match action {
        "name" => {
            state.set_collection_saving(false);
            state.set_collection_fetching(false);
            state.set_collection_status("".into());
            state.set_collection_path(
                super::preferences::clean_folder_path(&edit.identity.key.replace('/', "\\")).into(),
            );
            request["draft_id"] = json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string());
            state.set_collection_kind(request["kind"].as_str().unwrap_or("").into());
            state.set_collection_name(if from_menu {
                state.get_collection_group().title
            } else {
                state.get_detail_title()
            });
            state.set_collection_cover(if from_menu {
                state.get_collection_group().cover
            } else {
                state.get_detail_cover()
            });
            state.set_collection_cover_missing(if from_menu {
                state.get_collection_group().cover_missing
            } else {
                state.get_detail_cover_missing()
            });
            state.set_collection_request(request.to_string().into());
            state.set_collection_dialog(true);
            let preview = OperationRequest::CollectionPreview(CollectionPreview {
                identity: edit.identity,
                collection_seed: request.to_string(),
            });
            let _ = tx.send(Request::Operation(preview));
        }
        "save-name" => {
            let name = state.get_collection_name();
            let name = name.trim();
            if name.is_empty() && edit.identity.kind != CollectionKind::Folders {
                state.set_error("Please enter a name :)".into());
                return;
            }
            // The runtime closes this exact draft only after successful persistence.
            let edit = CollectionEdit {
                name: (edit.identity.kind != CollectionKind::Folders).then(|| name.into()),
                ..edit
            };
            state.set_collection_saving(true);
            if tx
                .send(Request::Operation(OperationRequest::CollectionEdit(edit)))
                .is_err()
            {
                state.set_collection_saving(false);
                state.set_error(
                    "Orca couldn't save this edit. Please restart and try again.".into(),
                );
            }
        }
        "remove-cover" => {
            request["cover"] = json!("");
            if staged {
                state.set_collection_request(request.to_string().into());
                state.set_collection_cover(slint::Image::default());
                state.set_collection_cover_missing(true);
            } else {
                let edit = CollectionEdit {
                    cover: Some(String::new()),
                    ..edit
                };
                let _ = tx.send(Request::Operation(OperationRequest::CollectionEdit(edit)));
            }
        }
        "cover" => {
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                    .pick_file()
                {
                    request["cover"] = json!(file.to_string_lossy());
                    let _ = tx.send(if staged {
                        Request::CollectionDraft(request.to_string())
                    } else {
                        Request::Operation(OperationRequest::CollectionEdit(CollectionEdit {
                            cover: Some(file.to_string_lossy().into_owned()),
                            ..edit
                        }))
                    });
                }
            });
        }
        "fetch-cover" => {
            if staged {
                state.set_collection_fetching(true);
                state.set_collection_status("Searching for artwork...".into());
            }
            let identity = edit.identity;
            let album = identity.kind == CollectionKind::Albums;
            let fetch = OperationRequest::FetchCover(CoverFetch {
                kind: identity.kind,
                key: if album {
                    format!("{}:{}", identity.secondary, identity.key)
                } else {
                    identity.key.clone()
                },
                artist: identity.secondary,
                album: if album { identity.key } else { String::new() },
                title: None,
                track_artist: None,
                duration_ms: None,
                editor_generation: None,
                editor_path: None,
                collection_draft: staged,
                collection_seed: state.get_collection_request().to_string(),
            });
            if tx.send(Request::Operation(fetch)).is_err() {
                state.set_collection_fetching(false);
                state.set_collection_status(
                    "Artwork search couldn't start. Please restart Orca.".into(),
                );
            }
        }
        _ => {}
    }
}
pub(crate) fn collection_seed_matches(seed: &serde_json::Value, current: &str) -> bool {
    let Some(seed) = seed.as_str() else {
        return false;
    };
    let (Ok(mut seed), Ok(mut current)) = (
        serde_json::from_str::<serde_json::Value>(seed),
        serde_json::from_str::<serde_json::Value>(current),
    ) else {
        return false;
    };
    // Loading a preview is read-only and must not invalidate a pending fetch.
    for value in [&mut seed, &mut current] {
        let Some(object) = value.as_object_mut() else {
            return false;
        };
        object.remove("preview");
    }
    seed == current
}

pub(crate) fn finish_collection_fetch(ui: &OrcaWindow, fetch: &CoverFetch, message: &str) -> bool {
    let state = ui.global::<AppState>();
    if !fetch.collection_draft
        || !state.get_collection_dialog()
        || !collection_seed_matches(
            &Value::String(fetch.collection_seed.clone()),
            &state.get_collection_request(),
        )
    {
        return false;
    }
    state.set_collection_fetching(false);
    state.set_collection_status(message.into());
    true
}

pub(crate) fn stage_collection_draft(ui: &OrcaWindow, draft: &serde_json::Value) -> bool {
    let s = ui.global::<AppState>();
    if !s.get_collection_dialog() {
        return false;
    }
    let current =
        serde_json::from_str::<serde_json::Value>(&s.get_collection_request()).unwrap_or_default();
    if ["kind", "key", "secondary", "draft_id"]
        .iter()
        .any(|field| current[*field] != draft[*field])
    {
        return false;
    }
    s.set_collection_request(draft.to_string().into());
    if draft["cover"].as_str() == Some("") {
        s.set_collection_cover(slint::Image::default());
        s.set_collection_cover_missing(true);
    }
    true
}
