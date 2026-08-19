use std::{
    collections::HashSet,
    sync::{mpsc, Arc, Mutex},
};

use orca_core::audio_engine::{AudioCommand, PlaybackState};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{playback_snapshot_from, SharedOrcaState};

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhantomSession {
    pub(crate) current_path: String,
    pub(crate) order_paths: Vec<String>,
    pub(crate) removed_paths: Vec<String>,
    pub(crate) manual_paths: Vec<String>,
    pub(crate) shuffle_played_paths: Vec<String>,
    pub(crate) shuffle_enabled: bool,
    pub(crate) repeat_mode: RepeatMode,
}

#[derive(Default)]
pub(crate) struct PhantomController {
    active: bool,
    prepared_session: Option<PhantomSession>,
    current_path: Option<String>,
    playable_paths: Vec<String>,
    manual_paths: Vec<String>,
    shuffle_played_paths: HashSet<String>,
    shuffle_enabled: bool,
    repeat_mode: Option<RepeatMode>,
}

impl PhantomController {
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn prepare(&mut self, session: PhantomSession) {
        self.prepared_session = Some(session);
    }

    fn prepared_session(&self) -> Option<PhantomSession> {
        self.prepared_session.clone()
    }

    fn enter(&mut self, session: PhantomSession, available_paths: &[String]) -> Result<(), String> {
        let available: HashSet<_> = available_paths.iter().cloned().collect();
        if !available.contains(&session.current_path) {
            return Err("The current track is no longer in your library.".to_string());
        }

        let current_path = session.current_path.clone();
        let removed: HashSet<_> = session.removed_paths.into_iter().collect();
        let playable = |path: &String| path == &current_path || (!removed.contains(path) && available.contains(path));
        let ordered = if session.order_paths.is_empty() {
            available_paths.iter().filter(|path| playable(path)).cloned().collect()
        } else {
            unique_existing(session.order_paths, &available, &playable)
        };

        if !ordered.contains(&current_path) {
            return Err("The current track is not part of the active queue.".to_string());
        }

        self.active = true;
        self.current_path = Some(current_path.clone());
        self.playable_paths = ordered;
        self.manual_paths = unique_existing(session.manual_paths, &available, &playable);
        self.shuffle_played_paths = session
            .shuffle_played_paths
            .into_iter()
            .filter(|path| available.contains(path))
            .collect();
        self.shuffle_enabled = session.shuffle_enabled;
        self.repeat_mode = Some(session.repeat_mode);
        Ok(())
    }

    fn leave(&mut self) {
        self.active = false;
    }

    fn next_path(&mut self) -> Option<String> {
        let current = self.current_path.clone()?;
        let repeat_mode = self.repeat_mode.unwrap_or(RepeatMode::Off);
        if matches!(repeat_mode, RepeatMode::One) {
            return Some(current);
        }

        if let Some(next_manual) = self
            .manual_paths
            .iter()
            .find(|path| *path != &current)
            .cloned()
        {
            self.current_path = Some(next_manual.clone());
            self.shuffle_played_paths.insert(next_manual.clone());
            return Some(next_manual);
        }

        if self.shuffle_enabled {
            let mut candidates: Vec<_> = self
                .playable_paths
                .iter()
                .filter(|path| *path != &current && !self.shuffle_played_paths.contains(*path))
                .cloned()
                .collect();
            if candidates.is_empty() && matches!(repeat_mode, RepeatMode::All) {
                self.shuffle_played_paths.clear();
                self.shuffle_played_paths.insert(current.clone());
                candidates = self
                    .playable_paths
                    .iter()
                    .filter(|path| *path != &current)
                    .cloned()
                    .collect();
            }
            let next = candidates.into_iter().next()?;
            self.current_path = Some(next.clone());
            self.shuffle_played_paths.insert(next.clone());
            return Some(next);
        }

        let current_index = self.playable_paths.iter().position(|path| path == &current)?;
        let next_index = current_index + 1;
        let next = if next_index < self.playable_paths.len() {
            self.playable_paths[next_index].clone()
        } else if matches!(repeat_mode, RepeatMode::All) {
            self.playable_paths.first()?.clone()
        } else {
            return None;
        };
        self.current_path = Some(next.clone());
        self.shuffle_played_paths.insert(next.clone());
        Some(next)
    }

    fn previous_path(&mut self) -> Option<String> {
        let current = self.current_path.clone()?;
        let current_index = self.playable_paths.iter().position(|path| path == &current)?;
        let previous = current_index.checked_sub(1).and_then(|index| self.playable_paths.get(index)).cloned()?;
        self.current_path = Some(previous.clone());
        self.shuffle_played_paths.insert(previous.clone());
        Some(previous)
    }
}

fn unique_existing(
    paths: Vec<String>,
    available: &HashSet<String>,
    include: &impl Fn(&String) -> bool,
) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| available.contains(path) && include(path) && seen.insert(path.clone()))
        .collect()
}

