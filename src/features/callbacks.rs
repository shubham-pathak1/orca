use crate::AppState;
use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::{AppController, MainWindow};

pub fn wire_callbacks(window: &MainWindow, state: Rc<RefCell<AppController>>) {
    let weak = window.as_weak();

    window.global::<AppState>().on_play_song({
        let weak = weak.clone();
        let state = state.clone();
        move |index| {
            if index < 0 {
                return;
            }
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .play_song_from_row(index as usize, &window);
            }
        }
    });

    window.global::<AppState>().on_toggle_play_pause({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().toggle_play_pause(&window);
            }
        }
    });

    window.global::<AppState>().on_previous_song({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().play_previous_manual(&window);
            }
        }
    });

    window.global::<AppState>().on_next_song({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().play_next_manual(&window);
            }
        }
    });

    window.global::<AppState>().on_seek_to_ratio({
        let state = state.clone();
        move |ratio| {
            state.borrow_mut().seek_to_ratio(ratio);
        }
    });

    window.global::<AppState>().on_toggle_shuffle({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().toggle_shuffle(&window);
            }
        }
    });

    window.global::<AppState>().on_cycle_repeat({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().cycle_repeat(&window);
            }
        }
    });

    window.global::<AppState>().on_toggle_eq({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().toggle_eq(&window);
            }
        }
    });

    window.global::<AppState>().on_set_eq_preset({
        let weak = weak.clone();
        let state = state.clone();
        move |index| {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().set_eq_preset(index, &window);
            }
        }
    });

    window.global::<AppState>().on_apply_search({
        let weak = weak.clone();
        let state = state.clone();
        move |text: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().apply_search(text.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_clear_filter({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().clear_filter(&window);
            }
        }
    });

    window.global::<AppState>().on_filter_by_artist({
        let weak = weak.clone();
        let state = state.clone();
        move |artist: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .filter_by_artist(artist.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_filter_by_album({
        let weak = weak.clone();
        let state = state.clone();
        move |title: slint::SharedString, artist: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .filter_by_album(title.to_string(), artist.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_filter_by_playlist({
        let weak = weak.clone();
        let state = state.clone();
        move |playlist_id: i32| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .filter_by_playlist(playlist_id, &window);
            }
        }
    });

    window.global::<AppState>().on_scan_saved_roots({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().scan_saved_roots(&window);
            }
        }
    });

    window.global::<AppState>().on_enqueue_selected({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().enqueue_selected(&window);
            }
        }
    });

    window.global::<AppState>().on_clear_queue({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().clear_queue(&window);
            }
        }
    });

    window.global::<AppState>().on_enqueue_song({
        let weak = weak.clone();
        let state = state.clone();
        move |index| {
            if index < 0 {
                return;
            }
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .enqueue_song_by_row(index as usize, &window);
            }
        }
    });

    window.global::<AppState>().on_song_grid_viewport_changed({
        let weak = weak.clone();
        let state = state.clone();
        move |viewport_y, visible_height| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .update_song_grid_viewport(viewport_y, visible_height, &window);
            }
        }
    });

    window.global::<AppState>().on_set_blur_enabled({
        let weak = weak.clone();
        let state = state.clone();
        move |enabled| {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().set_app_blur_enabled(enabled, &window);
            }
        }
    });

    window.global::<AppState>().on_set_compact_library_mode({
        let weak = weak.clone();
        let state = state.clone();
        move |enabled| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .set_compact_library_mode(enabled, &window);
            }
        }
    });

    window.global::<AppState>().on_set_monochrome_mode({
        let weak = weak.clone();
        let state = state.clone();
        move |enabled| {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().set_monochrome_mode(enabled, &window);
            }
        }
    });

    window.global::<AppState>().on_set_sort_mode({
        let weak = weak.clone();
        let state = state.clone();
        move |mode| {
            if let Some(window) = weak.upgrade() {
                state.borrow_mut().set_sort_mode(mode, &window);
            }
        }
    });

    window.global::<AppState>().on_remove_scan_root({
        let weak = weak.clone();
        let state = state.clone();
        move |root_path: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .remove_scan_root(root_path.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_create_playlist({
        let weak = weak.clone();
        let state = state.clone();
        move |name: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .create_playlist(name.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_change_playlist_cover({
        let weak = weak.clone();
        let state = state.clone();
        move |playlist_id: i32| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .change_playlist_cover(playlist_id, &window);
            }
        }
    });

    window.global::<AppState>().on_rename_playlist({
        let weak = weak.clone();
        let state = state.clone();
        move |playlist_id: i32, new_name: slint::SharedString| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .rename_playlist(playlist_id, new_name.to_string(), &window);
            }
        }
    });

    window.global::<AppState>().on_delete_playlist({
        let weak = weak.clone();
        let state = state.clone();
        move |playlist_id: i32| {
            if let Some(window) = weak.upgrade() {
                state
                    .borrow_mut()
                    .delete_playlist(playlist_id, &window);
            }
        }
    });

    window.global::<AppState>().on_add_folder({
        let weak = weak.clone();
        let state = state.clone();
        move || {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                if let Some(window) = weak.upgrade() {
                    let mut s = state.borrow_mut();
                    let mut roots = s.load_scan_roots();
                    if !roots.contains(&folder) {
                        roots.push(folder);
                        s.persist_scan_roots(&roots);
                        s.scan_saved_roots(&window);
                    }
                }
            }
        }
    });

    window.global::<AppState>().on_toggle_fullscreen({
        let weak = weak.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let current = window.window().is_fullscreen();
                window.window().set_fullscreen(!current);
            }
        }
    });

    window.global::<AppState>().on_add_wheel_velocity({
        let state = state.clone();
        move |delta| {
            state.borrow_mut().scroller.add_velocity(delta * 200.0); // Scalar for velocity
        }
    });
}
