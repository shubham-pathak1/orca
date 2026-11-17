//! Collection editor callbacks and draft identity checks for late async results.
use crate::{protocol::Request, OrcaWindow};
use orca_services::operation_types::{
    CollectionEdit, CollectionKind, CollectionPreview, CoverFetch, OperationRequest,
};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::sync::mpsc::Sender;

pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    ui.library().on_artist_names(|credits, title| {
        std::rc::Rc::new(slint::VecModel::from(artist_links(&credits, &title))).into()
    });
    ui.library().on_open_collection({
        let weak = ui.as_weak();
        move |kind, key, secondary| {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            if key.is_empty()
                || ui.shell().get_focus_layer() != "browse"
                || !matches!(kind.as_str(), "artists" | "albums" | "years" | "composers")
            {
                return;
            }
            ui.shell().set_view(kind);
            ui.library().invoke_open_group(key.clone(), secondary, key);
        }
    });
    ui.player().on_like_song({
        let tx = tx.clone();
        move |id| {
            if id > 0 {
                let _ = tx.send(Request::ToggleLiked(id as i64));
            }
        }
    });
    ui.player().on_play_group({
        let weak = ui.as_weak();
        let tx = tx.clone();
        move |group, kind| {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            if group.key.is_empty()
                || ui.shell().get_focus_layer() != "browse"
                || !matches!(
                    kind.as_str(),
                    "albums" | "genres" | "artists" | "playlists" | "composers" | "years"
                )
            {
                return;
            }
            let _ = tx.send(Request::Collection(
                crate::protocol::CollectionAction::Toggle,
                orca_services::types::Query {
                    kind: kind.into(),
                    key: group.key.into(),
                    secondary: group.secondary.into(),
                    ..Default::default()
                },
            ));
        }
    });
    let weak = ui.as_weak();
    ui.dialogs().on_collection_menu({
        let weak = weak.clone();
        move |group, x, y| {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            ui.dialogs().set_menu_open(false);
            if matches!(ui.shell().get_view().as_str(), "composers" | "years") {
                return;
            }
            ui.dialogs().set_collection_kind(ui.shell().get_view());
            ui.dialogs().set_collection_menu_x(x);
            ui.dialogs().set_collection_menu_y(y);
            ui.dialogs().set_collection_request(
                json!({
                    "action": "collection-edit", "kind": ui.shell().get_view().as_str(),
                    "key": group.key.as_str(), "secondary": group.secondary.as_str()
                })
                .to_string()
                .into(),
            );
            ui.dialogs().set_collection_group(group);
            ui.dialogs().set_collection_menu_open(true);
        }
    });
    ui.dialogs().on_collection_action(move |action| {
        if let Some(ui) = weak.upgrade() {
            handle_action(&ui, &tx, action.as_str());
        }
    });
}

