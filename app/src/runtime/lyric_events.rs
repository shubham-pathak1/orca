//! Apply accepted lyric results without disturbing manual edits or refresh fallbacks.
use super::RuntimeModels;
use crate::presentation::waveform;
use crate::{lyrics, OrcaWindow};
use slint::VecModel;
use std::rc::Rc;

fn install_text(ui: &OrcaWindow, models: &RuntimeModels, text: &str) {
    let player = &models.player;
    if ui.dialogs().get_metadata_open()
        && ui.dialogs().get_draft().path.as_str() == player.now.borrow().path
        && ui.dialogs().get_draft().lyrics.trim().is_empty()
        && ui
            .dialogs()
            .get_editor_status()
            .starts_with("Could not connect")
    {
        let mut draft = ui.dialogs().get_draft();
        draft.lyrics = orca_services::original_lyrics(text).into();
        ui.dialogs().set_draft(draft);
        ui.dialogs()
            .set_editor_status("Lyrics ready - Save metadata to embed them".into());
    }
    let parsed = lyrics::parse(text);
    ui.lyrics()
        .set_lyrics(Rc::new(VecModel::from(parsed.clone())).into());
    *player.lines.borrow_mut() = parsed;
    player.lyric_renderer.borrow_mut().invalidate();
}
pub(super) fn apply_analysis(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    text: String,
    peaks: Vec<f32>,
) {
    if !text.is_empty() && models.player.lines.borrow().is_empty() {
        install_text(ui, models, &text);
        ui.lyrics().set_lyrics_status("".into());
    }
    ui.player().set_waveform(waveform(&peaks));
    ui.lyrics().set_active_line(lyrics::active(
        &models.player.lines.borrow(),
        ui.player().get_position(),
    ));
}
pub(super) fn apply_lyrics(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    text: String,
    status: String,
    refresh: bool,
) {
    if refresh {
        ui.lyrics().set_lyrics_refreshing(false);
        if text.is_empty() && !models.player.lines.borrow().is_empty() {
            ui.shell().set_error(
                if status == "Lyrics not found." {
                    "No matching lyrics found. Keeping the current lyrics."
                } else {
                    "Couldn't refresh lyrics. Keeping the current lyrics."
                }
                .into(),
            );
        }
    }
    if !text.is_empty() {
        install_text(ui, models, &text);
        ui.lyrics().set_active_line(lyrics::active(
            &models.player.lines.borrow(),
            ui.player().get_position(),
        ));
    }
    ui.lyrics().set_lyrics_status(status.into());
}
