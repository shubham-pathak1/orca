//! Bind player actions to the UI-owned playback session and worker messages.
use crate::{
    controllers::input::{command_request, play_request},
    lyrics, models,
    playback::state::{PlayerModels, SeekPreview},
    protocol::Request,
    OrcaWindow,
};
use slint::ComponentHandle;
use std::{rc::Rc, sync::mpsc::Sender, time::Instant};
pub(crate) fn install(ui: &OrcaWindow, tx: Sender<Request>, player: Rc<PlayerModels>) {
    ui.player()
        .on_compact_quality(|quality| models::quality_label(&quality).into());
    ui.player().on_split_title(crate::presentation::split_title);
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.player().on_play(move |path| {
            if let Some(ui) = weak.upgrade() {
                if ui.library().get_catalog_loading() {
                    return;
                }
                let _ = tx.send(play_request(&ui, path.into()));
            }
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.player().on_command(move |action, value| {
            if let Some(ui) = weak.upgrade() {
                match command_request(&ui, action.into(), value as f64) {
                    Ok(request) => {
                        let _ = tx.send(request);
                    }
                    Err(error) => ui.shell().set_error(error.into()),
                }
            }
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        let player = player.clone();
        ui.player().on_seek(move |target| {
            if !target.is_finite() {
                return;
            }
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let duration = ui.player().get_duration();
            let target = target.clamp(
                0.0,
                if duration.is_finite() && duration > 0.0 {
                    duration
                } else {
                    86_400_000.0
                },
            );
            {
                *player.preview.borrow_mut() = Some(SeekPreview {
                    target,
                    path: ui.player().get_now().path.into(),
                    started: Instant::now(),
                });
                ui.player().set_position(target);
                ui.lyrics().set_lyric_position(target);
                ui.lyrics()
                    .set_active_line(lyrics::active(&player.lines.borrow(), target));
                ui.player()
                    .set_elapsed(models::duration(target.max(0.0) as u64).into());
            }
            let _ = tx.send(Request::Seek(target as f64));
        });
    }
    {
        let tx = tx.clone();
        let weak = ui.as_weak();
        ui.player().on_volume_changed(move |value| {
            if !value.is_finite() {
                return;
            }
            let value = value.clamp(0.0, 1.0);
            if let Some(ui) = weak.upgrade() {
                if ui.player().get_output_unprocessed() {
                    return;
                }
                ui.player().set_volume(value.clamp(0.0, 1.0));
            }
            let _ = tx.send(Request::Volume(value as f64));
        });
    }
    {
        let tx = tx.clone();
        ui.player().on_queue_action(move |action, source, target| {
            if let Ok(action) =
                crate::protocol::QueueAction::parse(&action, source.into(), target.into())
            {
                let _ = tx.send(Request::Queue(action));
            }
        });
    }
    {
        let tx = tx.clone();
        ui.lyrics().on_refresh_lyrics(move || {
            let _ = tx.send(Request::RefreshLyrics);
        });
    }
    {
        let player = player.clone();
        ui.lyrics()
            .on_lyric_render(move |index, _| player.lyric_renderer.borrow_mut().get_signed(index));
    }
    ui.lyrics().on_lyric_offset(move |index, _| {
        player.lyric_renderer.borrow().offset(index.max(0) as usize)
    });
}
