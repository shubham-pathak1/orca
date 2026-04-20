use crate::AppState;
use slint::ComponentHandle;
use std::time::{Duration, Instant};

use crate::{build_placeholder_song_entries, AppController, ArtworkTask, MainWindow, RepeatMode};

fn decode_cached_thumbnail(cache_path: &std::path::Path) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = std::fs::read(cache_path).ok()?;
    let expected_size = (crate::THUMB_SIZE * crate::THUMB_SIZE * 4) as usize;
    if bytes.len() != expected_size {
        return None;
    }
    Some((bytes, crate::THUMB_SIZE, crate::THUMB_SIZE))
}

fn build_and_store_thumbnail(
    cache_path: &std::path::Path,
    artwork_path: &str,
) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::open(artwork_path).ok()?;
    let resized = img
        .resize_to_fill(
            crate::THUMB_SIZE,
            crate::THUMB_SIZE,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    let (w, h) = resized.dimensions();
    
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let raw_bytes = resized.as_raw();
    if std::fs::write(cache_path, raw_bytes).is_err() {
        return None;
    }

    Some((resized.into_raw(), w, h))
}

fn ensure_thumbnail_cached(cache_path: &std::path::Path, artwork_path: &str) -> bool {
    if cache_path.exists() {
        return true;
    }
    build_and_store_thumbnail(cache_path, artwork_path).is_some()
}

