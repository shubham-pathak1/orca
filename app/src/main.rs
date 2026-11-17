#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

slint::include_modules!();
mod controllers;
mod infrastructure;
mod media;
mod platform;
mod playback;
mod protocol;
mod runtime;
#[cfg(test)]
mod tests;
mod views;
mod worker;

use infrastructure::{errors, jobs, operation_dispatch, persistence, watcher};
use media::{artwork, lyric_render, lyrics, metadata};
#[cfg(windows)]
use platform::media_session;
use platform::{hotkey, platform_integration, render_mode};
use playback::{navigation, playback_clock, playback_flow};
use views::{models, presentation};

use controllers::preferences::{apply_settings, settings_from_ui};
use orca_services::types::Track;
use platform_integration::{dispatch_tray_action, toggle_background_playback};
use presentation::waveform;
use protocol::Request;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
use worker::AppWorker;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = run();
    #[cfg(all(windows, not(debug_assertions)))]
    if let Err(error) = &result {
        if !std::env::args().any(|arg| arg == "--media-smoke" || arg == "--register-shell") {
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
    let args: Vec<String> = std::env::args().collect();
    #[cfg(windows)]
    if args.iter().any(|arg| arg == "--register-shell") {
        return media_session::identity();
    }
    #[cfg(windows)]
    if let Err(error) = media_session::identity() {
        eprintln!("Windows app identity: {error}");
    }
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
    #[cfg(windows)]
    let frame_pacing = (mode == render_mode::Mode::Cpu).then(|| {
        platform::frame_pacing::FramePacing::new(
            !args.iter().any(|arg| arg == "--legacy-frame-pacing"),
        )
    });
    #[cfg(windows)]
    let selector = if let Some(pacing) = &frame_pacing {
        selector.with_winit_custom_application_handler(pacing.clone())
    } else {
        selector
    };
    selector.select()?;
    eprintln!("Orca renderer: {renderer}");
    let audio = !args.iter().any(|a| a == "--no-audio");
    let directory = if let Some(path) = args.windows(2).find(|a| a[0] == "--data-dir") {
        PathBuf::from(&path[1])
    } else if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        orca_services::migrate_profile_directory(&PathBuf::from(base))?
    } else if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(base).join("orca-slint")
    } else {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/orca-slint")
    };
    std::fs::create_dir_all(&directory)?;
    // Import the legacy library only when the current profile has no database.
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
    let saved = persistence::read_json(&directory.join("slint-settings.json")).unwrap_or_default();
    let worker = Rc::new(AppWorker::start(directory.clone(), audio));
    let cache = Rc::new(RefCell::new(artwork::ArtworkCache::new(30 * 1024 * 1024)));
    let ui = OrcaWindow::new()?;
    ui.shell().set_app_version(env!("CARGO_PKG_VERSION").into());
    models::install_grid_window(&ui);
    ui.shell()
        .set_software_rendering(mode == render_mode::Mode::Cpu);
    #[cfg(windows)]
    if let Some(pacing) = &frame_pacing {
        pacing.attach(&ui);
    }
    platform::input_modality::install(&ui);
    let tray = OrcaTray::new()
        .map_err(|error| {
            eprintln!("Tray unavailable: {error}");
            error
        })
        .ok();
    ui.shell().set_tray_available(tray.is_some());
    {
        let weak = ui.as_weak();
        ui.shell().on_phantom(move || {
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
    ui.shell().set_phantom_global(phantom_hotkey.is_some());
    if let Some(tray) = &tray {
        let weak = ui.as_weak();
        let tx = worker.tx.clone();
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
    let catalog = Rc::new(models::CatalogModels::new(worker.tx.clone(), cache.clone()));
    let models::CatalogModels {
        list,
        grid,
        groups,
        related,
        ..
    } = catalog.as_ref();
    let player = Rc::new(playback::state::PlayerModels::new());
    let browse = Rc::new(Timer::default());
    let settings_timer = Rc::new(Timer::default());

    ui.shell().set_windows_audio(cfg!(windows));
    ui.library().set_songs(list.clone().into());
    ui.library().set_song_grid(grid.clone().into());
    ui.library().set_group_grid(groups.clone().into());
    ui.player().set_queue(player.queue.clone().into());
    ui.player().set_waveform(waveform(&[]));
    ui.library().set_related_albums(related.clone().into());
    controllers::input::install(&ui, worker.tx.clone());
    controllers::preferences::install(&ui, worker.tx.clone(), settings_timer.clone());
    controllers::collections::install(&ui, worker.tx.clone());
    controllers::editor::install(&ui, worker.tx.clone());
    controllers::playlists::install(&ui, worker.tx.clone());
    controllers::browse::install(
        &ui,
        worker.clone(),
        catalog.clone(),
        cache.clone(),
        browse.clone(),
    );
    controllers::player::install(&ui, worker.tx.clone(), player.clone());
    controllers::folders::install(&ui, worker.tx.clone());
    ui.shell().on_report_issue(|| {
        let url = "https://github.com/shubham-pathak1/orca/issues";
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer.exe").arg(url).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(url).spawn();
    });
    if let Some(scan) = args.windows(2).find(|a| a[0] == "--scan") {
        let _ = worker.tx.send(Request::Folder(scan[1].clone()));
    }
    #[cfg(windows)]
    let media_track = player.now.clone();
    let ui_runtime = runtime::start(
        &ui,
        tray.as_ref().map(OrcaTray::as_weak),
        runtime::RuntimeModels {
            worker: worker.clone(),
            cache,
            catalog,
            player,
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
        ui.player().set_playing(true);
    }
    #[cfg(windows)]
    let media_session = Rc::new(RefCell::new(None));
    #[cfg(windows)]
    if audio {
        let session = media_session.clone();
        let weak = ui.as_weak();
        let tx = worker.tx.clone();
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
    let final_settings = settings_from_ui(&ui);
    ui_runtime.stop();
    browse.stop();
    settings_timer.stop();
    drop(ui_runtime);
    drop(browse);
    drop(settings_timer);

    drop(ui);
    drop(worker);
    persistence::write_json(&directory.join("slint-settings.json"), &final_settings)?;
    Ok(())
}
