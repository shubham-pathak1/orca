use crate::AppState;
use slint::ComponentHandle;
use std::rc::Rc;

use slint::{Model, VecModel};

use crate::features::app_utils::format_duration;
use crate::{AlbumData, AlbumRow, AlbumRowData, AppController, ArtistRow, ArtistRowData, MainWindow, SongRow, SongRowData, GRID_MODEL_LIMIT};

impl AppController {
    fn representative_artwork(&self, song_idx: usize) -> slint::Image {
        if let Some(img) = self.thumbnail_cache.get(&song_idx) {
            return img.clone();
        }

        let Some(entry) = self.songs.get(song_idx) else {
            return slint::Image::default();
        };

        entry.artwork_image.clone()
    }

    pub(crate) fn refresh_song_playback_rows(&mut self, window: &MainWindow) {
        let (current_path, is_playing) = self
            .playback_state
            .lock()
            .map(|s| (s.current_path.clone(), s.is_playing))
            .unwrap_or((None, false));

        for row_idx in 0..self.song_model.row_count() {
            let Some(row_data) = self.song_model.row_data(row_idx) else {
                continue;
            };

            let mut changed = false;
            let mut songs_vec: Vec<SongRow> = Vec::new();
            for si in 0..row_data.songs.row_count() {
                if let Some(mut song) = row_data.songs.row_data(si) {
                    let should_play = current_path
                        .as_ref()
                        .and_then(|path| {
                            self.songs
                                .get(song.master_idx as usize)
                                .map(|entry| entry.song.path == *path)
                        })
                        .unwrap_or(false)
                        && is_playing;

                    if song.is_playing != should_play {
                        song.is_playing = should_play;
                        changed = true;
                    }
                    songs_vec.push(song);
                }
            }

            if changed {
                self.song_model.set_row_data(
                    row_idx,
                    SongRowData {
                        songs: Rc::new(VecModel::from(songs_vec)).into(),
                    },
                );
            }
        }

        let selected_visible_index = self
            .selected_song_index()
            .and_then(|song_idx| self.displayed_indices.iter().position(|idx| *idx == song_idx))
            .map(|index| index as i32)
            .unwrap_or(-1);

        if selected_visible_index != self.last_selected_visible_index {
            window.global::<AppState>().set_selected_index(selected_visible_index);
            self.last_selected_visible_index = selected_visible_index;
        }

        self.last_model_playback_path = current_path;
        self.last_model_is_playing = is_playing;
    }

    pub(crate) fn is_master_index_in_thumbnail_window(&self, window: &MainWindow, m_idx: usize) -> bool {
        let viewport_y = window.global::<AppState>().get_viewport_y_manual_songs();
        let visible_height = window.global::<AppState>().get_song_list_visible_height();
        
        let row_height = 34.0;
        let start_row = (-viewport_y / row_height).floor() as i32;
        let visible_rows = (visible_height / row_height).ceil() as i32;
        let end_row = start_row + visible_rows;

        if let Some(pos) = self.displayed_indices.iter().position(|idx| *idx == m_idx) {
            let pos = pos as i32;
            // Larger eviction window (40 items buffer) to prevent thrashing
            pos >= start_row - 40 && pos <= end_row + 40
        } else {
            false
        }
    }

