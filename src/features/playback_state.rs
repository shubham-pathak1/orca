use crate::AppState;
use slint::ComponentHandle;
use rand::seq::SliceRandom;

use crate::features::audio_prefs::eq_preset_from_index;
use crate::{AppController, MainWindow, RepeatMode};

impl AppController {
    pub(crate) fn play_song_from_row(&mut self, row_index: usize, window: &MainWindow) {
        // UI sends master song index via `song.master_idx`; keep row-index fallback for safety.
        if self.songs.get(row_index).is_some() {
            self.play_song_index(row_index, window);
            return;
        }

        if let Some(song_idx) = self.displayed_indices.get(row_index).copied() {
            self.play_song_index(song_idx, window);
        }
    }

    pub(crate) fn play_song_index(&mut self, song_idx: usize, window: &MainWindow) {
        if let Some(entry) = self.songs.get(song_idx) {
            let path = entry.song.path.clone();
            let artist = entry.song.artist.clone();
            let title = entry.song.title.clone();

            // Optimistic UI state
            if let Ok(mut playback) = self.playback_state.lock() {
                playback.current_path = Some(path.clone());
                playback.is_playing = true;
                playback.position_ms = 0;
                playback.duration_ms = entry.song.duration as u64;
            }

            let _ = self.audio_tx.send(orca_core::audio_engine::AudioCommand::Play(path));
            self.current_song_index = Some(song_idx);
            self.last_crossfade_triggered_path = None; // Reset crossfade tracker
            self.select_song_in_filtered(song_idx);
            self.refresh_song_model(window);
            self.update_now_playing(window);
            self.set_status(format!("Now playing: {} - {}", artist, title), window);
            self.persist_preferences();
        }
    }

    pub(crate) fn play_song_index_crossfade(&mut self, song_idx: usize, window: &MainWindow, duration: std::time::Duration) {
        if let Some(entry) = self.songs.get(song_idx) {
            let path = entry.song.path.clone();
            let artist = entry.song.artist.clone();
            let title = entry.song.title.clone();

            if let Ok(mut playback) = self.playback_state.lock() {
                playback.current_path = Some(path.clone());
                playback.is_playing = true;
                playback.position_ms = 0;
                playback.duration_ms = entry.song.duration as u64;
            }

            let _ = self.audio_tx.send(orca_core::audio_engine::AudioCommand::PlayCrossfade(path.clone(), duration));
            
            self.current_song_index = Some(song_idx);
            self.last_crossfade_triggered_path = Some(path);
            self.select_song_in_filtered(song_idx);
            self.refresh_song_model(window);
            self.update_now_playing(window);
            self.set_status(format!("Fading into: {} - {}", artist, title), window);
            self.persist_preferences();
        }
    }

    pub(crate) fn get_next_song_index(&self) -> Option<usize> {
        // Look at queue
        if let Some(next_idx) = self.queue.front().copied() {
            return Some(next_idx);
        }
        
        if self.repeat_mode == RepeatMode::One {
            return self.current_song_index;
        }

        if self.shuffle_enabled {
            if self.filtered_indices.is_empty() { return None; }
            let mut candidates = self.filtered_indices.clone();
            if candidates.len() > 1 {
                if let Some(current_idx) = self.current_song_index {
                    candidates.retain(|idx| *idx != current_idx);
                }
            }
            let mut rng = rand::thread_rng();
            return candidates.choose(&mut rng).copied();
        }

        if let Some(current_idx) = self.current_song_index {
            if current_idx + 1 < self.songs.len() {
                return Some(current_idx + 1);
            } else if self.repeat_mode == RepeatMode::All {
                return Some(0);
            }
        }
        None
    }

    pub(crate) fn toggle_play_pause(&mut self, window: &MainWindow) {
        let playback = self
            .playback_state
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();

        if playback.is_playing {
            let _ = self.audio_tx.send(crate::AudioCommand::Pause);
            self.set_status("Paused", window);
            window.global::<AppState>().set_is_playing(false);
            return;
        }

        if let Some(song_idx) = self.selected_song_index() {
            if self.current_song_index != Some(song_idx) || playback.current_path.is_none() {
                self.play_song_index(song_idx, window);
                return;
            }
        }

        if playback.current_path.is_some() {
            let _ = self.audio_tx.send(crate::AudioCommand::Resume);
            self.set_status("Resumed", window);
            window.global::<AppState>().set_is_playing(true);
        }
    }

