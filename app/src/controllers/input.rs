//! Playback keyboard bindings and command capture. Editing widgets retain text input.
use super::browse::query;
use crate::{
    protocol::{CollectionAction, PlayerAction, Request},
    AppState, OrcaWindow,
};
use slint::ComponentHandle;
use std::{cell::Cell, time::Duration};
pub(crate) fn play_request(ui: &OrcaWindow, path: String) -> Request {
    Request::Play(path, false, query(ui).0)
}
pub(crate) fn command_request(
    ui: &OrcaWindow,
    action: String,
    _value: f64,
) -> Result<Request, String> {
    Ok(match action.as_str() {
        "play-context" => Request::Collection(CollectionAction::Play, query(ui).0),
        "queue-context" => Request::Collection(CollectionAction::AddToQueue, query(ui).0),
        _ => Request::Command(PlayerAction::parse(&action)?),
    })
}

pub(crate) fn install_keyboard(ui: &OrcaWindow, tx: std::sync::mpsc::Sender<Request>) {
    let state = ui.global::<AppState>();
    let weak = ui.as_weak();
    let fullscreen = Cell::new(false);
    let last_volume = Cell::new(1.0f32);
    state.on_keyboard(move |key, alt| {
        let Some(ui) = weak.upgrade() else {
            return false;
        };
        let s = ui.global::<AppState>();
        let escape: slint::SharedString = slint::platform::Key::Escape.into();
        let f11: slint::SharedString = slint::platform::Key::F11.into();
        if key == escape {
            if s.get_metadata_open() {
                s.set_metadata_open(false);
            } else if s.get_collection_dialog() {
                s.set_collection_dialog(false);
            } else if s.get_playlist_dialog() {
                s.set_playlist_dialog(false);
            } else if s.get_menu_open() {
                s.set_menu_open(false);
            } else if s.get_collection_menu_open() {
                s.set_collection_menu_open(false);
            } else if s.get_queue_open() {
                s.set_queue_open(false);
            } else if s.get_full_player() {
                s.set_full_player(false);
                ui.invoke_focus_shell();
            } else {
                return false;
            }
            ui.invoke_focus_shell();
            return true;
        }
        if s.get_metadata_open() || s.get_playlist_dialog() || s.get_collection_dialog() {
            return false;
        }
        if key == f11 {
            fullscreen.set(!fullscreen.get());
            ui.window().set_fullscreen(fullscreen.get());
            return true;
        }
        match key.to_lowercase().as_str() {
            " " => {
                let _ = tx.send(Request::Command(PlayerAction::Toggle));
            }
            "n" if alt => {
                let _ = tx.send(Request::Command(PlayerAction::Next));
            }
            "p" if alt => {
                let _ = tx.send(Request::Command(PlayerAction::Previous));
            }
            "m" if !alt => {
                let volume = s.get_volume();
                if volume > 0.0 {
                    last_volume.set(volume);
                }
                let volume = if volume == 0.0 {
                    last_volume.get()
                } else {
                    0.0
                };
                s.set_volume(volume);
                let _ = tx.send(Request::Volume(volume as f64));
            }
            "l" if !alt => {
                if s.get_full_player() {
                    s.set_lyrics_open(!s.get_lyrics_open());
                } else {
                    s.set_queue_open(false);
                    s.set_full_player(true);
                    s.set_lyrics_open(true);
                    ui.invoke_focus_shell();
                }
            }
            "q" if !alt => {
                s.set_queue_open(!s.get_queue_open());
                if s.get_queue_open() {
                    let _ = tx.send(Request::Command(PlayerAction::QueuePreview));
                }
                ui.invoke_focus_shell();
            }
            "focus-search" => {
                if !s.get_full_player()
                    && (s.get_search_input_focused() || s.get_search_focus_pending())
                {
                    ui.invoke_focus_shell();
                    return true;
                }
                s.set_search_focus_pending(true);
                s.set_full_player(false);
                s.set_queue_open(false);
                s.set_menu_open(false);
                let weak = ui.as_weak();
                slint::Timer::single_shot(Duration::from_millis(1), move || {
                    if let Some(ui) = weak.upgrade() {
                        let s = ui.global::<AppState>();
                        if s.get_search_focus_pending() {
                            s.set_search_focus_request(
                                s.get_search_focus_request().wrapping_add(1),
                            );
                        }
                    }
                });
            }
            _ => return false,
        }
        true
    });
}
