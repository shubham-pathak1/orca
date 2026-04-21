#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use slint::ComponentHandle;
use std::cell::RefCell;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use slint::{Timer, TimerMode, VecModel};
use crate::features::audio_prefs::{RepeatMode, EQ_DEFAULT_GAINS};
use orca_core::{
    audio_engine::{AudioCommand, PlaybackState},
    library::LocalSong,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyManager, GlobalHotKeyEvent,
};

mod features;

slint::include_modules!();

const SETTING_SEARCH_QUERY: &str = "tui_search_query";
const SETTING_SHUFFLE_ENABLED: &str = "tui_shuffle_enabled";
const SETTING_REPEAT_MODE: &str = "tui_repeat_mode";
const SETTING_SELECTED_PATH: &str = "tui_selected_path";
const SETTING_LIBRARY_SCAN_ROOTS: &str = "library_scan_roots";
const SETTING_VOLUME: &str = "volume";
const SETTING_EQ_ENABLED: &str = "eq_enabled";
const SETTING_EQ_GAINS: &str = "eq_gains";
const SETTING_EQ_PRESET: &str = "eq_preset";
const SETTING_APP_BLUR_ENABLED: &str = "app_blur_enabled";
const SETTING_COMPACT_LIBRARY_MODE: &str = "compact_library_mode";
const SETTING_LIBRARY_SORT_MODE: &str = "library_sort_mode";

pub(crate) const THUMB_SIZE: u32 = 64;
pub(crate) const THUMB_CACHE_LIMIT: usize = 120;
pub(crate) const THUMBNAIL_APPLY_PER_TICK: usize = 32;
pub(crate) const THUMB_WORKER_COUNT: usize = 4;
#[allow(dead_code)]
pub(crate) const BLUR_ART_SIZE: u32 = 240;
pub(crate) const NOW_ART_CACHE_LIMIT: usize = 1;
pub(crate) const BLUR_CACHE_LIMIT: usize = 1;
const GRID_MODEL_LIMIT: usize = 2000;
const UI_TICK_INTERVAL_MS: u64 = 16;
const ARTWORK_TASK_QUEUE_CAPACITY: usize = 96;

#[derive(Clone, Default)]
struct ArtistData {
    song_indices: Vec<usize>,
}

#[derive(Clone, Default)]
struct AlbumData {
    title: String,
    artist: String,
    song_indices: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LibrarySortMode {
    Title = 0,
    Artist = 1,
    Album = 2,
    Year = 3,
    Track = 4,
}

impl LibrarySortMode {
    fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "artist" | "1" => Self::Artist,
            "album" | "2" => Self::Album,
            "year" | "3" => Self::Year,
            "track" | "4" => Self::Track,
            _ => Self::Title,
        }
    }

    fn as_setting(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Year => "year",
            Self::Track => "track",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Title => "Sort: Title",
            Self::Artist => "Sort: Artist",
            Self::Album => "Sort: Album",
            Self::Year => "Sort: Year",
            Self::Track => "Sort: Track",
        }
    }
}

#[derive(Clone)]
struct SongEntry {
    song: LocalSong,
    artwork_image: slint::Image,
}

enum AudioEvent {
    TrackEnded,
}

enum ScanWorkResult {
    Success {
        scanned_roots: usize,
        scanned_root_norms: Vec<String>,
        unique_scanned: Vec<LocalSong>,
    },
    Error(String),
}

