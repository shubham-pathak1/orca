//! Playback keyboard bindings and command capture. Editing widgets retain text input.
use super::browse::query;
use crate::{
    protocol::{CollectionAction, PlayerAction, Request},
    AppState, OrcaWindow,
};
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
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

#[derive(Default)]
struct PlayerSwipe {
    last_event: Option<Instant>,
    last_magnitude: f32,
    distance: f32,
    triggered: bool,
}

impl PlayerSwipe {
    fn update(&mut self, now: Instant, dx: f32, dy: f32) -> (bool, Option<PlayerAction>) {
        if !dx.is_finite() || !dy.is_finite() {
            return (false, None);
        }
        let gap = self
            .last_event
            .map(|last| now.saturating_duration_since(last));
        // Inertia decays to small deltas. A fresh strong movement after that
        // decay can start another swipe without waiting out the inertia latch.
        let fresh_motion = self.triggered
            && dx.abs() >= 30.0
            && dx.abs() >= dy.abs() * 1.5
            && (gap.is_some_and(|gap| {
                gap >= Duration::from_millis(180) && self.last_magnitude <= 12.0
            }) || gap.is_some_and(|gap| gap >= Duration::from_millis(80))
                && dx.signum() != self.distance.signum());
        let quiet_period = if self.triggered { 1200 } else { 500 };
        if fresh_motion || gap.is_none_or(|gap| gap > Duration::from_millis(quiet_period)) {
            self.distance = 0.0;
            self.triggered = false;
        }
        self.last_event = Some(now);
        if dx.abs() < 1.0 || dx.abs() < dy.abs() * 1.5 {
            if !self.triggered {
                self.distance = 0.0;
            }
            self.last_magnitude = dx.abs();
            return (false, None);
        }
        self.last_magnitude = dx.abs();
        if self.triggered {
            return (true, None);
        }
        if self.distance.signum() != dx.signum() {
            self.distance = 0.0;
        }
        self.distance += dx;
        if self.distance.abs() >= 85.0 {
            self.triggered = true;
            return (
                true,
                Some(if self.distance < 0.0 {
                    PlayerAction::Next
                } else {
                    PlayerAction::Previous
                }),
            );
        }
        (true, None)
    }
}

pub(crate) fn install_player_swipe(ui: &OrcaWindow, tx: std::sync::mpsc::Sender<Request>) {
    let weak = ui.as_weak();
    let tracker = RefCell::new(PlayerSwipe::default());
    let motion_timer = Rc::new(slint::Timer::default());
    let skip = Rc::new(move |action: PlayerAction| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let state = ui.global::<AppState>();
        state.set_player_swipe_offset(if action == PlayerAction::Next {
            -20.0
        } else {
            20.0
        });
        let weak = weak.clone();
        motion_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(85),
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<AppState>().set_player_swipe_offset(0.0);
                }
            },
        );
        let _ = tx.send(Request::Command(action));
    });
    let weak = ui.as_weak();
    let wheel_skip = skip.clone();
    ui.global::<AppState>().on_player_swipe(move |dx, dy| {
        let Some(ui) = weak.upgrade() else {
            return false;
        };
        let state = ui.global::<AppState>();
        if state.get_now().path.is_empty()
            || state.get_queue_open()
            || state.get_playlist_picker_open()
        {
            return false;
        }
        let (handled, action) = tracker.borrow_mut().update(Instant::now(), dx, dy);
        if let Some(action) = action {
            wheel_skip(action);
        }
        handled
    });
    let weak = ui.as_weak();
    ui.global::<AppState>().on_player_drag(move |dx, dy| {
        let Some(ui) = weak.upgrade() else {
            return false;
        };
        let state = ui.global::<AppState>();
        if state.get_now().path.is_empty()
            || state.get_queue_open()
            || state.get_playlist_picker_open()
            || !dx.is_finite()
            || !dy.is_finite()
            || dx.abs() < 85.0
            || dx.abs() < dy.abs() * 1.3
        {
            return false;
        }
        skip(if dx < 0.0 {
            PlayerAction::Next
        } else {
            PlayerAction::Previous
        });
        true
    });
}

#[cfg(test)]
#[test]
fn horizontal_swipe_skips_once_without_reacting_to_vertical_scroll() {
    let start = Instant::now();
    let mut swipe = PlayerSwipe::default();
    assert_eq!(swipe.update(start, 15.0, 40.0), (false, None));
    assert_eq!(
        swipe.update(start + Duration::from_millis(20), -55.0, 2.0),
        (true, None)
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(40), -55.0, 2.0),
        (true, Some(PlayerAction::Next))
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(80), -200.0, 0.0),
        (true, None)
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(780), -200.0, 0.0),
        (true, None)
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(1580), -200.0, 0.0),
        (true, None)
    );
    let later = start + Duration::from_millis(3100);
    assert_eq!(swipe.update(later, 55.0, 0.0), (true, None));
    assert_eq!(
        swipe.update(later + Duration::from_millis(20), 55.0, 0.0),
        (true, Some(PlayerAction::Previous))
    );
}

#[cfg(test)]
#[test]
fn new_swipe_after_inertia_tail_does_not_wait_for_full_cooldown() {
    let start = Instant::now();
    let mut swipe = PlayerSwipe::default();
    assert_eq!(swipe.update(start, -55.0, 0.0), (true, None));
    assert_eq!(
        swipe.update(start + Duration::from_millis(20), -55.0, 0.0),
        (true, Some(PlayerAction::Next))
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(100), -8.0, 0.0),
        (true, None)
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(310), -55.0, 0.0),
        (true, None)
    );
    assert_eq!(
        swipe.update(start + Duration::from_millis(330), -55.0, 0.0),
        (true, Some(PlayerAction::Next))
    );
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
            } else if s.get_folder_dialog() {
                s.set_folder_dialog(false);
            } else if s.get_folder_remove_open() {
                s.set_folder_remove_open(false);
                s.set_folder_remove_path("".into());
            } else if s.get_playlist_picker_open() {
                s.set_playlist_picker_open(false);
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
        if s.get_metadata_open()
            || s.get_playlist_dialog()
            || s.get_collection_dialog()
            || s.get_folder_dialog()
            || s.get_folder_remove_open()
        {
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
                if !s.get_now().path.is_empty() {
                    s.set_queue_open(false);
                    s.set_playlist_picker_open(false);
                    s.set_lyrics_open(!s.get_lyrics_open());
                }
            }
            "f" if !alt => {
                if s.get_full_player() || !s.get_now().path.is_empty() {
                    s.set_full_player(!s.get_full_player());
                    s.set_queue_open(false);
                    s.set_playlist_picker_open(false);
                    ui.invoke_focus_shell();
                }
            }
            "q" if !alt => {
                s.set_queue_open(!s.get_queue_open());
                if s.get_queue_open() {
                    s.set_lyrics_open(false);
                    s.set_playlist_picker_open(false);
                    let _ = tx.send(Request::Command(PlayerAction::QueuePreview));
                }
                ui.invoke_focus_shell();
            }
            "open-settings" => {
                s.set_full_player(false);
                s.set_queue_open(false);
                s.set_playlist_picker_open(false);
                s.set_menu_open(false);
                s.set_collection_menu_open(false);
                s.set_view("settings".into());
                s.set_search("".into());
                s.set_detail_key("".into());
                s.invoke_browse();
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
