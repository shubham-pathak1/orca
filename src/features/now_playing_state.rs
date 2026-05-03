use crate::AppState;
use slint::ComponentHandle;
use std::path::Path;
use std::rc::Rc;
use std::time::{Instant, Duration};

use slint::{ModelRc, VecModel};

use crate::features::app_utils::{format_duration, normalize_song_path};
use crate::{AppController, AudioEvent, MainWindow};

fn parse_lrc_timestamp(tag: &str) -> Option<u64> {
    let mut parts = tag.split(':');
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let sec_part = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let mut sec_split = sec_part.split('.');
    let seconds = sec_split.next()?.parse::<u64>().ok()?;
    let frac = sec_split.next().unwrap_or("0");
    let millis = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac[..3].parse::<u64>().ok()?,
    };
    Some(minutes * 60_000 + seconds * 1_000 + millis)
}

fn parse_lrc_lines(raw: &str) -> Vec<(u64, String)> {
    let mut parsed: Vec<(u64, String)> = Vec::new();

    for line in raw.lines() {
        let mut rest = line.trim();
        if rest.is_empty() {
            continue;
        }

        let mut stamps: Vec<u64> = Vec::new();
        loop {
            if !rest.starts_with('[') {
                break;
            }
            let Some(end_idx) = rest.find(']') else {
                break;
            };

            let tag = &rest[1..end_idx];
            if let Some(ms) = parse_lrc_timestamp(tag) {
                stamps.push(ms);
                rest = rest[end_idx + 1..].trim_start();
                continue;
            }
            break;
        }

        let text = rest.trim().to_string();
        if text.is_empty() {
            continue;
        }
        for stamp in stamps {
            parsed.push((stamp, text.clone()));
        }
    }

    parsed.sort_by(|a, b| a.0.cmp(&b.0));
    parsed
}

impl AppController {
    fn apply_lyrics_to_ui(&mut self, window: &MainWindow, raw_lyrics: &str) {
        self.parsed_lyrics = parse_lrc_lines(raw_lyrics);
        self.active_lyrics_index = -1;
        window.global::<AppState>().set_active_lyrics_index(-1);
        window.global::<AppState>().set_lyrics_text(raw_lyrics.into());

        if self.parsed_lyrics.is_empty() {
            let empty: ModelRc<slint::SharedString> = Rc::new(VecModel::from(Vec::<slint::SharedString>::new())).into();
            window.global::<AppState>().set_lyric_lines(empty);
            return;
        }

        let lines = self
            .parsed_lyrics
            .iter()
            .map(|(_, text)| slint::SharedString::from(text.as_str()))
            .collect::<Vec<_>>();
        let model: ModelRc<slint::SharedString> = Rc::new(VecModel::from(lines)).into();
        window.global::<AppState>().set_lyric_lines(model);
    }

