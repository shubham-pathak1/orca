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
            planned.context(backend.context_paths(&play_query)?, &target);
        }
        submit_play(
            &mut self.navigation,
            &mut self.requested,
            planned,
            target,
            |target| backend.playback_command(PlaybackCommand::Play((target).into())),
        )?;
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
                if let Some(target) = backend.context_paths(&collection)?.first().cloned() {
                    let mut planned = self.navigation.clone();
                    planned.context(backend.context_paths(&collection)?, &target);
                    submit_play(
                        &mut self.navigation,
                        &mut self.requested,
                        planned,
                        target,
                        |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                    )?;
                    self.preloaded.clear();
                    self.planned_navigation = None;
                }
            }
            CollectionAction::AddToQueue => {
                let mut all = collection;
                all.search.clear();
                let targets = backend.context_paths(&all)?;
                cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
                    if audio {
                        backend.playback_command(PlaybackCommand::ClearQueued)
                    } else {
                        Ok(())
                    }
                })?;
                for target in targets {
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
                if let Some(next) = planned.next(current, action == PlayerAction::Previous, false) {
                    submit_play(
                        &mut self.navigation,
                        &mut self.requested,
                        planned,
                        next,
                        |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                    )?;
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
                self.navigation.removed.remove(&source);
                self.navigation.manual.retain(|p| p != &source);
                self.navigation.manual.push(source);
            }
            QueueAction::Remove(source) => self.navigation.remove(&self.path, &source),
            QueueAction::Clear => self.navigation.clear(&self.path),
            QueueAction::Move { source, target } => {
                self.navigation.reorder(&self.path, &source, &target)
            }
            QueueAction::Play(source) if audio => {
                let planned = self.navigation.clone();
                submit_play(
                    &mut self.navigation,
                    &mut self.requested,
                    planned,
                    source,
                    |target| backend.playback_command(PlaybackCommand::Play((target).into())),
                )?;
                self.preloaded.clear();
                self.planned_navigation = None;
            }
            _ => {}
        }
        Ok(())
    }
}
