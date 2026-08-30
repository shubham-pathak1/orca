use std::sync::{Arc, Mutex};

use orca_core::{audio_engine::PlaybackState, db, library::SongMetadataUpdate};
use tauri::{Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

mod commands;
mod library_watcher;
mod phantom;
mod state;

use commands::media_controls::MediaControlsUpdate;
use state::{load_state, LibrarySnapshot, LibraryWatchMessage, SharedOrcaState};

#[tauri::command]
fn library_snapshot(state: State<'_, SharedOrcaState>) -> Result<LibrarySnapshot, String> {
    commands::library::library_snapshot(state)
}

#[tauri::command]
fn library_folder_count(state: State<'_, SharedOrcaState>) -> Result<usize, String> {
    commands::library::library_folder_count(state)
}

#[tauri::command]
fn library_scan_roots(state: State<'_, SharedOrcaState>) -> Result<Vec<String>, String> {
    commands::library::library_scan_roots(state)
}

#[tauri::command]
fn library_sources(
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<commands::library::LibrarySource>, String> {
    commands::library::library_sources(state)
}

#[tauri::command]
async fn remove_library_scan_root(
    root: String,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::remove_library_scan_root(root, app, state).await
}

#[tauri::command]
async fn rescan_library_source(
    root: String,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::rescan_library_source(root, app, state).await
}

#[tauri::command]
fn list_playlists(state: State<'_, SharedOrcaState>) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::list_playlists(state)
}

#[tauri::command]
fn create_playlist(
    name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::create_playlist(name, state)
}

#[tauri::command]
fn rename_playlist(
    playlist_id: i64,
    name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::rename_playlist(playlist_id, name, state)
}

#[tauri::command]
fn delete_playlist(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::delete_playlist(playlist_id, state)
}

#[tauri::command]
fn add_song_to_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::add_song_to_playlist(playlist_id, song_id, state)
}

#[tauri::command]
fn remove_song_from_playlist(
    playlist_id: i64,
    song_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::remove_song_from_playlist(playlist_id, song_id, state)
}

#[tauri::command]
fn playlist_song_ids(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<i64>, String> {
    commands::playlists::playlist_song_ids(playlist_id, state)
}

#[tauri::command]
fn choose_playlist_cover(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::choose_playlist_cover(playlist_id, state)
}

#[tauri::command]
fn remove_playlist_cover(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<db::Playlist>, String> {
    commands::playlists::remove_playlist_cover(playlist_id, state)
}

#[tauri::command]
fn import_playlist(
    state: State<'_, SharedOrcaState>,
) -> Result<commands::playlists::PlaylistImportResult, String> {
    commands::playlists::import_playlist(state)
}

#[tauri::command]
fn export_playlist(
    playlist_id: i64,
    state: State<'_, SharedOrcaState>,
) -> Result<commands::playlists::PlaylistExportResult, String> {
    commands::playlists::export_playlist(playlist_id, state)
}

#[tauri::command]
fn cached_lyrics(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<Option<String>, String> {
    commands::lyrics::cached_lyrics(path, state)
}

#[tauri::command]
fn cache_lyrics(
    path: String,
    lyrics: String,
    state: State<'_, SharedOrcaState>,
) -> Result<(), String> {
    commands::lyrics::cache_lyrics(path, lyrics, state)
}

#[tauri::command]
fn local_lyrics(path: String) -> Result<Option<String>, String> {
    commands::lyrics::local_lyrics(path)
}

#[tauri::command]
fn pick_lyrics_file() -> Result<Option<String>, String> {
    commands::lyrics::pick_lyrics_file()
}

#[tauri::command]
fn choose_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::choose_artist_cover(artist_name, state)
}

#[tauri::command]
fn remove_artist_cover(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::remove_artist_cover(artist_name, state)
}

#[tauri::command]
fn choose_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::choose_album_cover(album_key, state)
}

#[tauri::command]
async fn remove_album_cover(
    album_key: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::remove_album_cover(album_key, state).await
}

#[tauri::command]
async fn fetch_artist_artwork_manual(
    artist_name: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::fetch_artist_artwork_manual(artist_name, state).await
}

#[tauri::command]
async fn fetch_album_artwork_manual(
    album_key: String,
    artist: String,
    album: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::fetch_album_artwork_manual(album_key, artist, album, state).await
}

#[tauri::command]
async fn fetch_all_missing_artwork(
    state: State<'_, SharedOrcaState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    commands::artwork::fetch_all_missing_artwork(state, app).await
}

#[tauri::command]
async fn update_song_metadata(
    update: SongMetadataUpdate,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::update_song_metadata(update, state).await
}

#[tauri::command]
async fn choose_song_cover(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::choose_song_cover(path, state).await
}

#[tauri::command]
async fn remove_song_cover(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::artwork::remove_song_cover(path, state).await
}

#[tauri::command]
fn playback_snapshot(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    commands::playback::playback_snapshot(state)
}

#[tauri::command]
async fn pick_and_scan_folder(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::pick_and_scan_folder(app, state).await
}

#[tauri::command]
async fn rescan_library(
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<LibrarySnapshot, String> {
    commands::library::rescan_library(app, state).await
}

#[tauri::command]
fn play_song(path: String, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    commands::playback::play_song(path, state)
}

#[tauri::command]
fn restore_playback_session(
    path: String,
    position_ms: u64,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    commands::playback::restore_playback_session(path, position_ms, state)
}

#[tauri::command]
fn queue_next_playback(
    path: String,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    commands::playback::queue_next_playback(path, state)
}

#[tauri::command]
fn pause_playback(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    commands::playback::pause_playback(state)
}

#[tauri::command]
fn resume_playback(state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    commands::playback::resume_playback(state)
}

#[tauri::command]
fn seek_playback(
    position_ms: u64,
    state: State<'_, SharedOrcaState>,
) -> Result<PlaybackState, String> {
    commands::playback::seek_playback(position_ms, state)
}

#[tauri::command]
fn set_volume(volume: f32, state: State<'_, SharedOrcaState>) -> Result<PlaybackState, String> {
    commands::playback::set_volume(volume, state)
}

#[tauri::command]
async fn waveform_peaks(
    path: String,
    buckets: usize,
    state: State<'_, SharedOrcaState>,
) -> Result<Vec<f32>, String> {
    commands::playback::waveform_peaks(path, buckets, state).await
}

#[tauri::command]
fn update_media_controls(update: MediaControlsUpdate, state: State<'_, SharedOrcaState>) -> Result<(), String> {
    commands::media_controls::update_media_controls(update, state)
}

#[tauri::command]
fn enter_phantom_mode(
    session: phantom::PhantomSession,
    app: tauri::AppHandle,
    state: State<'_, SharedOrcaState>,
) -> Result<(), String> {
    phantom::enter_phantom_mode(session, app, state)
}

#[tauri::command]
fn update_phantom_session(
    session: phantom::PhantomSession,
    state: State<'_, SharedOrcaState>,
) -> Result<(), String> {
    phantom::update_phantom_session(session, state)
}

#[tauri::command]
fn pick_font_file() -> Result<String, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Font files", &["ttf", "otf", "woff", "woff2"])
        .set_title("Pick a font file")
        .pick_file()
    else {
        return Err("Font selection cancelled".to_string());
    };
    Ok(path.to_string_lossy().to_string())
}

pub fn run() {
    let phantom_shortcut = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::SHIFT),
        Code::KeyB,
    );
    // Register the App User Model ID so Windows taskbar thumbnail buttons
    // work in both the installed version and the portable .exe.
    #[cfg(target_os = "windows")]
    {
        // use windows::core::PCWSTR;
        // use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
        // let aumid: Vec<u16> = "dev.orca.player\0".encode_utf16().collect();
        // let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(aumid.as_ptr())) };
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_taskbar::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let state = load_state().map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            
            let mut state = state;

            // Initialize souvlaki MediaControls
            #[cfg(target_os = "windows")]
            {
                use souvlaki::{PlatformConfig, MediaControls, MediaControlEvent};
                use tauri::Manager;
                
                let hwnd = app.get_webview_window("main").unwrap().hwnd().unwrap().0 as *mut _;
                let config = PlatformConfig {
                    dbus_name: "orca",
                    display_name: "Orca",
                    hwnd: Some(hwnd),
                };
                
                match MediaControls::new(config) {
                    Ok(mut controls) => {
                        let app_handle = app.handle().clone();
                        controls.attach(move |event| {
                            match event {
                                MediaControlEvent::Play => phantom::emit_or_handle_playback_action(&app_handle, "play"),
                                MediaControlEvent::Pause => phantom::emit_or_handle_playback_action(&app_handle, "pause"),
                                MediaControlEvent::Toggle => phantom::emit_or_handle_playback_action(&app_handle, "toggle"),
                                MediaControlEvent::Next => phantom::emit_or_handle_playback_action(&app_handle, "next"),
                                MediaControlEvent::Previous => phantom::emit_or_handle_playback_action(&app_handle, "prev"),
                                _ => {}
                            }
                        }).ok();
                        
                        state.media_controls = Some(controls);
                        println!("Successfully initialized souvlaki MediaControls");
                    }
                    Err(e) => {
                        eprintln!("Failed to initialize souvlaki MediaControls: {:?}", e);
                    }
                }
            }

            let watched_roots = commands::library::watcher_roots(&mut state);
            let (watch_tx, watch_rx) = std::sync::mpsc::channel();
            state.library_watch_tx = watch_tx.clone();

            let shared_state = Arc::new(Mutex::new(state));
            library_watcher::start_library_watcher(
                app.handle().clone(),
                Arc::clone(&shared_state),
                watch_rx,
                watch_tx.clone(),
            );
            let _ = watch_tx.send(LibraryWatchMessage::UpdateRoots(watched_roots));
            app.manage(SharedOrcaState(shared_state));

            match app.global_shortcut().on_shortcut(phantom_shortcut, |app, _, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }

                println!("Phantom Mode shortcut invoked");
                let state = app.state::<SharedOrcaState>();
                let is_phantom = state
                    .0
                    .lock()
                    .ok()
                    .and_then(|state| state.phantom_controller.lock().ok().map(|controller| controller.is_active()))
                    .unwrap_or(false);

                if is_phantom {
                    if let Err(error) = phantom::show_main_window(app, &state) {
                        eprintln!("Could not restore Orca from Phantom Mode: {error}");
                    }
                } else if let Err(error) = phantom::enter_prepared_phantom_mode(app, &state) {
                    eprintln!("Could not enter Phantom Mode: {error}");
                }
            }) {
                Ok(()) => println!("Registered Ctrl+Shift+B for Phantom Mode"),
                Err(error) => eprintln!("Could not register Ctrl+Shift+B for Phantom Mode: {error}"),
            }

            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| "Orca tray icon is not configured.".to_string())?;
            let play_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/taskbar/play.ico"))?;
            let previous_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/taskbar/prev.ico"))?;
            let next_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/taskbar/next.ico"))?;
            let show = tauri::menu::IconMenuItem::with_id(app, "show", "Show Orca", true, Some(tray_icon.clone()), None::<&str>)?;
            let toggle = tauri::menu::IconMenuItem::with_id(app, "toggle", "Play/Pause", true, Some(play_icon), None::<&str>)?;
            let previous = tauri::menu::IconMenuItem::with_id(app, "previous", "Previous", true, Some(previous_icon), None::<&str>)?;
            let next = tauri::menu::IconMenuItem::with_id(app, "next", "Next", true, Some(next_icon), None::<&str>)?;
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit = tauri::menu::IconMenuItem::with_id(app, "quit", "Quit Orca", true, None::<tauri::image::Image<'static>>, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &toggle, &previous, &next, &separator, &quit])?;
            tauri::tray::TrayIconBuilder::with_id("orca-tray")
                .icon(tray_icon)
                .tooltip("Orca")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        let state = app.state::<SharedOrcaState>();
                        let _ = phantom::show_main_window(app, &state);
                    }
                    "toggle" => phantom::emit_or_handle_playback_action(app, "toggle"),
                    "previous" => phantom::emit_or_handle_playback_action(app, "prev"),
                    "next" => phantom::emit_or_handle_playback_action(app, "next"),
                    "quit" => {
                        let state = app.state::<SharedOrcaState>();
                        phantom::leave_phantom_mode(&state);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library_snapshot,
            library_folder_count,
            library_scan_roots,
            library_sources,
            remove_library_scan_root,
            rescan_library_source,
            playback_snapshot,
            list_playlists,
            create_playlist,
            rename_playlist,
            delete_playlist,
            add_song_to_playlist,
            remove_song_from_playlist,
            playlist_song_ids,
            choose_playlist_cover,
            remove_playlist_cover,
            import_playlist,
            export_playlist,
            cached_lyrics,
            cache_lyrics,
            local_lyrics,
            pick_lyrics_file,
            choose_artist_cover,
            remove_artist_cover,
            choose_album_cover,
            remove_album_cover,
            fetch_artist_artwork_manual,
            fetch_album_artwork_manual,
            fetch_all_missing_artwork,
            update_song_metadata,
            choose_song_cover,
            remove_song_cover,
            pick_and_scan_folder,
            rescan_library,
            play_song,
            restore_playback_session,
            queue_next_playback,
            pause_playback,
            resume_playback,
            seek_playback,
            waveform_peaks,
            playback_snapshot,
            update_media_controls,
            enter_phantom_mode,
            update_phantom_session,
            set_volume,
            pick_font_file
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<SharedOrcaState>();
                let phantom_active = state
                    .0
                    .lock()
                    .ok()
                    .and_then(|state| state.phantom_controller.lock().ok().map(|controller| controller.is_active()))
                    .unwrap_or(false);
                if phantom_active {
                    println!("Keeping Orca alive in Phantom Mode");
                    api.prevent_exit();
                }
            }
        });
}