    pub(crate) fn update_now_playing(&mut self, window: &MainWindow) {
        // --- Standard Active Logic ---
        let playback = self.playback_state.lock().map(|s| s.clone()).unwrap_or_default();
        let current_path = playback.current_path.clone();
        let path_changed = current_path != self.last_now_path;

        if path_changed {
            if let Some(path) = current_path.as_ref() {
                if let Some(song_idx) = self.songs.iter().position(|entry| &entry.song.path == path)
                {
                    self.current_song_index = Some(song_idx);
                    let (title, artist, artwork_path) = {
                        let entry = &self.songs[song_idx];
                        (
                            entry.song.title.clone(),
                            entry.song.artist.clone(),
                            entry.song.artwork.clone(),
                        )
                    };
                    window.global::<AppState>().set_now_title(title.into());
                    window.global::<AppState>().set_now_artist(artist.into());

                    // Format Quality Info
                    if let Some(entry) = self.songs.get(song_idx) {
                        let mut parts = Vec::new();
                        if let Some(ref fmt) = entry.song.format {
                            parts.push(fmt.clone());
                        }
                        if let (Some(depth), Some(rate)) = (entry.song.bit_depth, entry.song.sample_rate) {
                            parts.push(format!("{}-bit / {:.1} kHz", depth, rate as f32 / 1000.0));
                        } else if let Some(rate) = entry.song.sample_rate {
                            parts.push(format!("{:.1} kHz", rate as f32 / 1000.0));
                        }
                        if let Some(br) = entry.song.bitrate {
                            parts.push(format!("{} kbps", br));
                        }
                        window.global::<AppState>().set_now_playing_quality(parts.join(" | ").into());
                    }

                        self.now_art_cache.clear();
                        self.now_art_cache_order.clear();
                        self.blur_cache.clear();
                        self.blur_cache_order.clear();

                        if let Some(artwork_key) = artwork_path.as_ref() {
                        let cached_now = self.now_art_cache.get(artwork_key).cloned();
                        let cached_blur = self.blur_cache.get(artwork_key).cloned();

                        if let Some(now_img) = cached_now {
                            let missing_blur = cached_blur.is_none();
                            window.global::<AppState>().set_now_artwork(now_img.clone());
                            if self.app_blur_enabled {
                                window.global::<AppState>().set_now_artwork_blurred(
                                    cached_blur.unwrap_or_else(|| now_img.clone()),
                                );
                            } else {
                                window.global::<AppState>().set_now_artwork_blurred(now_img.clone());
                            }

                            if self.app_blur_enabled && missing_blur {
                                let _ = self.now_artwork_tx.send(crate::NowArtworkTask::Load {
                                    path: artwork_key.clone(),
                                    with_blur: true,
                                    with_monochrome: self.monochrome_mode,
                                });
                                self.queued_now_artwork_path = Some(artwork_key.clone());
                            }
                        } else {
                            let immediate = self
                                .thumbnail_cache
                                .get(&song_idx)
                                .cloned()
                                .unwrap_or_else(|| self.songs[song_idx].artwork_image.clone());

                            // Always paint something in the same frame to avoid visible lag.
                            window.global::<AppState>().set_now_artwork(immediate.clone());
                            window.global::<AppState>().set_now_artwork_blurred(immediate);

                            if self.queued_now_artwork_path.as_deref() != Some(artwork_key.as_str()) {
                                let _ = self.now_artwork_tx.send(crate::NowArtworkTask::Load {
                                    path: artwork_key.clone(),
                                    with_blur: self.app_blur_enabled,
                                    with_monochrome: self.monochrome_mode,
                                });
                                self.queued_now_artwork_path = Some(artwork_key.clone());
                            }
                        }
                    } else {
                        window.global::<AppState>().set_now_artwork(slint::Image::default());
                        window.global::<AppState>().set_now_artwork_blurred(slint::Image::default());
                        window.global::<AppState>().set_accent_color(slint::Color::from_rgb_u8(255, 255, 255));
                    }

                    self.select_song_in_filtered(song_idx);

                    if let Some(embedded_lyrics) = self
                        .songs
                        .get(song_idx)
                        .and_then(|entry| entry.song.lyrics.clone())
                        .filter(|s| !s.trim().is_empty())
                    {
                        self.apply_lyrics_to_ui(window, &embedded_lyrics);
                    } else if let Some(cached_lyrics) = orca_core::db::get_lyrics(&self.db_conn, path) {
                        self.apply_lyrics_to_ui(window, &cached_lyrics);
                    } else if let Ok(lyrics) = std::fs::read_to_string(Path::new(path).with_extension("lrc")) {
                        self.apply_lyrics_to_ui(window, &lyrics);
                    } else {
                        self.parsed_lyrics.clear();
                        self.active_lyrics_index = -1;
                        let empty: ModelRc<slint::SharedString> = Rc::new(VecModel::from(Vec::<slint::SharedString>::new())).into();
                        window.global::<AppState>().set_lyric_lines(empty);
                        window.global::<AppState>().set_active_lyrics_index(-1);
                        window.global::<AppState>().set_lyrics_text("No lyrics found for this track.".into());
                    }

                    if artwork_path.is_none() {
                        self.now_art_cache.clear();
                        self.now_art_cache_order.clear();
                        self.blur_cache.clear();
                        self.blur_cache_order.clear();
                    }
                }
            } else {
                window.global::<AppState>().set_now_title("No track selected".into());
                window.global::<AppState>().set_now_artist("".into());
                window.global::<AppState>().set_now_playing_quality("".into());
                window.global::<AppState>().set_now_artwork(slint::Image::default());
                window.global::<AppState>().set_now_artwork_blurred(slint::Image::default());
                window.global::<AppState>().set_lyrics_text("No track selected".into());
                let empty: ModelRc<slint::SharedString> = Rc::new(VecModel::from(Vec::<slint::SharedString>::new())).into();
                window.global::<AppState>().set_lyric_lines(empty);
                window.global::<AppState>().set_active_lyrics_index(-1);
                self.parsed_lyrics.clear();
                self.active_lyrics_index = -1;
                self.current_song_index = None;
                self.now_art_cache.clear();
                self.now_art_cache_order.clear();
                self.blur_cache.clear();
                self.blur_cache_order.clear();
            }
            self.last_now_path = current_path.clone();
        }

        let duration_ms = playback.duration_ms;
        let position_ms = playback.position_ms.min(duration_ms);
        let ratio = if duration_ms > 0 {
            position_ms as f32 / duration_ms as f32
        } else {
            0.0
        };
        let progress_value = ratio.clamp(0.0, 1.0);
        if self.last_progress_slider_value < 0.0
            || (progress_value - self.last_progress_slider_value).abs() >= 0.0035
            || path_changed
        {
            window.global::<AppState>().set_progress_slider_value(progress_value);
            self.last_progress_slider_value = progress_value;
        }

        let position_sec = position_ms / 1000;
        let duration_sec = duration_ms / 1000;
        if path_changed
            || position_sec != self.last_progress_position_sec
            || duration_sec != self.last_progress_duration_sec
        {
            let current = format_duration(position_ms);
            let total = format_duration(duration_ms);
            window.global::<AppState>().set_progress_text(format!("{} / {}", current, total).into());
            window.global::<AppState>().set_progress_current(current.into());
            window.global::<AppState>().set_progress_total(total.into());
            
            self.last_progress_position_sec = position_sec;
            self.last_progress_duration_sec = duration_sec;
        }

        if self.last_play_pause_is_playing != Some(playback.is_playing) {
            window.global::<AppState>().set_is_playing(playback.is_playing);
            self.last_play_pause_is_playing = Some(playback.is_playing);
        }

        if !self.parsed_lyrics.is_empty() {
            let mut idx: i32 = -1;
            for (i, (ts, _)) in self.parsed_lyrics.iter().enumerate() {
                if position_ms >= *ts {
                    idx = i as i32;
                } else {
                    break;
                }
            }
            if idx != self.active_lyrics_index {
                self.active_lyrics_index = idx;
                window.global::<AppState>().set_active_lyrics_index(idx);
            }
        }

        let should_refresh_rows = path_changed
            || current_path != self.last_model_playback_path
            || playback.is_playing != self.last_model_is_playing;
        if should_refresh_rows {
            self.refresh_song_playback_rows(window);
        } else {
            let selected_index = self
                .selected_filtered_index
                .and_then(|row| self.filtered_indices.get(row).copied())
                .and_then(|song_idx| self.displayed_indices.iter().position(|idx| *idx == song_idx))
                .map(|idx| idx as i32)
                .unwrap_or(-1);
            if selected_index != self.last_selected_visible_index {
                window.global::<AppState>().set_selected_index(selected_index);
                self.last_selected_visible_index = selected_index;
            }
        }
    }