fn handle_action(ui: &OrcaWindow, tx: &Sender<Request>, action: &str) {
    let staged = ui.dialogs().get_collection_dialog();
    if ui.dialogs().get_collection_saving() || (staged && ui.dialogs().get_collection_fetching()) {
        return;
    }
    let from_menu = ui.dialogs().get_collection_menu_open();
    let mut request = if staged || from_menu {
        serde_json::from_str::<Value>(&ui.dialogs().get_collection_request()).unwrap_or_default()
    } else {
        json!({"action": "collection-edit", "kind": ui.shell().get_view().as_str(),
            "key": ui.library().get_detail_key().as_str(), "secondary": ui.library().get_detail_secondary().as_str()})
    };
    let edit = match OperationRequest::from_value(request.clone()) {
        Ok(OperationRequest::CollectionEdit(edit)) => edit,
        _ => {
            ui.shell()
                .set_error("This collection is no longer available. Please open it again.".into());
            return;
        }
    };
    ui.dialogs().set_collection_menu_open(false);
    if action == "open-location" && edit.identity.kind == CollectionKind::Folders {
        #[cfg(windows)]
        if let Err(error) = std::process::Command::new("explorer.exe")
            .arg(super::preferences::clean_folder_path(
                &edit.identity.key.replace('/', "\\"),
            ))
            .spawn()
        {
            ui.shell()
                .set_error(format!("Could not open folder: {error}").into());
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = std::process::Command::new("xdg-open")
            .arg(&edit.identity.key)
            .spawn()
        {
            ui.shell()
                .set_error(format!("Could not open folder: {error}").into());
        }
        return;
    }
    match action {
        "name" => {
            ui.dialogs().set_collection_saving(false);
            ui.dialogs().set_collection_fetching(false);
            ui.dialogs().set_collection_status("".into());
            ui.player().set_collection_path(
                super::preferences::clean_folder_path(&edit.identity.key.replace('/', "\\")).into(),
            );
            request["draft_id"] = json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string());
            ui.dialogs()
                .set_collection_kind(request["kind"].as_str().unwrap_or("").into());
            ui.dialogs().set_collection_name(if from_menu {
                ui.dialogs().get_collection_group().title
            } else {
                ui.library().get_detail_title()
            });
            ui.dialogs().set_collection_cover(if from_menu {
                ui.dialogs().get_collection_group().cover
            } else {
                ui.library().get_detail_cover()
            });
            ui.dialogs().set_collection_cover_missing(if from_menu {
                ui.dialogs().get_collection_group().cover_missing
            } else {
                ui.library().get_detail_cover_missing()
            });
            ui.dialogs()
                .set_collection_request(request.to_string().into());
            ui.dialogs().set_collection_dialog(true);
            let preview = OperationRequest::CollectionPreview(CollectionPreview {
                identity: edit.identity,
                collection_seed: request.to_string(),
            });
            let _ = tx.send(Request::Operation(preview));
        }
        "save-name" => {
            let name = ui.dialogs().get_collection_name();
            let name = name.trim();
            if name.is_empty() && edit.identity.kind != CollectionKind::Folders {
                ui.shell().set_error("Please enter a name :)".into());
                return;
            }
            // The runtime closes this exact draft only after successful persistence.
            let edit = CollectionEdit {
                name: (edit.identity.kind != CollectionKind::Folders).then(|| name.into()),
                ..edit
            };
            ui.dialogs().set_collection_saving(true);
            if tx
                .send(Request::Operation(OperationRequest::CollectionEdit(edit)))
                .is_err()
            {
                ui.dialogs().set_collection_saving(false);
                ui.shell().set_error(
                    "Orca couldn't save this edit. Please restart and try again.".into(),
                );
            }
        }
        "remove-cover" => {
            request["cover"] = json!("");
            if staged {
                ui.dialogs()
                    .set_collection_request(request.to_string().into());
                ui.dialogs().set_collection_cover(slint::Image::default());
                ui.dialogs().set_collection_cover_missing(true);
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
                ui.dialogs().set_collection_fetching(true);
                ui.dialogs()
                    .set_collection_status("Searching for artwork...".into());
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
                collection_seed: ui.dialogs().get_collection_request().to_string(),
            });
            if tx.send(Request::Operation(fetch)).is_err() {
                ui.dialogs().set_collection_fetching(false);
                ui.dialogs().set_collection_status(
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
    if !fetch.collection_draft
        || !ui.dialogs().get_collection_dialog()
        || !collection_seed_matches(
            &Value::String(fetch.collection_seed.clone()),
            &ui.dialogs().get_collection_request(),
        )
    {
        return false;
    }
    ui.dialogs().set_collection_fetching(false);
    ui.dialogs().set_collection_status(message.into());
    true
}

pub(crate) fn stage_collection_draft(ui: &OrcaWindow, draft: &serde_json::Value) -> bool {
    if !ui.dialogs().get_collection_dialog() {
        return false;
    }
    let current = serde_json::from_str::<serde_json::Value>(&ui.dialogs().get_collection_request())
        .unwrap_or_default();
    if ["kind", "key", "secondary", "draft_id"]
        .iter()
        .any(|field| current[*field] != draft[*field])
    {
        return false;
    }
    ui.dialogs()
        .set_collection_request(draft.to_string().into());
    if draft["cover"].as_str() == Some("") {
        ui.dialogs().set_collection_cover(slint::Image::default());
        ui.dialogs().set_collection_cover_missing(true);
    }
    true
}

// Preserve the original credit, including featured guests and punctuation. Only
// names with artist-browser membership become navigation targets.
fn artist_links(credits: &str, title: &str) -> Vec<crate::ArtistCredit> {
    let mut links = Vec::new();
    let mut cursor = 0;
    for name in orca_services::primary_artist_names(credits, title) {
        let Some(relative) = credits[cursor..].find(&name) else {
            continue;
        };
        let start = cursor + relative;
        if start > cursor {
            links.push(crate::ArtistCredit {
                text: credits[cursor..start].into(),
                key: "".into(),
            });
        }
        cursor = start + name.len();
        links.push(crate::ArtistCredit {
            text: name.clone().into(),
            key: name.into(),
        });
    }
    if cursor < credits.len() {
        links.push(crate::ArtistCredit {
            text: credits[cursor..].into(),
            key: "".into(),
        });
    }
    links
}

#[cfg(test)]
mod link_tests {
    #[test]
    fn artist_links_preserve_original_credits_and_do_not_link_featured_guests() {
        for (credits, title, expected) in [
            (" Band ; Guest ", "Song", vec!["Band", "Guest"]),
            ("Band feat. Guest", "Song", vec!["Band"]),
            ("Band; Guest", "Song (feat. Guest)", vec!["Band"]),
            ("Earth, Wind & Fire", "Song", vec!["Earth", "Wind & Fire"]),
            (
                "Grover Washington, Jr.",
                "Song",
                vec!["Grover Washington, Jr."],
            ),
        ] {
            let links = super::artist_links(credits, title);
            assert_eq!(
                links
                    .iter()
                    .map(|link| link.text.as_str())
                    .collect::<String>(),
                credits
            );
            assert_eq!(
                links
                    .iter()
                    .filter(|link| !link.key.is_empty())
                    .map(|link| link.key.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}
