//! Present typed playback and lyric results after the router checks their generation.
use super::RuntimeModels;
use crate::presentation::reset_empty_player;
use crate::{errors, lyrics, models, protocol::Request, LyricLine, OrcaTray, OrcaWindow};
use orca_services::types::{Snapshot, Track};
use slint::VecModel;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
pub(super) fn apply_snapshot(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    tray: &Option<slint::Weak<OrcaTray>>,
    snapshot: Snapshot,
) {
    let player = &models.player;
    let crate::playback::state::PlayerModels {
        lines,
        preview,
        playback_clock,
        ..
    } = player.as_ref();
    let output = &snapshot.output;
    ui.player()
        .set_output_device_name(output.device_name.clone().into());
    ui.player()
        .set_output_sample_format(output.sample_format.clone().into());
    ui.player().set_output_rate(output.sample_rate as i32);
    ui.player()
        .set_output_source_rate(output.source_sample_rate as i32);
    ui.player().set_output_channels(output.channels as i32);
    ui.player().set_output_active_exclusive(output.exclusive);
    ui.player()
        .set_output_active_unprocessed(output.unprocessed);
    ui.player().set_output_switching(output.switching);
    if ui.player().get_output_notice() != output.notice {
        ui.player().set_output_notice(output.notice.clone().into());
    }
    let output_error = if errors::is_unsupported_source_format(&output.error) {
        errors::friendly(&output.error)
    } else {
        output.error.clone()
    };
    if ui.player().get_output_error() != output_error {
        ui.player().set_output_error(output_error.into());
    }
    if let Some(tray) = tray.as_ref().and_then(slint::Weak::upgrade) {
        tray.set_playing(snapshot.playing);
    }
    let mut seek = preview.borrow_mut();
    if seek.as_ref().is_some_and(|p| {
        p.path != snapshot.path
            || p.started.elapsed() > Duration::from_secs(2)
            || (snapshot.position_ms as f32 - p.target).abs() < 750.0
            || !snapshot.error.is_empty()
    }) {
        *seek = None;
    }
    let position = seek
        .as_ref()
        .map_or(snapshot.position_ms as f32, |p| p.target);
    ui.player().set_position(position);
    ui.lyrics().set_lyric_position(position);
    playback_clock.borrow_mut().update(
        position,
        snapshot.duration_ms as f32,
        snapshot.playing,
        Instant::now(),
    );
    ui.player()
        .set_elapsed(models::duration(position.max(0.0) as u64).into());
    ui.player().set_duration(snapshot.duration_ms as f32);
    ui.player()
        .set_total(models::duration(snapshot.duration_ms).into());
    ui.player().set_playing(snapshot.playing);
    if snapshot.playing
        && !snapshot.path.is_empty()
        && snapshot.playback_error.is_empty()
        && output.error.is_empty()
        && !output.switching
        && output.sample_rate > 0
    {
        if !ui.player().get_audio_error_banner().is_empty()
            && ui.shell().get_error() == ui.player().get_audio_error_banner()
        {
            ui.shell().set_error("".into());
        }
        ui.player().set_audio_error_banner("".into());
    }
    ui.player().set_volume(snapshot.volume);
    ui.lyrics()
        .set_active_line(lyrics::active(&lines.borrow(), position));
    if !snapshot.error.is_empty() {
        ui.shell()
            .set_error(errors::friendly(&snapshot.error).into());
    }
}

pub(super) fn apply_track(
    ui: &OrcaWindow,
    models: &RuntimeModels,
    neighbor_revision: &Cell<u64>,
    track: Track,
) -> bool {
    let player = &models.player;
    let crate::playback::state::PlayerModels {
        now,
        lines,
        preview,
        lyric_renderer,
        neighbors,
        ..
    } = player.as_ref();
    let poll_service = &models.worker;
    ui.lyrics().set_lyrics_refreshing(false);
    if *now.borrow() == track {
        return false;
    }
    let changed_song = now.borrow().path != track.path;
    if !changed_song {
        // A collection/tag/artwork refresh is not a new song. Keep
        // the current lyric layout, focus and neighbor prefetch.
        *now.borrow_mut() = track;
        return true;
    }
    neighbors.borrow_mut().clear();
    ui.player().set_liked(false);
    if track.id > 0 {
        let _ = poll_service.tx.send(Request::LikedStatus(track.id));
    }
    neighbor_revision.set(neighbor_revision.get().wrapping_add(1));
    if track.path.is_empty() {
        reset_empty_player(ui);
        *preview.borrow_mut() = None;
    }
    *now.borrow_mut() = track;
    lines.borrow_mut().clear();
    lyric_renderer.borrow_mut().invalidate();
    ui.lyrics()
        .set_lyrics(Rc::new(VecModel::<LyricLine>::default()).into());
    ui.lyrics().set_active_line(-1);
    ui.lyrics().set_lyrics_status(
        if now.borrow().path.is_empty() {
            ""
        } else {
            "Just a sec, fetching lyrics…"
        }
        .into(),
    );
    true
}
