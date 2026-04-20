use crate::AppState;
use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::features::app_utils::{get_app_data_dir, get_artwork_dir, load_songs_from_db};
use crate::{AppController, AudioCommand, AudioEvent, MainWindow, SETTING_VOLUME};
use orca_core::{audio_engine, db};

pub(crate) fn bootstrap_app() -> Result<(MainWindow, Rc<RefCell<AppController>>), Box<dyn std::error::Error>> {
    println!("Starting Orca Desktop!");
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("skia".into())
        .select()?;
    println!("Backend selected.");

    let app_dir = get_app_data_dir();
    std::fs::create_dir_all(&app_dir)?;
    let artwork_dir = get_artwork_dir();
    std::fs::create_dir_all(&artwork_dir)?;

    println!("Initializing DB...");
    let conn = db::init_db(app_dir.clone())?;
    let migrated_artwork = db::migrate_inline_artwork_to_files(&conn, &artwork_dir)?;

    println!("Loading songs from DB...");
    let songs = load_songs_from_db(&conn)?;

    println!("Starting audio thread...");
    let (event_tx, event_rx) = std::sync::mpsc::channel::<AudioEvent>();
    let (audio_tx, playback_state, visualizer_data) =
        audio_engine::spawn_audio_thread(Some(move |event: &str, _| {
            if event == "playback-ended" {
                let _ = event_tx.send(AudioEvent::TrackEnded);
            }
        }));

    let initial_volume = db::get_setting(&conn, SETTING_VOLUME)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let _ = audio_tx.send(AudioCommand::SetVolume(initial_volume));
    if let Ok(mut state) = playback_state.lock() {
        state.volume = initial_volume;
    }

    println!("Building MainWindow...");
    let window = MainWindow::new()?;
    
    // Load branding logo
    let logo_path = "C:/Users/shubh/.gemini/antigravity/brain/1a6d71aa-d457-4053-ac94-9c278b649d2b/orca_premium_logo_png_1776150779681.png";
    if let Ok(logo_img) = slint::Image::load_from_path(std::path::Path::new(logo_path)) {
        window.global::<AppState>().set_branding_logo(logo_img);
    }

    window.global::<AppState>().set_status_text("Loading library...".into());
    println!("Setting up AppController...");
    let state = Rc::new(RefCell::new(AppController::new(
        conn,
        artwork_dir,
        songs,
        audio_tx,
        playback_state,
        event_rx,
        window.as_weak(),
        visualizer_data,
    )));
    println!("AppController instantiated.");

    {
        let mut app_state = state.borrow_mut();
        if migrated_artwork > 0 {
            app_state.status_text =
                format!("Migrated {migrated_artwork} embedded artwork entries.");
        }
        println!("Calling initialize_ui...");
        app_state.restore_preferences();
        app_state.initialize_ui(&window);
        println!("Finished initialize_ui.");
    }

    println!("Wiring callbacks...");
    crate::features::callbacks::wire_callbacks(&window, state.clone());
    println!("Callbacks wired.");

    Ok((window, state))
}