    pub(crate) fn tick(&mut self, window: &MainWindow) {
        // --- Dormant Mode Optimization ---
        if !window.window().is_visible() {
            if !self.thumbnail_cache.is_empty() {
                self.thumbnail_cache.clear();
                self.thumbnail_order.clear();
                self.thumbnail_inflight.clear();
            }
            if !self.now_art_cache.is_empty() {
                self.now_art_cache.clear();
                self.now_art_cache_order.clear();
                self.blur_cache.clear();
                self.blur_cache_order.clear();
            }
            
            // Minimal critical status updates while hidden
            let (is_playing, _pos_ms, _dur_ms) = self.playback_state.lock()
                .map(|s| (s.is_playing, s.position_ms, s.duration_ms))
                .unwrap_or((false, 0, 0));
                
            if self.last_play_pause_is_playing != Some(is_playing) {
                window.global::<AppState>().set_is_playing(is_playing);
                self.last_play_pause_is_playing = Some(is_playing);
            }
            
            // Still update progress for the tray/taskbar if needed
            return;
        }

        // 1. Audio Event Processing
        while let Ok(event) = self.event_rx.try_recv() {
            if matches!(event, AudioEvent::TrackEnded) {
                self.on_track_finished(window);
            }
        }

        // Automatic Crossfade Lookahead (2.5s crossfade, trigger 3s before end)
        let (pos_ms, dur_ms, is_playing, curr_path) = {
            let pb = self.playback_state.lock().map(|s| s.clone()).unwrap_or_default();
            (pb.position_ms, pb.duration_ms, pb.is_playing, pb.current_path)
        };
        
        if is_playing && dur_ms > 5000 && (dur_ms - pos_ms) < 3000 {
            if let Some(path) = curr_path {
                if self.last_crossfade_triggered_path.as_ref() != Some(&path) {
                    if let Some(next_idx) = self.get_next_song_index() {
                        self.play_song_index_crossfade(next_idx, window, Duration::from_millis(2500));
                    }
                }
            }
        }

        if let Some(deadline) = self.status_ready_deadline {
            if Instant::now() >= deadline {
                self.set_status("Ready", window);
            }
        }

        // Kinetic scrolling physics (Inertial momentum)
        let scroll_delta = self.scroller.tick();
        if scroll_delta != 0.0 {
            if window.global::<AppState>().get_active_nav_index() == 0 {
                // Apply to virtualized ListView (1D Songs)
                let current = window.global::<AppState>().get_viewport_y_manual_songs();
                window.global::<AppState>().set_viewport_y_manual_songs(current + scroll_delta);
            } else {
                // Apply to ScrollView (Artists / Albums / Playlists)
                let current = window.global::<AppState>().get_viewport_y_manual();
                window.global::<AppState>().set_viewport_y_manual(current + scroll_delta);
            }
        }

        while let Ok(event) = self.event_rx.try_recv() {
            if matches!(event, AudioEvent::TrackEnded) {
                self.on_track_finished(window);
            }
        }
        while let Ok((path, lyrics)) = self.lyrics_rx.try_recv() {
            let _ = orca_core::db::set_lyrics(&self.db_conn, &path, &lyrics);
            if let Some(current_path) = self.last_now_path.as_ref() {
                if current_path == &path {
                    self.apply_lyrics_to_ui(window, &lyrics);
                }
            }
        }

        // Keep now-playing panel in sync (progress bar, sidebar info, etc.)
        self.update_now_playing(window);

        // 5. Playback Row Sync (Efficient Throttling for 1D List highlights)
        let (current_path, is_playing) = self.playback_state.lock()
            .map(|s| (s.current_path.clone(), s.is_playing))
            .unwrap_or((None, false));

        if current_path != self.last_model_playback_path || is_playing != self.last_model_is_playing {
            self.refresh_song_playback_rows(window);
            self.last_model_playback_path = current_path.clone();
            self.last_model_is_playing = is_playing;
        }

        // Dynamic viewport loading and eviction for maximum smoothness
        self.enqueue_visible_thumbnail_requests(window);
        self.trim_thumbnail_cache_to_visible_window(window);

        while let Ok(scan_result) = self.scan_rx.try_recv() {
            self.scan_in_progress = false;
            match scan_result {
                crate::ScanWorkResult::Error(err) => {
                    self.set_status(err, window);
                }
                crate::ScanWorkResult::Success {
                    scanned_roots,
                    scanned_root_norms,
                    unique_scanned,
                } => {
                    let before_count = self.songs.len();
                    if let Err(err) = orca_core::db::save_songs_to_db(&self.db_conn, &unique_scanned) {
                        self.set_status(format!("Failed saving rescan results: {err}"), window);
                        continue;
                    }

                    if !scanned_root_norms.is_empty() {
                        let stale_paths = self
                            .songs
                            .iter()
                            .filter_map(|entry| {
                                let normalized = normalize_song_path(Path::new(&entry.song.path));
                                let is_under_scanned_root = scanned_root_norms
                                    .iter()
                                    .any(|root| normalized.starts_with(root));
                                if is_under_scanned_root {
                                    None
                                } else {
                                    Some(entry.song.path.clone())
                                }
                            })
                            .collect::<Vec<_>>();

                        for path in stale_paths {
                            let _ = orca_core::db::delete_song_by_path(&self.db_conn, &path);
                        }
                    }

                    if let Err(err) = self.reload_songs_from_db() {
                        self.set_status(format!("Failed reloading library: {err}"), window);
                        continue;
                    }

                    let after_count = self.songs.len();
                    let delta = after_count as isize - before_count as isize;
                    self.refresh_song_model(window);
                    self.update_now_playing(window);
                    let warmup_jobs = self.schedule_thumbnail_warmup();
                    self.set_temporary_status(
                        format!(
                            "Rescan complete: {} folder(s), {} indexed, library {} ({:+}), thumbnail init queued {}",
                            scanned_roots,
                            unique_scanned.len(),
                            after_count,
                            delta,
                            warmup_jobs
                        ),
                        window,
                        std::time::Duration::from_secs(4),
                    );
                }
            }
        }

        while let Ok(result) = self.now_artwork_rx.try_recv() {
            self.queued_now_artwork_path = None;

            let now_buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &result.now_pixels,
                result.now_w,
                result.now_h,
            );
            let now_img = slint::Image::from_rgba8(now_buffer);

            let blur_img = if self.app_blur_enabled {
                if let Some(blur_pixels) = result.blur_pixels.as_ref() {
                    let blur_buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        blur_pixels,
                        result.blur_w,
                        result.blur_h,
                    );
                    Some(slint::Image::from_rgba8(blur_buffer))
                } else {
                    None
                }
            } else {
                None
            };