struct AppController {
    db_conn: rusqlite::Connection,
    artwork_dir: PathBuf,
    songs: Vec<SongEntry>,
    filtered_indices: Vec<usize>,
    displayed_indices: Vec<usize>,
    selected_filtered_index: Option<usize>,
    current_song_index: Option<usize>,
    shuffle_enabled: bool,
    repeat_mode: RepeatMode,
    eq_enabled: bool,
    app_blur_enabled: bool,
    monochrome_mode: bool,
    #[allow(dead_code)]
    compact_library_mode: bool,
    sort_mode: LibrarySortMode,
    eq_gains: [f32; 5],
    eq_preset: String,
    search_query: String,
    status_text: String,
    status_ready_deadline: Option<Instant>,
    blur_cache: HashMap<String, slint::Image>,
    blur_cache_order: Vec<String>,
    now_art_cache: HashMap<String, slint::Image>,
    now_art_cache_order: Vec<String>,
    last_model_playback_path: Option<String>,
    last_model_is_playing: bool,
    last_now_path: Option<String>,
    last_progress_slider_value: f32,
    last_progress_position_sec: u64,
    last_progress_duration_sec: u64,
    parsed_lyrics: Vec<(u64, String)>,
    active_lyrics_index: i32,
    last_play_pause_is_playing: Option<bool>,
    last_selected_visible_index: i32,
    queue: VecDeque<usize>,
    audio_tx: mpsc::Sender<AudioCommand>,
    playback_state: Arc<Mutex<PlaybackState>>,
    event_rx: mpsc::Receiver<AudioEvent>,
    scan_tx: mpsc::Sender<ScanWorkResult>,
    scan_rx: mpsc::Receiver<ScanWorkResult>,
    scan_in_progress: bool,
    lyrics_rx: mpsc::Receiver<(String, String)>,
    artwork_tx: mpsc::SyncSender<ArtworkTask>,
    now_artwork_tx: mpsc::Sender<NowArtworkTask>,
    now_artwork_rx: mpsc::Receiver<NowArtworkResult>,
    queued_now_artwork_path: Option<String>,
    // m_idx, pixels, w, h
    thumbnail_rx: mpsc::Receiver<(usize, Vec<u8>, u32, u32)>,
    thumbnail_cache: HashMap<usize, slint::Image>,
    thumbnail_order: VecDeque<usize>,
    thumbnail_inflight: HashSet<usize>,
    #[allow(dead_code)]
    visible_row_start: usize,
    #[allow(dead_code)]
    visible_row_end: usize,
    song_model: Rc<VecModel<SongRowData>>,
    song_row_models: Vec<Rc<VecModel<crate::SongRow>>>,
    playlist_model: Rc<VecModel<PlaylistRow>>,
    scan_roots_model: Rc<VecModel<slint::SharedString>>,
    artist_model: Rc<VecModel<ArtistRowData>>,
    album_model: Rc<VecModel<AlbumRowData>>,
    artists: HashMap<String, ArtistData>,
    albums: HashMap<String, AlbumData>,
    scroller: crate::features::scroller::KineticScroller,
    total_thumbnails: usize,
    processed_thumbnails: usize,
    #[allow(dead_code)]
    tray_icon: Option<TrayIcon>,
    #[allow(dead_code)]
    hotkey_manager: Option<GlobalHotKeyManager>,
    #[allow(dead_code)]
    phantom_hotkey: Option<HotKey>,
    #[allow(dead_code)]
    visualizer_data: orca_core::audio_engine::VisualizerData,
    last_crossfade_triggered_path: Option<String>,
}

enum ArtworkTask {
    // master_index, artwork path, with_monochrome
    LoadThumbnail { m_idx: usize, artwork_path: String, with_monochrome: bool },
    // pre-generate cache entry on disk without UI update
    WarmCache { artwork_path: String },
}

enum NowArtworkTask {
    Load { path: String, with_blur: bool, with_monochrome: bool },
}

struct NowArtworkResult {
    path: String,
    now_pixels: Vec<u8>,
    now_w: u32,
    now_h: u32,
    blur_pixels: Option<Vec<u8>>,
    blur_w: u32,
    blur_h: u32,
}

