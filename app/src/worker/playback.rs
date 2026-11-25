//! Playback mutation policy lives with navigation and pending gapless plans.
use super::state::{CatalogState, PlaybackState};
use crate::{
    playback_flow::{cancel_preload, submit_play},
    protocol::{CollectionAction, Event, PlayerAction, QueueAction},
};
use orca_services::types::{PlaybackCommand, Query, Snapshot};
use orca_services::Backend;
use std::sync::mpsc::Sender;
impl PlaybackState {
    pub(super) fn publish_queue(&self, backend: &Backend, events: &Sender<Event>, current: &str) {
        let tracks = self
            .navigation
            .upcoming_limited(
                &self.path,
                20,
                |id| backend.path_for_id(id).ok().flatten(),
                |path| backend.id_for_path(path).ok().flatten(),
            )
            .into_iter()
            .filter_map(|path| backend.track(&path).ok())
            .collect();
        let _ = events.send(Event::Queue(tracks));
        let neighbors = self
            .navigation
            .neighbors_compact(current, |id| backend.path_for_id(id).ok().flatten())
            .into_iter()
            .filter_map(|path| backend.track(&path).ok())
            .collect();
        let _ = events.send(Event::Neighbors(current.into(), neighbors));
    }

    pub(super) fn collect_errors(
        &mut self,
        snapshot: &Snapshot,
        id_for_path: impl FnOnce(&str) -> Option<i64>,
    ) -> Vec<String> {
        let errors = self.errors.collect(snapshot);
        if !self.requested.is_empty()
            && snapshot.path != self.requested
            && errors.iter().any(|error| {
                error.starts_with("playback:play:")
                    || error.starts_with("playback:restore:")
                    || error.starts_with("output:")
            })
        {
            self.requested.clear();
            if !snapshot.path.is_empty() {
                self.navigation
                    .reanchor(&snapshot.path, id_for_path(&snapshot.path));
            }
        }
        errors
    }

    pub(super) fn expand_cleared_search(
        &mut self,
        backend: &Backend,
        audio: bool,
        catalog: &CatalogState,
        latest_generation: u64,
    ) -> Result<bool, String> {
        if catalog.generation != latest_generation
            || catalog.ids.is_empty()
            || (catalog.kind != "songs" && catalog.query.kind.is_empty())
            || !self.navigation.can_expand_search(&catalog.query)
        {
            return Ok(false);
        }
        let current = if self.requested.is_empty() {
            &self.path
        } else {
            &self.requested
        };
        let current_id = backend.id_for_path(current)?;
        cancel_preload(&mut self.preloaded, &mut self.planned_navigation, || {
            if audio {
                backend.playback_command(PlaybackCommand::ClearQueued)
            } else {
                Ok(())
            }
        })?;
        self.navigation
            .expand_search(catalog.query.clone(), catalog.ids.clone(), current_id);
        Ok(true)
    }

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
            CollectionAction::Play | CollectionAction::Toggle if audio => {
                if action == CollectionAction::Toggle {
                    let snapshot = backend.snapshot()?;
                    let active = backend
                        .id_for_path(&snapshot.path)?
                        .and_then(|id| self.navigation.collection_for_id(id));
                    if active.is_some_and(|query| {
                        query.kind == collection.kind
                            && query.key == collection.key
                            && query.secondary == collection.secondary
                    }) && (snapshot.duration_ms == 0
                        || snapshot.position_ms < snapshot.duration_ms)
                    {
                        return backend.playback_command(if snapshot.playing {
                            PlaybackCommand::Pause
                        } else {
                            PlaybackCommand::Resume
                        });
                    }
                }
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