    pub(crate) fn play_random(&mut self, window: &MainWindow) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let mut candidates = self.filtered_indices.clone();
        if candidates.len() > 1 {
            if let Some(current_idx) = self.current_song_index {
                candidates.retain(|idx| *idx != current_idx);
            }
        }

        let mut rng = rand::thread_rng();
        if let Some(song_idx) = candidates.choose(&mut rng).copied() {
            self.play_song_index(song_idx, window);
        }
    }

    pub(crate) fn play_next_manual(&mut self, window: &MainWindow) {
        if let Some(next_idx) = self.pop_queue_next(window) {
            self.play_song_index(next_idx, window);
            return;
        }
        if self.shuffle_enabled {
            self.play_random(window);
            return;
        }
        if let Some(current_idx) = self.current_song_index {
            if current_idx + 1 < self.songs.len() {
                self.play_song_index(current_idx + 1, window);
            } else if self.repeat_mode == RepeatMode::All {
                self.play_song_index(0, window);
            }
        } else if let Some(song_idx) = self.selected_song_index() {
            self.play_song_index(song_idx, window);
        }
    }

    pub(crate) fn play_previous_manual(&mut self, window: &MainWindow) {
        if self.shuffle_enabled {
            self.play_random(window);
            return;
        }
        if let Some(current_idx) = self.current_song_index {
            if current_idx > 0 {
                self.play_song_index(current_idx - 1, window);
            } else if self.repeat_mode == RepeatMode::All && !self.songs.is_empty() {
                self.play_song_index(self.songs.len() - 1, window);
            }
        }
    }

    pub(crate) fn on_track_finished(&mut self, window: &MainWindow) {
        if let Some(next_idx) = self.pop_queue_next(window) {
            self.play_song_index(next_idx, window);
            return;
        }
        if self.repeat_mode == RepeatMode::One {
            if let Some(current_idx) = self.current_song_index {
                self.play_song_index(current_idx, window);
                return;
            }
        }
        if self.shuffle_enabled {
            self.play_random(window);
            return;
        }
        if let Some(current_idx) = self.current_song_index {
            if current_idx + 1 < self.songs.len() {
                self.play_song_index(current_idx + 1, window);
            } else if self.repeat_mode == RepeatMode::All {
                self.play_song_index(0, window);
            }
        }
    }

    pub(crate) fn seek_to_ratio(&mut self, ratio: f32) {
        let duration_ms = self
            .playback_state
            .lock()
            .map(|s| s.duration_ms)
            .unwrap_or(0);
        if duration_ms == 0 {
            return;
        }

        let seek_ms = (duration_ms as f32 * ratio.clamp(0.0, 1.0)) as u64;
        let _ = self
            .audio_tx
            .send(crate::AudioCommand::Seek(std::time::Duration::from_millis(seek_ms)));

        if let Ok(mut state) = self.playback_state.lock() {
            state.position_ms = seek_ms;
        }
    }

    pub(crate) fn toggle_shuffle(&mut self, window: &MainWindow) {
        self.shuffle_enabled = !self.shuffle_enabled;
        window.global::<AppState>().set_shuffle_enabled(self.shuffle_enabled);
        self.persist_preferences();
    }

    pub(crate) fn cycle_repeat(&mut self, window: &MainWindow) {
        self.repeat_mode = self.repeat_mode.cycle();
        window.global::<AppState>().set_repeat_mode(self.repeat_mode as i32);
        self.persist_preferences();
    }

    pub(crate) fn toggle_eq(&mut self, window: &MainWindow) {
        self.eq_enabled = !self.eq_enabled;
        self.refresh_eq_ui(window);
        self.apply_eq_to_audio();
        self.persist_preferences();
        self.set_status(
            if self.eq_enabled {
                "Equalizer enabled"
            } else {
                "Equalizer disabled"
            },
            window,
        );
    }

    pub(crate) fn set_eq_preset(&mut self, preset_index: i32, window: &MainWindow) {
        let (preset_name, preset_gains) = eq_preset_from_index(preset_index);
        self.eq_preset = preset_name.to_string();
        self.eq_gains = preset_gains;
        self.eq_enabled = true;

        self.refresh_eq_ui(window);
        self.apply_eq_to_audio();
        self.persist_preferences();
        self.set_status(format!("EQ preset: {}", preset_name), window);
    }
}
