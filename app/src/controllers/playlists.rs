//! Playlist editor/import/export callbacks; service owns database changes.
use crate::{
    protocol::{PlaylistEdit, Request},
    OrcaWindow,
};
use orca_services::operation_types::OperationRequest;
use slint::{ComponentHandle, Model};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::mpsc::Sender,
};
pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    let selected = Rc::new(RefCell::new(HashSet::<i32>::new()));
    let selection_anchor = Rc::new(Cell::new(None::<i32>));
    let drag_base = Rc::new(RefCell::new(HashSet::<i32>::new()));
    {
        let selected = selected.clone();
        ui.library()
            .on_playlist_song_selected(move |song_id, _revision| {
                selected.borrow().contains(&song_id)
            });
    }
    {
        let selected = selected.clone();
        let selection_anchor = selection_anchor.clone();
        let drag_base = drag_base.clone();
        let weak = ui.as_weak();
        let tx = tx.clone();
        ui.library().on_playlist_selection(move |action, song_id| {
            let Some(ui) = weak.upgrade() else { return };

            match action.as_str() {
                "start"
                    if ui.shell().get_view() == "playlists"
                        && !ui.library().get_detail_key().is_empty() =>
                {
                    selected.borrow_mut().clear();
                    drag_base.borrow_mut().clear();
                    selection_anchor.set(None);
                    ui.library().set_playlist_selection_count(0);
                    ui.library().set_playlist_selection_mode(true);
                }
                "toggle" if ui.library().get_playlist_selection_mode() && song_id > 0 => {
                    let count = {
                        let mut chosen = selected.borrow_mut();
                        if !chosen.insert(song_id) {
                            chosen.remove(&song_id);
                        }
                        chosen.len() as i32
                    };
                    ui.library().set_playlist_selection_count(count);
                }
                "anchor" if ui.library().get_playlist_selection_mode() && song_id >= 0 => {
                    selection_anchor.set(Some(song_id));
                }
                "range" | "drag-to"
                    if ui.library().get_playlist_selection_mode() && song_id >= 0 =>
                {
                    let songs = ui.library().get_songs();
                    let anchor = selection_anchor.get().unwrap_or(song_id);
                    let upper = songs.row_count().saturating_sub(1) as i32;
                    let start = anchor.min(song_id).clamp(0, upper);
                    let end = anchor.max(song_id).clamp(0, upper);
                    let mut chosen = if action == "drag-to" {
                        drag_base.borrow().clone()
                    } else {
                        selected.borrow().clone()
                    };
                    for index in start..=end {
                        if let Some(song) = songs.row_data(index as usize) {
                            if song.id > 0 {
                                chosen.insert(song.id);
                            }
                        }
                    }
                    let count = chosen.len() as i32;
                    *selected.borrow_mut() = chosen;
                    ui.library().set_playlist_selection_count(count);
                }
                "drag-start" if ui.library().get_playlist_selection_mode() && song_id >= 0 => {
                    selection_anchor.set(Some(song_id));
                    *drag_base.borrow_mut() = selected.borrow().clone();
                }
                "remove" if ui.library().get_playlist_selection_mode() => {
                    let song_ids: Vec<i64> =
                        selected.borrow().iter().map(|id| *id as i64).collect();
                    if !song_ids.is_empty() {
                        let id = if ui.library().get_detail_key() == "liked" {
                            None
                        } else {
                            match ui.library().get_detail_key().parse::<i64>() {
                                Ok(id) if id > 0 => Some(id),
                                _ => return,
                            }
                        };
                        let _ =
                            tx.send(Request::Playlist(PlaylistEdit::RemoveMany { id, song_ids }));
                    }
                    selected.borrow_mut().clear();
                    drag_base.borrow_mut().clear();
                    selection_anchor.set(None);
                    ui.library().set_playlist_selection_count(0);
                    ui.library().set_playlist_selection_mode(false);
                }
                "cancel" => {
                    selected.borrow_mut().clear();
                    drag_base.borrow_mut().clear();
                    selection_anchor.set(None);
                    ui.library().set_playlist_selection_count(0);
                    ui.library().set_playlist_selection_mode(false);
                }
                _ => return,
            }
            ui.library().set_playlist_selection_revision(
                ui.library()
                    .get_playlist_selection_revision()
                    .wrapping_add(1),
            );
        });
    }
    {
        let weak = ui.as_weak();
        let tx = tx.clone();
        ui.player().on_toggle_like(move || {
            if let Some(ui) = weak.upgrade() {
                let song_id = ui.player().get_now().id;
                if song_id > 0 {
                    let _ = tx.send(Request::ToggleLiked(song_id as i64));
                }
            }
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.dialogs().on_playlist_action(move |action, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            match action.as_str() {
                "import" | "export" => {
                    let tx = tx.clone();
                    let export = action == "export";
                    let id = ui.library().get_detail_key().parse::<i64>().unwrap_or(0);
                    std::thread::spawn(move || {
                        let dialog =
                            rfd::FileDialog::new().add_filter("Playlists", &["m3u", "m3u8"]);
                        let path = if export {
                            dialog.set_file_name("playlist.m3u8").save_file()
                        } else {
                            dialog.pick_file()
                        };
                        if let Some(path) = path {
                            let file = path.to_string_lossy().into_owned();
                            let request = if export {
                                OperationRequest::ExportPlaylist { id, file }
                            } else {
                                OperationRequest::ImportPlaylist { file }
                            };
                            let _ = tx.send(Request::Operation(request));
                        }
                    });
                }
                "new-dialog" | "rename-dialog" => {
                    let rename = action == "rename-dialog";
                    ui.dialogs()
                        .set_playlist_edit_action(if rename { "rename" } else { "create" }.into());
                    ui.dialogs().set_playlist_name(if rename {
                        ui.library().get_detail_title()
                    } else {
                        "".into()
                    });
                    ui.dialogs().set_playlist_dialog(true);
                }
                "open-current" => {
                    let song_id = ui.player().get_now().id;
                    if song_id > 0 {
                        ui.dialogs().set_playlist_picker_loading(true);
                        ui.dialogs().set_playlist_choices(Default::default());
                        ui.dialogs().set_playlist_picker_open(true);
                        let _ = tx.send(Request::PlaylistChoices(song_id as i64));
                    }
                }
                "create" | "rename" => {
                    let edit = if action == "create" {
                        PlaylistEdit::Create { name: value.into() }
                    } else {
                        PlaylistEdit::Rename {
                            id: ui.library().get_detail_key().parse().unwrap_or(0),
                            name: value.into(),
                        }
                    };
                    let _ = tx.send(Request::Playlist(edit));
                    if action == "create"
                        && ui.dialogs().get_playlist_picker_open()
                        && ui.player().get_now().id > 0
                    {
                        ui.dialogs().set_playlist_picker_loading(true);
                        let _ = tx.send(Request::PlaylistChoices(ui.player().get_now().id as i64));
                    }
                }
                "delete" => {
                    let _ = tx.send(Request::Playlist(PlaylistEdit::Delete {
                        id: value.parse().unwrap_or(0),
                    }));
                }
                "remove-menu" => {
                    let song_id = ui.dialogs().get_menu_song().id;
                    if song_id <= 0
                        || ui.shell().get_view() != "playlists"
                        || ui.library().get_detail_key().is_empty()
                    {
                        return;
                    }
                    let id = if ui.library().get_detail_key() == "liked" {
                        None
                    } else {
                        match ui.library().get_detail_key().parse::<i64>() {
                            Ok(id) if id > 0 => Some(id),
                            _ => return,
                        }
                    };
                    let _ = tx.send(Request::Playlist(PlaylistEdit::RemoveMany {
                        id,
                        song_ids: vec![song_id as i64],
                    }));
                }
                "add" | "add-current" | "remove-current" => {
                    let song_id = if action == "add-current" || action == "remove-current" {
                        ui.player().get_now().id
                    } else {
                        ui.dialogs().get_menu_song().id
                    };
                    if song_id <= 0 {
                        return;
                    }
                    let id = value.parse().unwrap_or(0);
                    let edit = if action == "remove-current" {
                        PlaylistEdit::Remove {
                            id,
                            song_id: song_id as i64,
                        }
                    } else {
                        PlaylistEdit::Add {
                            id,
                            song_id: song_id as i64,
                        }
                    };
                    let _ = tx.send(Request::Playlist(edit));
                    if action != "add" {
                        ui.dialogs().set_playlist_picker_loading(true);
                        let _ = tx.send(Request::PlaylistChoices(song_id as i64));
                    }
                }
                _ => {}
            }
        });
    }
}
