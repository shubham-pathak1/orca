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
use platform::{hotkey, render_mode};
use playback::{navigation, playback_clock, playback_flow};
use views::{models, presentation};

use protocol::Request;
use slint::{Timer, TimerMode};
use std::{path::PathBuf, rc::Rc, time::Duration};
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
    let saved =
        persistence::read_json(&directory.join("slint-settings.json")).unwrap_or_else(|| {
            crate::protocol::Settings {
                full_player_background: "soft-artwork".into(),
                ..Default::default()
            }
        });
    let worker = Rc::new(AppWorker::start(directory.clone(), audio));
    let desktop = platform::desktop::Desktop::new(
        worker.clone(),
        saved,
        mode == render_mode::Mode::Cpu,
        #[cfg(windows)]
        frame_pacing,
    );
    let phantom_hotkey = desktop.register_hotkey();
    desktop.open()?;
    if let Some(scan) = args.windows(2).find(|a| a[0] == "--scan") {
        let _ = worker.tx.send(Request::Folder(scan[1].clone()));
    }
    #[cfg(windows)]
    let media_session = if audio {
        let weak = Rc::downgrade(&desktop);
        match media_session::Session::new(worker.tx.clone(), move || {
            weak.upgrade().map(|desktop| desktop.media_state())
        }) {
            Ok(controls) => {
                if media_smoke {
                    if let Err(error) = controls.diagnose() {
                        eprintln!("Windows media controls diagnostic: {error}");
                    }
                }
                Some(controls)
            }
            Err(error) => {
                eprintln!("Windows media controls: {error}");
                None
            }
        }
    } else {
        None
    };
    let smoke_timer = Timer::default();
    if media_smoke {
        smoke_timer.start(TimerMode::SingleShot, Duration::from_secs(3), || {
            let _ = slint::quit_event_loop();
        });
    }
    slint::run_event_loop_until_quit()?;
    #[cfg(windows)]
    drop(media_session);
    drop(phantom_hotkey);
    let final_settings = desktop.final_settings();
    drop(desktop);
    drop(worker);
    persistence::write_json(&directory.join("slint-settings.json"), &final_settings)?;
    Ok(())
}
