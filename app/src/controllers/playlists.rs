//! Playlist editor/import/export callbacks; service owns database changes.
use crate::{
    protocol::{PlaylistEdit, Request},
    AppState, OrcaWindow,
};
use orca_services::operation_types::OperationRequest;
use slint::ComponentHandle;
use std::sync::mpsc::Sender;
pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>) {
    let state = ui.global::<AppState>();
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        state.on_playlist_action(move |action, value| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let s = ui.global::<AppState>();
            match action.as_str() {
                "import" | "export" => {
                    let tx = tx.clone();
                    let export = action == "export";
                    let id = s.get_detail_key().parse::<i64>().unwrap_or(0);
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
                    s.set_playlist_edit_action(if rename { "rename" } else { "create" }.into());
                    s.set_playlist_name(if rename {
                        s.get_detail_title()
                    } else {
                        "".into()
                    });
                    s.set_playlist_dialog(true);
                }
                "create" | "rename" => {
                    let edit = if action == "create" {
                        PlaylistEdit::Create { name: value.into() }
                    } else {
                        PlaylistEdit::Rename {
                            id: s.get_detail_key().parse().unwrap_or(0),
                            name: value.into(),
                        }
                    };
                    let _ = tx.send(Request::Playlist(edit));
                }
                "delete" => {
                    let _ = tx.send(Request::Playlist(PlaylistEdit::Delete {
                        id: value.parse().unwrap_or(0),
                    }));
                }
                "add" => {
                    let _ = tx.send(Request::Playlist(PlaylistEdit::Add {
                        id: value.parse().unwrap_or(0),
                        song_id: s.get_menu_song().id as i64,
                    }));
                }
                _ => {}
            }
        });
    }
}
