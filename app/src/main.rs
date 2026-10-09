#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

slint::include_modules!();
mod controllers;
mod media;
mod platform;
mod playback;
mod protocol;
mod runtime;
mod service;
#[cfg(test)]
mod tests;
mod views;
mod worker;

use media::{artwork, lyric_render, lyrics, metadata};
use platform::{hotkey, media_session, platform_integration, render_mode};
use playback::{navigation, playback_clock, playback_flow};
use service::{errors, jobs, operation_dispatch, persistence, watcher};
use views::{models, presentation};

use controllers::{
    browse::query,
    input::{command_request, install_keyboard, play_request},
    preferences::{apply_settings, clean_folder_path, setting_matches, settings},
};
use orca_services::types::Track;
use platform_integration::{dispatch_tray_action, toggle_background_playback};
use presentation::waveform;
use protocol::Request;
use runtime::SeekPreview;
use slint::{ComponentHandle, Model, ModelNotify, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use worker::Service;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = run();
    #[cfg(all(windows, not(debug_assertions)))]
    if let Err(error) = &result {
        if !std::env::args().any(|arg| arg == "--media-smoke") {
            rfd::MessageDialog::new()
                .set_title("Orca could not start")
                .set_description(errors::friendly(&error.to_string()))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }
    result
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    if let Err(error) = media_session::identity() {
        eprintln!("Windows app identity: {error}");
    }
    let args: Vec<String> = std::env::args().collect();
    let media_smoke = args.iter().any(|arg| arg == "--media-smoke");
    let mode = render_mode::requested(&args)?;
    let renderer = render_mode::renderer(&mode, cfg!(feature = "gpu"))?;
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.into());
    let selector = if mode == render_mode::Mode::Gpu {
        selector.require_opengl()
    } else {
        selector
    };
    selector.select()?;
    eprintln!("Orca renderer: {renderer}");
    let audio = !args.iter().any(|a| a == "--no-audio");
    let directory = args
        .windows(2)
        .find(|a| a[0] == "--data-dir")
        .map(|a| PathBuf::from(&a[1]))
        .unwrap_or_else(|| {
            if let Some(base) = std::env::var_os("LOCALAPPDATA") {
                PathBuf::from(base).join("OrcaSlintTauri")
            } else if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
                PathBuf::from(base).join("orca-slint")
            } else {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join(".local/share/orca-slint")
            }
        });
    std::fs::create_dir_all(&directory)?;
    // Import a SQLite snapshot on first use without modifying Tauri's database.
    // Backend startup moves referenced artwork into the native profile.
    if !media_smoke && !directory.join("orca.db").exists() {
        let source = args
            .windows(2)
            .find(|a| a[0] == "--library-db")
            .map(|a| PathBuf::from(&a[1]))
            .or_else(|| {
                std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join("orca/orca.db"))
            });
        if let Some(source) = source.filter(|path| path.is_file()) {
            orca_services::import_library(&source, &directory)?;
        }
    }
    let saved = worker::read_json(&directory.join("slint-settings.json")).unwrap_or_default();
    let service = Rc::new(Service::start(directory.clone(), audio));
    let cache = Rc::new(RefCell::new(artwork::ArtworkCache::new(30 * 1024 * 1024)));
    let lyric_renderer = Rc::new(RefCell::new(lyric_render::Renderer::new()));
    let playback_clock = Rc::new(RefCell::new(playback_clock::PlaybackClock::new()));
    let ui = OrcaWindow::new()?;
    let tray = OrcaTray::new()
        .map_err(|error| {
            eprintln!("Tray unavailable: {error}");
            error
        })
        .ok();
    ui.global::<AppState>().set_tray_available(tray.is_some());
    {
        let weak = ui.as_weak();
        ui.global::<AppState>().on_phantom(move || {
            if let Some(ui) = weak.upgrade() {
                toggle_background_playback(&ui);
            }
        });
    }
    let phantom_hotkey = if tray.is_some() {
        let weak = ui.as_weak();
        hotkey::PhantomHotkey::register(move || {
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    toggle_background_playback(&ui);
                }
            });
        })
        .map_err(|error| eprintln!("Phantom shortcut unavailable: {error}"))
        .ok()
    } else {
        None
    };
    ui.global::<AppState>()
        .set_phantom_global(phantom_hotkey.is_some());
    if let Some(tray) = &tray {
        let weak = ui.as_weak();
        let tx = service.tx.clone();
        tray.on_action(move |action| {
            if action == "quit" {
                let _ = slint::quit_event_loop();
            } else if let Some(ui) = weak.upgrade() {
                dispatch_tray_action(&ui, &tx, &action);
            }
        });
    }
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    apply_settings(&ui, saved);
    let tracks = Rc::new(models::Tracks::new(service.tx.clone(), cache.clone()));
    let list = Rc::new(models::SongList {
        store: tracks.clone(),
        notify: ModelNotify::default(),
    });
    let grid = Rc::new(models::SongGrid {
        visible_rows: RefCell::new(std::collections::HashMap::new()),
        store: tracks.clone(),
        notify: ModelNotify::default(),
        columns: Cell::new(6),
        edge: Cell::new(192),
    });
    let groups = Rc::new(models::GroupGrid {
        landscape: Cell::new(false),
        visible_rows: RefCell::new(std::collections::HashMap::new()),
        artwork: Cell::new(true),
        circular: Cell::new(false),
        groups: RefCell::new(vec![]),
        notify: ModelNotify::default(),
        columns: Cell::new(6),
        edge: Cell::new(240),
        cache: cache.clone(),
    });
    let related = Rc::new(models::GroupGrid {
        landscape: Cell::new(false),
        visible_rows: RefCell::new(std::collections::HashMap::new()),
        artwork: Cell::new(true),
        circular: Cell::new(false),
        groups: RefCell::new(vec![]),
        notify: ModelNotify::default(),
        columns: Cell::new(2),
        edge: Cell::new(160),
        cache: cache.clone(),
    });
    let neighbors = Rc::new(RefCell::new(Vec::<Track>::new()));
    let detail_art = Rc::new(RefCell::new((String::new(), String::new())));
    let queue = Rc::new(VecModel::<Song>::default());
    let queue_tracks = Rc::new(RefCell::new(Vec::<Track>::new()));
    let now = Rc::new(RefCell::new(Track::default()));
    let lines = Rc::new(RefCell::new(Vec::<LyricLine>::new()));
    let preview = Rc::new(RefCell::new(None::<SeekPreview>));
    let browse = Rc::new(Timer::default());
    let settings_timer = Rc::new(Timer::default());
    let state = ui.global::<AppState>();
    state.on_display_path(|path| clean_folder_path(&path).into());
    state.on_setting_matches(|text, search| setting_matches(&text, &search));
    {
        let weak = ui.as_weak();
        state.on_font_scale(move |value| {
            if let Some(ui) = weak.upgrade() {
                if let Ok(value) = value.trim().parse::<f32>() {
                    if value.is_finite() {
                        let s = ui.global::<AppState>();
                        s.set_scale(value.clamp(80.0, 120.0) / 100.0);
                        s.invoke_save_settings();
                    }
                }
            }
        });
    }
    state.set_songs(list.clone().into());
    state.set_song_grid(grid.clone().into());
    state.set_group_grid(groups.clone().into());
    state.set_queue(queue.clone().into());
    state.set_waveform(waveform(&[]));
    state.set_related_albums(related.clone().into());
    {
        let weak = ui.as_weak();
        state.on_unfocus(move || {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_focus_shell();
            }
        });
    }
    install_keyboard(&ui, service.tx.clone());
    controllers::preferences::install(&ui, service.tx.clone(), settings_timer.clone());
    controllers::collections::install(&ui, service.tx.clone());
    controllers::editor::install(&ui, service.tx.clone());
    controllers::playlists::install(&ui, service.tx.clone());
    {
        let weak = ui.as_weak();
        let service = service.clone();
        let timer = browse.clone();
        state.on_browse(move || {
            let weak = weak.clone();
            let service = service.clone();
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(100),
                move || {
                    if let Some(ui) = weak.upgrade() {
                        let generation = service.latest_query.fetch_add(1, Ordering::Relaxed) + 1;
                        let (query, view) = query(&ui);
                        let _ = service.tx.send(Request::Browse(generation, query, view));
                    }
                },
            );
        });
    }
    {
        let tx = service.tx.clone();
        let weak = ui.as_weak();
        state.on_play(move |path| {
            if let Some(ui) = weak.upgrade() {
                let _ = tx.send(play_request(&ui, path.into()));
            }
        });
    }
    {
        let tx = service.tx.clone();
        let weak = ui.as_weak();
        state.on_command(move |action, value| {
            if let Some(ui) = weak.upgrade() {
                match command_request(&ui, action.into(), value as f64) {
                    Ok(request) => {
                        let _ = tx.send(request);
                    }
                    Err(error) => ui.global::<AppState>().set_error(error.into()),
                }
            }
        });
    }
    {
        let tx = service.tx.clone();
        let weak = ui.as_weak();
        let preview = preview.clone();
        state.on_seek(move |target| {
            if let Some(ui) = weak.upgrade() {
                let state = ui.global::<AppState>();
                *preview.borrow_mut() = Some(SeekPreview {
                    target,
                    path: state.get_now().path.into(),
                    started: Instant::now(),
                });
                state.set_position(target);
                state.set_lyric_position(target);
                state.set_elapsed(models::duration(target.max(0.0) as u64).into());
            }
            let _ = tx.send(Request::Seek(target as f64));
        });
    }
    {
        let tx = service.tx.clone();
        let weak = ui.as_weak();
        state.on_volume_changed(move |value| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<AppState>().set_volume(value.clamp(0.0, 1.0));
            }
            let _ = tx.send(Request::Volume(value as f64));
        });
    }
    {
        let tx = service.tx.clone();
        state.on_refresh(move || {
            let _ = tx.send(Request::Refresh);
        });
    }
    {
        let tx = service.tx.clone();
        state.on_folder_action(move |action, folder| {
            let request = if action == "rescan" {
                Request::Folder(folder.into())
            } else {
                Request::Operation(
                    orca_services::operation_types::OperationRequest::RemoveSource {
                        key: folder.to_string(),
                    },
                )
            };
            let _ = tx.send(request);
        });
    }
    {
        let tx = service.tx.clone();
        state.on_add_folder(move || {
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    let _ = tx.send(Request::Folder(path.to_string_lossy().into()));
                }
            });
        });
    }
    {
        let weak = ui.as_weak();
        let header_art = detail_art.clone();
        let header_albums = related.clone();
        state.on_open_group(move |key, secondary, title| {
            if let Some(ui) = weak.upgrade() {
                let s = ui.global::<AppState>();
                header_art.borrow_mut().0.clear();
                header_art.borrow_mut().1.clear();
                header_albums.groups.borrow_mut().clear();
                header_albums.reset();
                s.set_detail_cover(slint::Image::default());
                s.set_detail_backdrop(slint::Image::default());
                s.set_detail_cover_missing(false);
                s.set_detail_summary("Loading...".into());
                s.set_detail_key(key);
                s.set_detail_secondary(secondary);
                s.set_detail_title(title);
                s.set_search("".into());
                s.invoke_browse();
            }
        });
    }
    {
        let weak = ui.as_weak();
        state.on_go_back(move || {
            if let Some(ui) = weak.upgrade() {
                let s = ui.global::<AppState>();
                if s.get_view() == "folders" && !s.get_detail_secondary().is_empty() {
                    let parent = s.get_detail_secondary().to_string();
                    let root = s
                        .get_folders()
                        .iter()
                        .any(|root| root.replace('\\', "/").trim_end_matches('/') == parent);
                    let ancestor = if root {
                        String::new()
                    } else {
                        parent
                            .rsplit_once('/')
                            .map(|(path, _)| path.to_string())
                            .unwrap_or_default()
                    };
                    s.set_detail_title(parent.rsplit('/').next().unwrap_or(&parent).into());
                    s.set_detail_key(parent.into());
                    s.set_detail_secondary(ancestor.into());
                } else {
                    s.set_detail_key("".into());
                    s.set_detail_secondary("".into());
                }
                s.set_search("".into());
                s.invoke_browse();
            }
        });
    }
    {
        let tx = service.tx.clone();
        state.on_queue_action(move |action, source, target| {
            if let Ok(action) =
                crate::protocol::QueueAction::parse(&action, source.into(), target.into())
            {
                let _ = tx.send(Request::Queue(action));
            }
        });
    }
    state.on_report_issue(|| {
        let url = "https://github.com/shubham-pathak1/orca/issues";
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer.exe").arg(url).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(url).spawn();
    });
    {
        let tx = service.tx.clone();
        let weak = ui.as_weak();
        state.on_menu(move |path, x, y| {
            if let Some(ui) = weak.upgrade() {
                let s = ui.global::<AppState>();
                s.set_menu_x(x);
                s.set_menu_y(y);
            }
            let _ = tx.send(Request::Context(path.into()));
        });
    }
    if let Some(scan) = args.windows(2).find(|a| a[0] == "--scan") {
        let _ = service.tx.send(Request::Folder(scan[1].clone()));
    }
    {
        let renderer = lyric_renderer.clone();
        state.on_lyric_render(move |index, _| renderer.borrow_mut().get_signed(index));
        let renderer = lyric_renderer.clone();
        state.on_lyric_offset(move |index, _| renderer.borrow().offset(index.max(0) as usize));
    }
    #[cfg(windows)]
    let media_track = now.clone();
    let poll = runtime::install(
        &ui,
        tray.as_ref().map(OrcaTray::as_weak),
        runtime::RuntimeModels {
            service: service.clone(),
            cache,
            lyric_renderer,
            playback_clock,
            tracks,
            list,
            grid,
            groups,
            related,
            neighbors,
            detail_art,
            queue,
            queue_tracks,
            now,
            lines,
            preview,
        },
    );
    ui.show()?;
    #[cfg(windows)]
    if media_smoke {
        *media_track.borrow_mut() = Track {
            path: "media-smoke".into(),
            title: "Orca media controls check".into(),
            artist: "Orca".into(),
            ..Default::default()
        };
        ui.global::<AppState>().set_playing(true);
    }
    #[cfg(windows)]
    let media_session = Rc::new(RefCell::new(None));
    #[cfg(windows)]
    if audio {
        let session = media_session.clone();
        let weak = ui.as_weak();
        let tx = service.tx.clone();
        // Winit creates its HWND when the event loop starts, after show().
        Timer::single_shot(Duration::from_millis(200), move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            match media_session::Session::new(&ui, tx, media_track) {
                Ok(controls) => {
                    if media_smoke {
                        if let Err(error) = controls.diagnose() {
                            eprintln!("Windows media controls diagnostic: {error}");
                        }
                    }
                    *session.borrow_mut() = Some(controls);
                }
                Err(error) => eprintln!("Windows media controls: {error}"),
            }
        });
    }
    let smoke_timer = Timer::default();
    if media_smoke {
        smoke_timer.start(TimerMode::SingleShot, Duration::from_secs(3), || {
            let _ = slint::quit_event_loop();
        });
    }
    slint::run_event_loop_until_quit()?;
    #[cfg(windows)]
    drop(media_session.borrow_mut().take());
    drop(phantom_hotkey);
    drop(tray);
    let final_settings = settings(&ui);
    poll.stop();
    browse.stop();
    settings_timer.stop();
    drop(poll);
    drop(browse);
    drop(settings_timer);
    drop(state);
    drop(ui);
    drop(service);
    worker::write_json(&directory.join("slint-settings.json"), &final_settings)?;
    Ok(())
}