    pub(crate) fn set_song_row_artwork_batch(&mut self, updates: Vec<(usize, slint::Image)>) {
        if updates.is_empty() {
            return;
        }

        for (m_idx, artwork) in updates {
            // 1. Update Songs List
            if let Some(m_idx_pos) = self.displayed_indices.iter().position(|idx| *idx == m_idx) {
                if let Some(inner_model) = self.song_row_models.get(m_idx_pos) {
                    if let Some(mut song) = inner_model.row_data(0) {
                        song.artwork = artwork.clone();
                        inner_model.set_row_data(0, song);
                        if let Some(outer_row) = self.song_model.row_data(m_idx_pos) {
                            self.song_model.set_row_data(m_idx_pos, outer_row);
                        }
                    }
                }
            }

            // 2. Update Artists Grid
            for r_idx in 0..self.artist_model.row_count() {
                if let Some(row_data) = self.artist_model.row_data(r_idx) {
                    for a_idx in 0..row_data.artists.row_count() {
                        if let Some(mut artist) = row_data.artists.row_data(a_idx) {
                            if let Some(data) = self.artists.get(artist.name.as_str()) {
                                if data.song_indices.contains(&m_idx) && artist.artwork.size().width == 0 {
                                    artist.artwork = artwork.clone();
                                    row_data.artists.set_row_data(a_idx, artist);
                                    self.artist_model.set_row_data(r_idx, row_data.clone());
                                }
                            }
                        }
                    }
                }
            }

            // 3. Update Albums Grid
            for r_idx in 0..self.album_model.row_count() {
                if let Some(row_data) = self.album_model.row_data(r_idx) {
                    for al_idx in 0..row_data.albums.row_count() {
                        if let Some(mut album) = row_data.albums.row_data(al_idx) {
                            // Find matching album data
                            let album_key = format!("{}\u{1f}{}", album.title.to_ascii_lowercase(), album.artist.to_ascii_lowercase());
                            if let Some(data) = self.albums.get(&album_key) {
                                if data.song_indices.contains(&m_idx) && album.artwork.size().width == 0 {
                                    album.artwork = artwork.clone();
                                    row_data.albums.set_row_data(al_idx, album);
                                    self.album_model.set_row_data(r_idx, row_data.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn trim_thumbnail_cache_to_visible_window(&mut self, window: &MainWindow) {
        if self.thumbnail_cache.is_empty() {
            return;
        }

        let evict_indices: Vec<usize> = self
            .thumbnail_cache
            .keys()
            .copied()
            .filter(|m_idx| !self.is_master_index_in_thumbnail_window(window, *m_idx))
            .collect();
        
        if evict_indices.is_empty() { return; }

        let mut pending_updates: Vec<(usize, slint::Image)> = Vec::new();

        for evict_idx in evict_indices {
            self.thumbnail_cache.remove(&evict_idx);
            self.thumbnail_order.retain(|idx| *idx != evict_idx);
            if let Some(entry) = self.songs.get_mut(evict_idx) {
                entry.artwork_image = slint::Image::default();
            }
            self.thumbnail_inflight.remove(&evict_idx);
            pending_updates.push((evict_idx, slint::Image::default()));
        }

        self.set_song_row_artwork_batch(pending_updates);
    }

    pub(crate) fn enqueue_visible_thumbnail_requests(&mut self, window: &MainWindow) {
        if window.global::<AppState>().get_active_nav_index() != 0 {
            return;
        }

        let viewport_y = window.global::<AppState>().get_viewport_y_manual_songs();
        let visible_height = window.global::<AppState>().get_song_list_visible_height();
        
        let row_height = 34.0;
        let start_row = (-viewport_y / row_height).floor() as usize;
        let visible_rows = (visible_height / row_height).ceil() as usize;
        let end_row = (start_row + visible_rows + 40).min(self.displayed_indices.len());
        let start_clamped = start_row.saturating_sub(20);

        let mut request_indices = Vec::new();
        
        // Tighter loading window (20 items buffer) - must be subset of eviction window
        for &m_idx in &self.displayed_indices[start_clamped..end_row] {
            if !self.thumbnail_cache.contains_key(&m_idx) && !self.thumbnail_inflight.contains(&m_idx) {
                request_indices.push(m_idx);
            }
        }

        if request_indices.is_empty() {
            return;
        }

        // Send to loader
        let sender = &self.artwork_tx;
        for m_idx in request_indices {
            if let Some(entry) = self.songs.get(m_idx) {
                if let Some(artwork_path) = entry.song.artwork.clone() {
                    let _ = sender.send(crate::ArtworkTask::LoadThumbnail {
                        m_idx,
                        artwork_path,
                        with_monochrome: self.monochrome_mode,
                    });
                    self.thumbnail_inflight.insert(m_idx);
                }
            }
        }
    }

    pub(crate) fn update_song_grid_viewport(
        &mut self,
        _viewport_y: f32,
        _visible_height: f32,
        window: &MainWindow,
    ) {
        self.enqueue_visible_thumbnail_requests(window);
        self.trim_thumbnail_cache_to_visible_window(window);
    }

    pub(crate) fn filter_by_playlist(&mut self, playlist_id: i32, window: &MainWindow) {
        if let Ok(song_ids) = orca_core::db::get_playlist_song_ids(&self.db_conn, playlist_id as i64) {
            let id_set: std::collections::HashSet<i64> = song_ids.into_iter().collect();
            self.search_query = String::new();
            self.filtered_indices = self
                .songs
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    entry.song.id.map(|id| id_set.contains(&id)).unwrap_or(false)
                })
                .map(|(idx, _)| idx)
                .collect();

            self.refresh_song_model(window);
            window.global::<AppState>().set_active_nav_index(0);
            window.global::<AppState>().set_search_text("".into());
        }
    }


    pub(crate) fn refresh_song_model(&mut self, window: &MainWindow) {
        let (current_path, is_playing) = self
            .playback_state
            .lock()
            .map(|s| (s.current_path.clone(), s.is_playing))
            .unwrap_or((None, false));

        self.displayed_indices = self
            .filtered_indices
            .iter()
            .take(GRID_MODEL_LIMIT)
            .copied()
            .collect();

        // 1. Songs List
        let mut grouped_songs = Vec::new();
        self.song_row_models.clear();
        for &m_idx in &self.displayed_indices {
            if let Some(entry) = self.songs.get(m_idx) {
                let is_current = current_path
                    .as_ref()
                    .map(|path| path == &entry.song.path)
                    .unwrap_or(false);

                let song = SongRow {
                    title: entry.song.title.clone().into(),
                    artist: entry.song.artist.clone().into(),
                    album: entry.song.album.clone().into(),
                    duration: format_duration(entry.song.duration as u64).into(),
                    artwork: self.representative_artwork(m_idx),
                    is_playing: is_current && is_playing,
                    is_selected: false,
                    master_idx: m_idx as i32,
                };
                let inner_model = Rc::new(VecModel::from(vec![song]));
                self.song_row_models.push(inner_model.clone());
                grouped_songs.push(SongRowData {
                    songs: inner_model.into(),
                });
            }
        }
        self.song_model.set_vec(grouped_songs);
        window.global::<AppState>().set_song_rows(self.song_model.clone().into());

        // 2. Playlists
        let mut pl_rows = Vec::new();
        if let Ok(playlists) = orca_core::db::get_playlists(&self.db_conn) {
            for pl in playlists {
                pl_rows.push(crate::PlaylistRow {
                    id: pl.id as i32,
                    name: pl.name.into(),
                    song_count: pl.song_count as i32,
                    cover: slint::Image::default(),
                });
            }
        }
        self.playlist_model.set_vec(pl_rows);
        window.global::<AppState>().set_playlist_rows(self.playlist_model.clone().into());

        // 3. Artists (Grid grouping: 5 per row)
        let mut artist_names: Vec<String> = self.artists.keys().cloned().collect();
        artist_names.sort();
        let mut grouped_artists = Vec::new();
        
        let chunk_size = 5;
        for chunk in artist_names.chunks(chunk_size) {
            let mut row_items = Vec::new();
            for name in chunk {
                if let Some(data) = self.artists.get(name) {
                    let artwork = if let Some(&first_song_idx) = data.song_indices.first() {
                        self.representative_artwork(first_song_idx)
                    } else {
                        slint::Image::default()
                    };
                    row_items.push(ArtistRow {
                        name: name.clone().into(),
                        song_count: data.song_indices.len() as i32,
                        artwork,
                    });
                }
            }
            grouped_artists.push(ArtistRowData {
                artists: Rc::new(VecModel::from(row_items)).into(),
            });
        }
        self.artist_model.set_vec(grouped_artists);
        window.global::<AppState>().set_artist_rows(self.artist_model.clone().into());

        // 3. Albums (Grid grouping: 5 per row)
        let mut albums_vec: Vec<&AlbumData> = self.albums.values().collect();
        albums_vec.sort_by(|a, b| {
            a.title
                .to_ascii_lowercase()
                .cmp(&b.title.to_ascii_lowercase())
                .then_with(|| a.artist.to_ascii_lowercase().cmp(&b.artist.to_ascii_lowercase()))
        });
        let mut grouped_albums = Vec::new();
        
        for chunk in albums_vec.chunks(chunk_size) {
            let mut row_items = Vec::new();
            for data in chunk {
                let artwork = if let Some(&first_song_idx) = data.song_indices.first() {
                    self.representative_artwork(first_song_idx)
                } else {
                    slint::Image::default()
                };
                row_items.push(AlbumRow {
                    title: data.title.clone().into(),
                    artist: data.artist.clone().into(),
                    song_count: data.song_indices.len() as i32,
                    artwork,
                });
            }
            grouped_albums.push(AlbumRowData {
                albums: Rc::new(VecModel::from(row_items)).into(),
            });
        }
        self.album_model.set_vec(grouped_albums);
        window.global::<AppState>().set_album_rows(self.album_model.clone().into());

        let selected_visible_index = self
            .selected_song_index()
            .and_then(|song_idx| self.displayed_indices.iter().position(|idx| *idx == song_idx))
            .map(|index| index as i32)
            .unwrap_or(-1);
        window.global::<AppState>().set_selected_index(selected_visible_index);

        self.enqueue_visible_thumbnail_requests(window);
        self.last_selected_visible_index = selected_visible_index;
        self.last_model_playback_path = current_path;
        self.last_model_is_playing = is_playing;
    }

}

