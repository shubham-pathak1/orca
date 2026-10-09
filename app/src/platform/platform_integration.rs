//! Window/tray/Phantom integration. No media or catalog policy belongs here.
use crate::{worker::Request, AppState, OrcaWindow};
use slint::ComponentHandle;
pub(crate) fn set_background_playback(ui: &OrcaWindow, hidden: bool) {
    let state = ui.global::<AppState>();
    if hidden && !state.get_tray_available() {
        return;
    }
    let result = if hidden { ui.hide() } else { ui.show() };
    match result {
        Ok(()) => {
            state.set_window_hidden(hidden);
            if !hidden {
                ui.window().set_minimized(false);
                ui.invoke_focus_shell();
            }
        }
        Err(error) => state.set_error(error.to_string().into()),
    }
}
pub(crate) fn toggle_background_playback(ui: &OrcaWindow) {
    set_background_playback(ui, !ui.global::<AppState>().get_window_hidden());
}
pub(crate) fn dispatch_tray_action(
    ui: &OrcaWindow,
    tx: &std::sync::mpsc::Sender<Request>,
    action: &str,
) {
    match action {
        "restore" => set_background_playback(ui, false),
        "toggle" | "previous" | "next" => {
            if let Ok(action) = crate::protocol::PlayerAction::parse(action) {
                let _ = tx.send(Request::Command(action));
            }
        }
        _ => {}
    }
}
