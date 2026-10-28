//! Playback mutation policy lives with navigation and pending gapless plans.
use super::state::PlaybackState;
use crate::{
    playback_flow::{cancel_preload, submit_play},
    protocol::{CollectionAction, Event, PlayerAction, QueueAction},
};
use orca_services::types::{PlaybackCommand, Query};
use orca_services::Backend;
use std::sync::mpsc::Sender;
impl PlaybackState {
    pub(super) fn play(
        &mut self,
        backend: &Backend,
        audio: bool,
        target: String,
        from_queue: bool,
        play_query: Query,
    ) -> Result<(), String> {
        if target.is_empty() || !audio {
            return Ok(());
        }
        let mut planned = self.navigation.clone();
        if !from_queue {
            let ids = backend.context_ids(&play_query)?;
            let id = backend
                .id_for_path(&target)?
                .ok_or("Song is no longer in the library")?;
            planned.compact_context(ids, play_query, id, &target);
        }
        let was_manual = planned.manual.iter().any(|path| path == &target);
        submit_play(
            &mut self.navigation,
            &mut self.requested,
            planned,
            target,
            |target| backend.playback_command(PlaybackCommand::Play((target).into())),
        )?;
        if let Some(id) = backend.id_for_path(&self.requested)? {
            self.navigation.mark_id(id, was_manual);
        }
        self.preloaded.clear();
        self.planned_navigation = None;
        Ok(())
    }
    pub(super) fn collection(
        &mut self,
        backend: &Backend,
        audio: bool,
        action: CollectionAction,
        collection: Query,
    ) -> Result<(), String> {
        match action {
            CollectionAction::Play if audio => {
                let ids = backend.context_ids(&collection)?;
                if let Some(id) = ids.first().copied() {
                    let Some(target) = backend.path_for_id(id)? else {
                        return Ok(());
                    };
                    let mut planned = self.navigation.clone();
                    planned.compact_context(ids, collection, id, &target);
                    submit_play(
                        &mut self.navigation,
                        &mut self.requested,
                        planned,
                        target,
                        |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                    )?;
                    self.navigation.mark_id(id, false);
                    self.preloaded.clear();
                    self.planned_navigation = None;
                }
            }
            CollectionAction::AddToQueue => {
                let mut all = collection;
                all.search.clear();
                let targets = backend.context_paths_limited(&all, 64)?;
                cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
                    if audio {
                        backend.playback_command(PlaybackCommand::ClearQueued)
                    } else {
                        Ok(())
                    }
                })?;
                for target in targets {
                    if self.navigation.manual.len() >= 20 {
                        break;
                    }
                    self.navigation.removed.remove(&target);
                    if target != self.path && !self.navigation.manual.contains(&target) {
                        self.navigation.manual.push(target);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn command(
        &mut self,
        backend: &Backend,
        events: &Sender<Event>,
        audio: bool,
        action: PlayerAction,
    ) -> Result<(), String> {
        match action {
            PlayerAction::Toggle if audio => {
                let snapshot = backend.snapshot()?;
                backend.playback_command(if snapshot.playing {
                    PlaybackCommand::Pause
                } else {
                    PlaybackCommand::Resume
                })?;
            }
            PlayerAction::Shuffle => {
                cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
                    if audio {
                        backend.playback_command(PlaybackCommand::ClearQueued)
                    } else {
                        Ok(())
                    }
                })?;
                self.navigation.shuffle = !self.navigation.shuffle;
                let _ = events.send(Event::Modes(
                    self.navigation.shuffle,
                    self.navigation.repeat,
                ));
            }
            PlayerAction::Repeat => {
                cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
                    if audio {
                        backend.playback_command(PlaybackCommand::ClearQueued)
                    } else {
                        Ok(())
                    }
                })?;
                self.navigation.repeat = (self.navigation.repeat + 1) % 3;
                let _ = events.send(Event::Modes(
                    self.navigation.shuffle,
                    self.navigation.repeat,
                ));
            }
            action @ (PlayerAction::Next | PlayerAction::Previous) if audio => {
                let current = if self.requested.is_empty() {
                    &self.path
                } else {
                    &self.requested
                };
                let mut planned = self.navigation.clone();
                if let Some(next) =
                    planned.next_compact(current, action == PlayerAction::Previous, false, |id| {
                        backend.path_for_id(id).ok().flatten()
                    })
                {
                    let was_manual = planned.manual.contains(&next);
                    submit_play(
                        &mut self.navigation,
                        &mut self.requested,
                        planned,
                        next,
                        |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                    )?;
                    if let Some(id) = backend.id_for_path(&self.requested)? {
                        self.navigation.mark_id(id, was_manual);
                    }
                    self.preloaded.clear();
                    self.planned_navigation = None;
                }
            }
            PlayerAction::QueuePreview => {}
            PlayerAction::Pause if audio => backend.playback_command(PlaybackCommand::Pause)?,
            PlayerAction::Resume if audio => backend.playback_command(PlaybackCommand::Resume)?,
            PlayerAction::Stop if audio => backend.playback_command(PlaybackCommand::Stop)?,
            PlayerAction::Letter(_) => {}
            _ => {}
        }
        Ok(())
    }
    pub(super) fn queue(
        &mut self,
        backend: &Backend,
        audio: bool,
        action: QueueAction,
    ) -> Result<(), String> {
        if !matches!(action, QueueAction::Play(_)) {
            cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
                if audio {
                    backend.playback_command(PlaybackCommand::ClearQueued)
                } else {
                    Ok(())
                }
            })?;
        }
        match action {
            QueueAction::Add(source) => {
                if !self.navigation.manual.contains(&source) && self.navigation.manual.len() >= 20 {
                    return Err("Queue is full (20 songs)".into());
                }
                self.navigation.removed.remove(&source);
                if let Some(id) = backend.id_for_path(&source)? {
                    self.navigation.removed_ids.remove(&id);
                }
                self.navigation.manual.retain(|p| p != &source);
                self.navigation.manual.push(source);
            }
            QueueAction::Remove(source) => {
                self.navigation
                    .remove_compact(&self.path, &source, backend.id_for_path(&source)?)
            }
            QueueAction::Clear => self.navigation.clear_compact(&self.path),
            QueueAction::Move { source, target } => self.navigation.reorder_compact(
                &self.path,
                &source,
                &target,
                |id| backend.path_for_id(id).ok().flatten(),
                |path| backend.id_for_path(path).ok().flatten(),
            ),
            QueueAction::Play(source) if audio => {
                let planned = self.navigation.clone();
                let was_manual = planned.manual.contains(&source);
                submit_play(
                    &mut self.navigation,
                    &mut self.requested,
                    planned,
                    source,
                    |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                )?;
                if let Some(id) = backend.id_for_path(&self.requested)? {
                    self.navigation.mark_id(id, was_manual);
                }
                self.preloaded.clear();
                self.planned_navigation = None;
            }
            _ => {}
        }
        Ok(())
    }
}