            self.now_art_cache.insert(result.path.clone(), now_img.clone());
            self.now_art_cache_order.push(result.path.clone());
            if self.now_art_cache_order.len() > crate::NOW_ART_CACHE_LIMIT {
                if let Some(evict) = self.now_art_cache_order.first().cloned() {
                    self.now_art_cache_order.remove(0);
                    self.now_art_cache.remove(&evict);
                }
            }

            if let Some(blur_img) = blur_img.clone() {
                self.blur_cache.insert(result.path.clone(), blur_img);
                self.blur_cache_order.push(result.path.clone());
                if self.blur_cache_order.len() > crate::BLUR_CACHE_LIMIT {
                    if let Some(evict) = self.blur_cache_order.first().cloned() {
                        self.blur_cache_order.remove(0);
                        self.blur_cache.remove(&evict);
                    }
                }
            }

            if let Some(current_idx) = self.current_song_index {
                if let Some(current_path) = self
                    .songs
                    .get(current_idx)
                    .and_then(|entry| entry.song.artwork.clone())
                {
                    if current_path == result.path {
                        window.global::<AppState>().set_now_artwork(now_img.clone());
                        window.global::<AppState>().set_now_artwork_blurred(
                            blur_img.unwrap_or_else(|| now_img.clone()),
                        );
                        if self.dynamic_theme_enabled {
                            if let Some(color) = result.dominant_color {
                                window.global::<AppState>().set_accent_color(color);
                            }
                        } else {
                            window.global::<AppState>().set_accent_color(slint::Color::from_rgb_u8(255, 255, 255));
                        }
                    }
                }
            }
        }

        let mut processed_thumbnail_updates = 0usize;
        let mut pending_row_updates: Vec<(usize, slint::Image)> = Vec::new();
        while processed_thumbnail_updates < crate::THUMBNAIL_APPLY_PER_TICK {
            let Ok((m_idx, pixels, w, h)) = self.thumbnail_rx.try_recv() else {
                break;
            };
            self.thumbnail_inflight.remove(&m_idx);



            processed_thumbnail_updates += 1;
            self.processed_thumbnails += 1;

            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&pixels, w, h);
            let img = slint::Image::from_rgba8(buffer);

            if let Some(entry) = self.songs.get_mut(m_idx) {
                entry.artwork_image = img.clone();
            }

            if !self.thumbnail_cache.contains_key(&m_idx) {
                self.thumbnail_order.push_back(m_idx);
            }
            self.thumbnail_cache.insert(m_idx, img.clone());

            if self.thumbnail_order.len() > crate::THUMB_CACHE_LIMIT {
                if let Some(evict_idx) = self.thumbnail_order.pop_front() {
                    self.thumbnail_cache.remove(&evict_idx);
                    if let Some(entry) = self.songs.get_mut(evict_idx) {
                        entry.artwork_image = slint::Image::default();
                    }
                    pending_row_updates.push((evict_idx, slint::Image::default()));
                }
            }

            pending_row_updates.push((m_idx, img));
        }

        if self.processed_thumbnails < self.total_thumbnails {
            window.global::<AppState>().set_processed_thumbnails(self.processed_thumbnails as i32);
            window.global::<AppState>().set_total_thumbnails(self.total_thumbnails as i32);
            let pct = (self.processed_thumbnails as f32 / self.total_thumbnails as f32 * 100.0) as i32;
            self.set_status(format!("Building Library Cache: {}% ({}/{})", pct, self.processed_thumbnails, self.total_thumbnails), window);
        } else if self.total_thumbnails > 0 && self.processed_thumbnails >= self.total_thumbnails {
            self.total_thumbnails = 0;
            self.processed_thumbnails = 0;
            self.set_temporary_status("Library cache finalized.", window, Duration::from_secs(3));
        }

        self.set_song_row_artwork_batch(pending_row_updates);
    }
}