fn build_placeholder_song_entries(songs: Vec<LocalSong>) -> Vec<SongEntry> {
    songs
        .into_iter()
        .map(|song| SongEntry {
            song,
            artwork_image: slint::Image::default(),
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (state_core, artwork_migrated) = features::bootstrap::bootstrap_app_core()?;
    let window_handle = Rc::new(RefCell::new(Some(MainWindow::new()?)));
    
    {
        let win_opt = window_handle.borrow();
        let win = win_opt.as_ref().unwrap();
        features::bootstrap::setup_window_ui(win, state_core.clone(), artwork_migrated)?;
        let _ = win.show();
    }
    
    let state = state_core;

    // ── Setup Tray & Hotkeys ──
    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Quit Orca", true, None);
    let show_hide_item = MenuItem::new("Show/Hide Window", true, None);
    let _ = tray_menu.append_items(&[
        &show_hide_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ]);

    // Icon loading for Official Orca Branding
    let icon_path = concat!(env!("CARGO_MANIFEST_DIR"), "/public/orca_logo.png");
    let icon = if let Ok(img) = image::open(icon_path) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        tray_icon::Icon::from_rgba(rgba.into_raw(), width, height).ok()
    } else {
        None
    };

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Orca Music");
    
    if let Some(i) = icon {
        tray_builder = tray_builder.with_icon(i);
    }

    let tray_icon = tray_builder.build()?;

    let hotkey_manager = GlobalHotKeyManager::new()?;
    let hk_show_hide = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyB);
    let hk_monochrome = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyM);
    let hk_prev = HotKey::new(Some(Modifiers::ALT), Code::KeyP);
    let hk_next = HotKey::new(Some(Modifiers::ALT), Code::KeyN);
    
    hotkey_manager.register(hk_show_hide).ok();
    hotkey_manager.register(hk_monochrome).ok();
    hotkey_manager.register(hk_prev).ok();
    hotkey_manager.register(hk_next).ok();

    let id_show_hide = hk_show_hide.id();
    let id_monochrome = hk_monochrome.id();
    let id_prev = hk_prev.id();
    let id_next = hk_next.id();

    {
        let mut s = state.borrow_mut();
        s.tray_icon = Some(tray_icon);
        s.hotkey_manager = Some(hotkey_manager);
    }

    // Initial window close override
    if let Some(win) = window_handle.borrow().as_ref() {
        let inner_window = win.window();
        inner_window.on_close_requested(move || {
            slint::CloseRequestResponse::HideWindow
        });
    }

    println!("Starting UI tick timer...");
    let timer = Timer::default();
    {
        let window_handle_clone = window_handle.clone();
        let state_clone = state.clone();
        timer.start(
            TimerMode::Repeated,
            Duration::from_millis(UI_TICK_INTERVAL_MS),
            move || {
                {
                    let win_ref = window_handle_clone.borrow();
                    if let Some(window) = win_ref.as_ref() {
                        state_clone.borrow_mut().tick(window);
                    }
                }

                // ── Poll Tray & Hotkey Events ──
                if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    if event.state == global_hotkey::HotKeyState::Pressed {
                        let id = event.id;
                        if id == id_show_hide {
                            let mut win_opt = window_handle_clone.borrow_mut();
                            if let Some(window) = win_opt.take() {
                                state_clone.borrow_mut().dehydrate();
                                let _ = window.hide();
                                println!("Deep Sleep activated.");
                            } else {
                                if let Ok(new_win) = MainWindow::new() {
                                    if features::bootstrap::setup_window_ui(&new_win, state_clone.clone(), artwork_migrated).is_ok() {
                                        state_clone.borrow_mut().hydrate(&new_win);
                                        let _ = new_win.show();
                                        *win_opt = Some(new_win);
                                        println!("System Restored.");
                                    }
                                }
                            }
                        } else if id == id_monochrome {
                            let win_ref = window_handle_clone.borrow();
                            if let Some(window) = win_ref.as_ref() {
                                let mut s = state_clone.borrow_mut();
                                let new_val = !s.monochrome_mode;
                                s.set_monochrome_mode(new_val, &window);
                            }
                        } else if id == id_prev {
                            let win_ref = window_handle_clone.borrow();
                            if let Some(window) = win_ref.as_ref() {
                                state_clone.borrow_mut().play_previous_manual(&window);
                            }
                        } else if id == id_next {
                            let win_ref = window_handle_clone.borrow();
                            if let Some(window) = win_ref.as_ref() {
                                state_clone.borrow_mut().play_next_manual(&window);
                            }
                        }
                    }
                }

                if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    match event {
                        TrayIconEvent::Click { button, button_state, .. } => {
                            if button == tray_icon::MouseButton::Left && button_state == tray_icon::MouseButtonState::Up {
                                let mut win_opt = window_handle_clone.borrow_mut();
                                if let Some(window) = win_opt.take() {
                                    state_clone.borrow_mut().dehydrate();
                                    let _ = window.hide();
                                } else {
                                    if let Ok(new_win) = MainWindow::new() {
                                        if features::bootstrap::setup_window_ui(&new_win, state_clone.clone(), artwork_migrated).is_ok() {
                                            state_clone.borrow_mut().hydrate(&new_win);
                                            let _ = new_win.show();
                                            *win_opt = Some(new_win);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if let Ok(event) = MenuEvent::receiver().try_recv() {
                    if event.id == quit_item.id() {
                        std::process::exit(0);
                    } else if event.id == show_hide_item.id() {
                        let mut win_opt = window_handle_clone.borrow_mut();
                        if let Some(window) = win_opt.take() {
                            state_clone.borrow_mut().dehydrate();
                            let _ = window.hide();
                        } else {
                            if let Ok(new_win) = MainWindow::new() {
                                if features::bootstrap::setup_window_ui(&new_win, state_clone.clone(), artwork_migrated).is_ok() {
                                    state_clone.borrow_mut().hydrate(&new_win);
                                    let _ = new_win.show();
                                    *win_opt = Some(new_win);
                                }
                            }
                        }
                    }
                }
            },
        );
    }

    slint::run_event_loop_until_quit()?;
    Ok(())
}