impl AppController {
    pub(crate) fn new(
        db_conn: rusqlite::Connection,
        artwork_dir: std::path::PathBuf,
        songs: Vec<orca_core::library::LocalSong>,
        audio_tx: std::sync::mpsc::Sender<orca_core::audio_engine::AudioCommand>,
        playback_state: std::sync::Arc<std::sync::Mutex<orca_core::audio_engine::PlaybackState>>,
        event_rx: std::sync::mpsc::Receiver<crate::AudioEvent>,
        _window_weak: slint::Weak<MainWindow>,
        visualizer_data: orca_core::audio_engine::VisualizerData,
    ) -> Self {
        let (_lyrics_tx, lyrics_rx) = std::sync::mpsc::channel();
        let (scan_tx, scan_rx) = std::sync::mpsc::channel::<crate::ScanWorkResult>();
        let (artwork_tx, artwork_rx) =
            std::sync::mpsc::sync_channel::<ArtworkTask>(crate::ARTWORK_TASK_QUEUE_CAPACITY);
        let (now_artwork_tx, now_artwork_rx) = std::sync::mpsc::channel::<crate::NowArtworkTask>();
        let (thumbnail_tx, thumbnail_rx) =
            std::sync::mpsc::channel::<(usize, Vec<u8>, u32, u32)>();

        let thumb_cache_dir = artwork_dir.join(format!("thumbs-{}", crate::THUMB_SIZE));
        let _ = std::fs::create_dir_all(&thumb_cache_dir);

        let artwork_rx = std::sync::Arc::new(std::sync::Mutex::new(artwork_rx));
        for _ in 0..crate::THUMB_WORKER_COUNT {
            let rx = artwork_rx.clone();
            let t_tx = thumbnail_tx.clone();
            let thumb_cache_dir = thumb_cache_dir.clone();
            std::thread::spawn(move || {
                loop {
                    let task = {
                        let Ok(guard) = rx.lock() else {
                            break;
                        };
                        guard.recv()
                    };

                    let Ok(task) = task else {
                        break;
                    };

                    match task {
                        ArtworkTask::LoadThumbnail {
                            m_idx,
                            artwork_path,
                            with_monochrome,
                        } => {
                            let cache_path = crate::features::app_utils::cached_thumbnail_path(
                                &thumb_cache_dir,
                                crate::THUMB_SIZE,
                                &artwork_path,
                            );
                            let thumb = decode_cached_thumbnail(&cache_path)
                                .or_else(|| build_and_store_thumbnail(&cache_path, &artwork_path));

                            if let Some((mut pixels, w, h)) = thumb {
                                if with_monochrome {
                                    for i in (0..pixels.len()).step_by(4) {
                                        let r = pixels[i] as f32;
                                        let g = pixels[i+1] as f32;
                                        let b = pixels[i+2] as f32;
                                        let gray = (r * 0.299 + g * 0.587 + b * 0.114) as u8;
                                        pixels[i] = gray;
                                        pixels[i+1] = gray;
                                        pixels[i+2] = gray;
                                    }
                                }
                                let _ = t_tx.send((m_idx, pixels, w, h));
                            }
                        }
                        ArtworkTask::WarmCache { artwork_path } => {
                            let cache_path = crate::features::app_utils::cached_thumbnail_path(
                                &thumb_cache_dir,
                                crate::THUMB_SIZE,
                                &artwork_path,
                            );
                            let _ = ensure_thumbnail_cached(&cache_path, &artwork_path);
                        }
                    }
                }
            });
        }

        let now_tx = now_artwork_tx.clone();
        let artwork_dir_for_now_worker = artwork_dir.clone();
        let now_result_tx = {
            let (tx, rx) = std::sync::mpsc::channel::<crate::NowArtworkResult>();
            // Replace receiver in local scope after worker init
            std::thread::spawn(move || {
                while let Ok(task) = now_artwork_rx.recv() {
                    match task {
                        crate::NowArtworkTask::Load { path, with_blur, with_monochrome } => {
                            let thumb_cache_dir =
                                artwork_dir_for_now_worker.join(format!("thumbs-{}", crate::THUMB_SIZE));
                            let thumb_path = crate::features::app_utils::cached_thumbnail_path(
                                &thumb_cache_dir,
                                crate::THUMB_SIZE,
                                &path,
                            );

                            // Fast path: prefer cached thumbnail for now-playing artwork so UI updates quickly.
                            let thumb_img = image::open(&thumb_path).ok();

                            // Use high-res for foreground if thumbnail is loaded, or fallback to file.
                            // The user wants ORIGINAL embedded cover art for the foreground.
                            let full_img = image::open(&path).ok();
                            
                            let (now_pixels, now_w, now_h, blur_pixels, blur_w, blur_h) =
                                if let Some(full) = full_img {
                                    let mut now_img = full.to_rgba8();
                                    let (nw, nh) = now_img.dimensions();

                                    if with_monochrome {
                                        for p in now_img.pixels_mut() {
                                            let gray = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) as u8;
                                            p[0] = gray; p[1] = gray; p[2] = gray;
                                        }
                                    }

                                    let (bp, bw, bh) = if with_blur {
                                        // Fast-Blur: Use the thumbnail for the background if it exists,
                                        // otherwise use a heavily downscaled version of the full image.
                                        let source_for_blur = thumb_img.unwrap_or(full);
                                        let blur_img = source_for_blur
                                            .resize_to_fill(
                                                128,
                                                128,
                                                image::imageops::FilterType::Triangle,
                                            );
                                        
                                        // Vibrancy Engine: Boost contrast and slight brightening to simulate web filters
                                        let mut vibrant = blur_img.adjust_contrast(15.0)
                                            .brighten(5)
                                            .blur(5.0)
                                            .to_rgba8();

                                        if with_monochrome {
                                            for p in vibrant.pixels_mut() {
                                                let gray = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) as u8;
                                                p[0] = gray; p[1] = gray; p[2] = gray;
                                            }
                                        }
                                            
                                        // Manual Vignette: Darken edges for depth (Native-stable)
                                        let (bw, bh) = vibrant.dimensions();
                                        let cx = bw as f32 / 2.0;
                                        let cy = bh as f32 / 2.0;
                                        let max_dist = (cx.powi(2) + cy.powi(2)).sqrt();
                                        
                                        for y in 0..bh {
                                            for x in 0..bw {
                                                let dx = x as f32 - cx;
                                                let dy = y as f32 - cy;
                                                let dist = (dx.powi(2) + dy.powi(2)).sqrt();
                                                let factor = 1.0 - (dist / max_dist).powf(2.2) * 0.85;
                                                let pixel = vibrant.get_pixel_mut(x, y);
                                                pixel[0] = (pixel[0] as f32 * factor) as u8;
                                                pixel[1] = (pixel[1] as f32 * factor) as u8;
                                                pixel[2] = (pixel[2] as f32 * factor) as u8;
                                            }
                                        }

                                        (Some(vibrant.into_raw()), bw, bh)
                                    } else {
                                        (None, 0, 0)
                                    };

                                    (now_img.into_raw(), nw, nh, bp, bw, bh)
                                } else {
                                    // Complete failure fallback
                                    continue;
                                };

                            let _ = tx.send(crate::NowArtworkResult {
                                path,
                                now_pixels,
                                now_w,
                                now_h,
                                blur_pixels,
                                blur_w,
                                blur_h,
                            });
                        }
                    }
                }
            });
            (now_tx, rx)
        };

        Self {
            db_conn,
            artwork_dir,
            songs: build_placeholder_song_entries(songs),
            filtered_indices: Vec::new(),
            displayed_indices: Vec::new(),
            selected_filtered_index: None,
            current_song_index: None,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::Off,
            eq_enabled: false,
            app_blur_enabled: true,
            monochrome_mode: false,
            compact_library_mode: false,
            sort_mode: crate::LibrarySortMode::Title,
            eq_gains: crate::EQ_DEFAULT_GAINS,
            eq_preset: "Flat".to_string(),
            search_query: String::new(),
            status_text: "Ready".to_string(),
            status_ready_deadline: None,
            blur_cache: std::collections::HashMap::new(),
            blur_cache_order: Vec::new(),
            now_art_cache: std::collections::HashMap::new(),
            now_art_cache_order: Vec::new(),
            last_model_playback_path: None,
            last_model_is_playing: false,
            last_now_path: None,
            last_progress_slider_value: -1.0,
            last_progress_position_sec: u64::MAX,
            last_progress_duration_sec: u64::MAX,
            parsed_lyrics: Vec::new(),
            active_lyrics_index: -1,
            last_play_pause_is_playing: None,
            last_selected_visible_index: -2,
            queue: std::collections::VecDeque::new(),
            audio_tx,
            playback_state,
            event_rx,
            scan_tx,
            scan_rx,
            scan_in_progress: false,
            lyrics_rx,
            artwork_tx,
            now_artwork_tx: now_result_tx.0,
            now_artwork_rx: now_result_tx.1,
            queued_now_artwork_path: None,
            thumbnail_rx,
                song_model: std::rc::Rc::new(slint::VecModel::<crate::SongRowData>::default()),
                song_row_models: Vec::new(),
                playlist_model: std::rc::Rc::new(slint::VecModel::<crate::PlaylistRow>::default()),
            thumbnail_cache: std::collections::HashMap::new(),
            thumbnail_order: std::collections::VecDeque::new(),
            thumbnail_inflight: std::collections::HashSet::new(),
            visible_row_start: 0,
            visible_row_end: 3,
            artist_model: std::rc::Rc::new(slint::VecModel::<crate::ArtistRowData>::default()),
            album_model: std::rc::Rc::new(slint::VecModel::<crate::AlbumRowData>::default()),
            scan_roots_model: std::rc::Rc::new(slint::VecModel::<slint::SharedString>::default()),
            artists: std::collections::HashMap::new(),
            albums: std::collections::HashMap::new(),
            scroller: crate::features::scroller::KineticScroller::new(),
            total_thumbnails: 0,
            processed_thumbnails: 0,
            tray_icon: None,
            hotkey_manager: None,
            phantom_hotkey: None,
            visualizer_data,
            last_crossfade_triggered_path: None,
        }
    }

    pub(crate) fn restore_preferences(&mut self) {
        if let Some(saved_query) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_SEARCH_QUERY) {
            self.search_query = saved_query;
        }
        if let Some(saved_shuffle) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_SHUFFLE_ENABLED) {
            self.shuffle_enabled =
                saved_shuffle == "1" || saved_shuffle.eq_ignore_ascii_case("true");
        }
        if let Some(saved_repeat) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_REPEAT_MODE) {
            self.repeat_mode = RepeatMode::from_setting(saved_repeat.as_str());
        }
        if let Some(saved_eq_enabled) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_EQ_ENABLED) {
            self.eq_enabled =
                saved_eq_enabled == "1" || saved_eq_enabled.eq_ignore_ascii_case("true");
        }
        if let Some(saved_blur_enabled) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_APP_BLUR_ENABLED) {
            self.app_blur_enabled =
                saved_blur_enabled == "1" || saved_blur_enabled.eq_ignore_ascii_case("true");
        }
        if let Some(saved_monochrome) = orca_core::db::get_setting(&self.db_conn, "orca_monochrome_mode") {
            self.monochrome_mode =
                saved_monochrome == "1" || saved_monochrome.eq_ignore_ascii_case("true");
        }
        if let Some(saved_compact_mode) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_COMPACT_LIBRARY_MODE) {
            self.compact_library_mode =
                saved_compact_mode == "1" || saved_compact_mode.eq_ignore_ascii_case("true");
        }
        if let Some(saved_sort_mode) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_LIBRARY_SORT_MODE) {
            self.sort_mode = crate::LibrarySortMode::from_setting(&saved_sort_mode);
        }
        if let Some(saved_gains) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_EQ_GAINS) {
            if let Ok(parsed) = serde_json::from_str::<Vec<f32>>(&saved_gains) {
                if parsed.len() == 5 {
                    self.eq_gains = [parsed[0], parsed[1], parsed[2], parsed[3], parsed[4]];
                }
            }
        }
        if let Some(saved_preset) = orca_core::db::get_setting(&self.db_conn, crate::SETTING_EQ_PRESET) {
            self.eq_preset = saved_preset;
        }
    }

    pub(crate) fn persist_preferences(&self) {
        let _ = orca_core::db::set_setting(&self.db_conn, crate::SETTING_SEARCH_QUERY, &self.search_query);
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_SHUFFLE_ENABLED,
            if self.shuffle_enabled { "1" } else { "0" },
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_REPEAT_MODE,
            self.repeat_mode.as_setting(),
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_EQ_ENABLED,
            if self.eq_enabled { "1" } else { "0" },
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_APP_BLUR_ENABLED,
            if self.app_blur_enabled { "1" } else { "0" },
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            "orca_monochrome_mode",
            if self.monochrome_mode { "1" } else { "0" },
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_COMPACT_LIBRARY_MODE,
            if self.compact_library_mode { "1" } else { "0" },
        );
        let _ = orca_core::db::set_setting(
            &self.db_conn,
            crate::SETTING_LIBRARY_SORT_MODE,
            self.sort_mode.as_setting(),
        );
        if let Ok(encoded_eq_gains) = serde_json::to_string(&self.eq_gains.to_vec()) {
            let _ = orca_core::db::set_setting(&self.db_conn, crate::SETTING_EQ_GAINS, &encoded_eq_gains);
        }
        let _ = orca_core::db::set_setting(&self.db_conn, crate::SETTING_EQ_PRESET, &self.eq_preset);
        if let Some(song_idx) = self.current_song_index {
            if let Some(entry) = self.songs.get(song_idx) {
                let _ = orca_core::db::set_setting(&self.db_conn, crate::SETTING_SELECTED_PATH, &entry.song.path);
            }
        }
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>, window: &MainWindow) {
        self.status_ready_deadline = None;
        self.status_text = status.into();
        window.global::<AppState>().set_status_text(self.status_text.clone().into());
    }

    pub(crate) fn set_temporary_status(
        &mut self,
        status: impl Into<String>,
        window: &MainWindow,
        for_duration: Duration,
    ) {
        self.status_text = status.into();
        window.global::<AppState>().set_status_text(self.status_text.clone().into());
        self.status_ready_deadline = Some(Instant::now() + for_duration);
    }

    pub(crate) fn initialize_ui(&mut self, window: &MainWindow) {
        self.set_status("Loading library...", window);

        let is_playing = self
            .playback_state
            .lock()
            .map(|s| s.is_playing)
            .unwrap_or(false);
        window.global::<AppState>().set_is_playing(is_playing);
        window.global::<AppState>().set_blur_enabled(self.app_blur_enabled);
        window.global::<AppState>().set_monochrome_mode(self.monochrome_mode);
        window.global::<AppState>().set_compact_library_mode(self.compact_library_mode);
        window.global::<AppState>().set_sort_mode(self.sort_mode as i32);
        window.global::<AppState>().set_sort_mode_label(self.sort_mode.label().into());
        window.global::<AppState>().set_search_text(self.search_query.clone().into());
        self.refresh_scan_roots_ui(window);
        self.refresh_filter();
        self.rebuild_group_indexes();
        self.refresh_song_model(window);
        self.refresh_eq_ui(window);
        self.apply_eq_to_audio();

        if self.songs.is_empty() {
            self.scan_saved_roots(window);
        }

        if let Some(saved_selected_path) =
            orca_core::db::get_setting(&self.db_conn, crate::SETTING_SELECTED_PATH)
        {
            if let Some(song_idx) = self
                .songs
                .iter()
                .position(|entry| entry.song.path == saved_selected_path)
            {
                self.select_song_in_filtered(song_idx);
                self.current_song_index = Some(song_idx);
            }
        }

        if self.selected_filtered_index.is_none() && !self.filtered_indices.is_empty() {
            self.selected_filtered_index = Some(0);
        }

        self.refresh_song_model(window);
        self.refresh_queue_ui(window);
        self.update_now_playing(window);

        if self.status_text == "Loading library..." {
            self.set_status("Ready", window);
        }
        window.global::<AppState>().set_status_text(self.status_text.clone().into());
        self.refresh_playlists_ui(window);
    }

    pub(crate) fn eq_toggle_label(&self) -> &'static str {
        if self.eq_enabled {
            "EQ ON"
        } else {
            "EQ OFF"
        }
    }

    pub(crate) fn eq_summary_label(&self) -> String {
        format!(
            "60:{:+.0} 230:{:+.0} 910:{:+.0} 3.6k:{:+.0} 14k:{:+.0}",
            self.eq_gains[0],
            self.eq_gains[1],
            self.eq_gains[2],
            self.eq_gains[3],
            self.eq_gains[4]
        )
    }

    pub(crate) fn refresh_eq_ui(&self, window: &MainWindow) {
        window.global::<AppState>().set_shuffle_enabled(self.shuffle_enabled);
        window.global::<AppState>().set_repeat_mode(self.repeat_mode as i32);
        window.global::<AppState>().set_eq_enabled(self.eq_enabled);
        window.global::<AppState>().set_eq_toggle_label(self.eq_toggle_label().into());
        window.global::<AppState>().set_eq_summary_label(self.eq_summary_label().into());
    }

    pub(crate) fn apply_eq_to_audio(&self) {
        let _ = self
            .audio_tx
            .send(orca_core::audio_engine::AudioCommand::SetEqGains(self.eq_gains));
        let _ = self
            .audio_tx
            .send(orca_core::audio_engine::AudioCommand::SetEqEnabled(self.eq_enabled));
    }

    pub(crate) fn set_monochrome_mode(&mut self, enabled: bool, window: &MainWindow) {
        if self.monochrome_mode == enabled { return; }
        self.monochrome_mode = enabled;
        window.global::<AppState>().set_monochrome_mode(enabled);
        self.persist_preferences();

        // ── TRANSFORM ──
        // 1. Flush all caches to force regeneration in noir/color
        self.blur_cache.clear();
        self.blur_cache_order.clear();
        self.now_art_cache.clear();
        self.now_art_cache_order.clear();
        
        // 2. Library thumbnails need to be wiped and re-queued
        self.thumbnail_cache.clear();
        self.thumbnail_order.clear();
        self.thumbnail_inflight.clear();
        self.processed_thumbnails = 0;
        
        // Wipe high-level entry markers
        for entry in self.songs.iter_mut() {
            entry.artwork_image = slint::Image::default();
        }

        // 3. Trigger immediate updates
        self.update_now_playing(window);
        self.refresh_song_model(window);
        self.schedule_thumbnail_warmup();
        
        self.set_temporary_status(
            if enabled { "Noir Mode Active 🐋🖤" } else { "Vibrant Mode Restored 🌈" },
            window,
            Duration::from_secs(2),
        );
    }

    pub(crate) fn dehydrate(&mut self) {
        println!("Deep Sleep: Dehydrating UI state...");
        // 1. Purge heavy image caches
        self.blur_cache.clear();
        self.blur_cache_order.clear();
        self.now_art_cache.clear();
        self.now_art_cache_order.clear();
        self.thumbnail_cache.clear();
        self.thumbnail_order.clear();
        self.thumbnail_inflight.clear();
        
        // 2. Clear Slint handles in global song list
        for entry in self.songs.iter_mut() {
            entry.artwork_image = slint::Image::default();
        }

        // 3. Wipe models to release Slint-resident memory
        self.song_model.set_vec(Vec::new());
        self.artist_model.set_vec(Vec::new());
        self.album_model.set_vec(Vec::new());
        self.playlist_model.set_vec(Vec::new());
        
        // Clear child models
        self.song_row_models.clear();
        
        println!("Deep Sleep: UI state purged. Sleep well, Orca.");
    }

    pub(crate) fn hydrate(&mut self, window: &MainWindow) {
        println!("Wake Up: Hydrating UI state...");
        // 1. Re-initialize everything using the core logic
        self.initialize_ui(window);
        
        // 2. Explicitly update now-playing to restore artwork/lyrics
        self.update_now_playing(window);
        
        // 3. Resume thumbnail backgrounding
        self.schedule_thumbnail_warmup();
        
        self.set_temporary_status("System Restored 🐋✨", window, Duration::from_secs(2));
    }
}