pub(crate) enum AudioEvent {
    PlaybackEnded,
}

pub(crate) fn spawn_audio_event_handler(
    events: mpsc::Receiver<AudioEvent>,
    controller: Arc<Mutex<PhantomController>>,
    audio_tx: mpsc::Sender<AudioCommand>,
) {
    std::thread::spawn(move || {
        while let Ok(event) = events.recv() {
            if !matches!(event, AudioEvent::PlaybackEnded) {
                continue;
            }

            let next = controller
                .lock()
                .ok()
                .and_then(|mut controller| controller.is_active().then(|| controller.next_path()).flatten());
            if let Some(path) = next {
                let _ = audio_tx.send(AudioCommand::Play(path));
            }
        }
    });
}

fn enter_session(
    session: PhantomSession,
    app: AppHandle,
    shared_state: &SharedOrcaState,
) -> Result<(), String> {
    let state = shared_state.0.lock().map_err(|error| error.to_string())?;
    let available_paths = state.songs.iter().map(|song| song.path.clone()).collect::<Vec<_>>();
    state
        .phantom_controller
        .lock()
        .map_err(|error| error.to_string())?
        .enter(session, &available_paths)?;
    drop(state);

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Could not find Orca's main window.".to_string())?;
    let window_for_destroy = window.clone();
    app.run_on_main_thread(move || {
        println!("Destroying Orca's WebView for Phantom Mode");
        if let Err(error) = window_for_destroy.destroy() {
            eprintln!("Could not destroy Orca's WebView for Phantom Mode: {error}");
        }
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn update_phantom_session(
    session: PhantomSession,
    state: tauri::State<'_, SharedOrcaState>,
) -> Result<(), String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    state
        .phantom_controller
        .lock()
        .map_err(|error| error.to_string())?
        .prepare(session);
    println!("Phantom Mode playback state cached");
    Ok(())
}

pub(crate) fn enter_phantom_mode(
    session: PhantomSession,
    app: AppHandle,
    state: tauri::State<'_, SharedOrcaState>,
) -> Result<(), String> {
    enter_session(session, app, &state)
}

pub(crate) fn enter_prepared_phantom_mode(
    app: &AppHandle,
    shared_state: &SharedOrcaState,
) -> Result<(), String> {
    let session = {
        let state = shared_state.0.lock().map_err(|error| error.to_string())?;
        let session = state
            .phantom_controller
            .lock()
            .map_err(|error| error.to_string())?
            .prepared_session()
            .ok_or_else(|| "Phantom Mode is still preparing playback state. Try again in a moment.".to_string())?;
        session
    };
    println!("Entering Phantom Mode from cached playback state");
    enter_session(session, app.clone(), shared_state)
}

pub(crate) fn show_main_window(app: &AppHandle, state: &SharedOrcaState) -> Result<(), String> {
    if let Ok(state) = state.0.lock() {
        if let Ok(mut controller) = state.phantom_controller.lock() {
            controller.leave();
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    let config = app
        .config()
        .app
        .windows
        .first()
        .ok_or_else(|| "Missing main window configuration.".to_string())?;
    tauri::WebviewWindowBuilder::from_config(app, config)
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn leave_phantom_mode(state: &SharedOrcaState) {
    if let Ok(state) = state.0.lock() {
        if let Ok(mut controller) = state.phantom_controller.lock() {
            controller.leave();
        }
    }
}

pub(crate) fn toggle_playback(state: &SharedOrcaState) -> Result<PlaybackState, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    let playback = playback_snapshot_from(&state);
    let command = if playback.is_playing { AudioCommand::Pause } else { AudioCommand::Resume };
    state.audio_tx.send(command).map_err(|error| error.to_string())?;
    Ok(playback_snapshot_from(&state))
}

pub(crate) fn skip(state: &SharedOrcaState, previous: bool) -> Result<bool, String> {
    let state = state.0.lock().map_err(|error| error.to_string())?;
    let mut controller = state
        .phantom_controller
        .lock()
        .map_err(|error| error.to_string())?;
    let path = if controller.is_active() {
        if previous { controller.previous_path() } else { controller.next_path() }
    } else {
        None
    };
    drop(controller);
    if let Some(path) = path {
        state.audio_tx.send(AudioCommand::Play(path)).map_err(|error| error.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn emit_or_handle_playback_action(app: &AppHandle, action: &str) {
    let state = app.state::<SharedOrcaState>();
    let active = state
        .0
        .lock()
        .ok()
        .and_then(|state| state.phantom_controller.lock().ok().map(|controller| controller.is_active()))
        .unwrap_or(false);
    if !active {
        let _ = app.emit(&format!("media-{action}"), ());
        return;
    }

    match action {
        "play" | "pause" | "toggle" => {
            let _ = toggle_playback(&state);
        }
        "next" => {
            let _ = skip(&state, false);
        }
        "prev" => {
            let _ = skip(&state, true);
        }
        _ => {}
    }
}
