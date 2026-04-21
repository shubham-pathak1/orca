use crate::AppState;
use slint::ComponentHandle;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use orca_core::{db, library};

use crate::features::app_utils::{
    cached_thumbnail_path, load_songs_from_db, normalize_root_string, normalize_song_path,
    song_path_is_under_any_root,
};
use crate::{build_placeholder_song_entries, AppController, ArtworkTask, MainWindow};

impl AppController {
    pub(crate) fn set_sort_mode(&mut self, mode: i32, window: &MainWindow) {
        self.sort_mode = match mode {
            1 => crate::LibrarySortMode::Artist,
            2 => crate::LibrarySortMode::Album,
            3 => crate::LibrarySortMode::Year,
            4 => crate::LibrarySortMode::Track,
            _ => crate::LibrarySortMode::Title,
        };
        window.global::<AppState>().set_sort_mode(self.sort_mode as i32);
        window.global::<AppState>().set_sort_mode_label(self.sort_mode.label().into());
        self.refresh_filter();
        self.refresh_song_model(window);
        self.persist_preferences();
    }

    fn apply_sort_to_filtered(&mut self) {
        let songs = &self.songs;
        let mode = self.sort_mode;
        self.filtered_indices.sort_by(|a, b| {
            let sa = &songs[*a].song;
            let sb = &songs[*b].song;

            let cmp = match mode {
                crate::LibrarySortMode::Title => sa.title.to_ascii_lowercase().cmp(&sb.title.to_ascii_lowercase()),
                crate::LibrarySortMode::Artist => canonical_artist_key(&sa.artist).cmp(&canonical_artist_key(&sb.artist)),
                crate::LibrarySortMode::Album => {
                    let a_album = normalize_unknown_text(&sa.album, "Unknown Album").to_ascii_lowercase();
                    let b_album = normalize_unknown_text(&sb.album, "Unknown Album").to_ascii_lowercase();
                    a_album
                        .cmp(&b_album)
                        .then_with(|| canonical_artist_key(&sa.album_artist).cmp(&canonical_artist_key(&sb.album_artist)))
                }
                crate::LibrarySortMode::Year => sa.year.unwrap_or(i32::MAX).cmp(&sb.year.unwrap_or(i32::MAX)),
                crate::LibrarySortMode::Track => sa
                    .disc_number
                    .unwrap_or(i32::MAX)
                    .cmp(&sb.disc_number.unwrap_or(i32::MAX))
                    .then_with(|| sa.track_number.unwrap_or(i32::MAX).cmp(&sb.track_number.unwrap_or(i32::MAX))),
            };

            cmp.then_with(|| sa.title.to_ascii_lowercase().cmp(&sb.title.to_ascii_lowercase()))
                .then_with(|| sa.path.cmp(&sb.path))
        });
    }

    pub(crate) fn set_compact_library_mode(&mut self, enabled: bool, window: &MainWindow) {
        self.compact_library_mode = enabled;
        window.global::<AppState>().set_compact_library_mode(enabled);

        if enabled {
            self.thumbnail_cache.clear();
            self.thumbnail_order.clear();
            self.thumbnail_inflight.clear();
        }

        self.refresh_song_model(window);
        self.persist_preferences();
    }

    pub(crate) fn refresh_playlists_ui(&mut self, window: &MainWindow) {
        let mut rows = Vec::new();
        if let Ok(playlists) = db::get_playlists(&self.db_conn) {
            for playlist in playlists {
                let cover = playlist
                    .cover_path
                    .as_deref()
                    .and_then(|path| slint::Image::load_from_path(Path::new(path)).ok())
                    .unwrap_or_else(slint::Image::default);

                rows.push(crate::PlaylistRow {
                    id: playlist.id as i32,
                    name: playlist.name.into(),
                    song_count: playlist.song_count as i32,
                    cover,
                });
            }
        }
        self.playlist_model.set_vec(rows);
        window.global::<AppState>().set_playlist_rows(self.playlist_model.clone().into());
    }

    pub(crate) fn create_playlist(&mut self, name: String, window: &MainWindow) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            self.set_status("Playlist name cannot be empty.", window);
            return;
        }

        if self.playlist_name_exists(trimmed, None) {
            self.set_status("A playlist with that name already exists.", window);
            return;
        }

        match db::create_playlist(&self.db_conn, trimmed, None) {
            Ok(_) => {
                self.refresh_playlists_ui(window);
                self.set_temporary_status(
                    format!("Created playlist: {trimmed}"),
                    window,
                    std::time::Duration::from_secs(2),
                );
            }
            Err(err) => self.set_status(format!("Failed creating playlist: {err}"), window),
        }
    }

    pub(crate) fn rename_playlist(&mut self, playlist_id: i32, new_name: String, window: &MainWindow) {
        if playlist_id <= 0 {
            self.set_status("Invalid playlist id.", window);
            return;
        }

        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            self.set_status("Playlist name cannot be empty.", window);
            return;
        }

        if self.playlist_name_exists(trimmed, Some(playlist_id as i64)) {
            self.set_status("A playlist with that name already exists.", window);
            return;
        }

        match db::rename_playlist(&self.db_conn, playlist_id as i64, trimmed) {
            Ok(_) => {
                self.refresh_playlists_ui(window);
                self.set_temporary_status(
                    format!("Renamed playlist to: {trimmed}"),
                    window,
                    std::time::Duration::from_secs(2),
                );
            }
            Err(err) => self.set_status(format!("Failed renaming playlist: {err}"), window),
        }
    }

    pub(crate) fn delete_playlist(&mut self, playlist_id: i32, window: &MainWindow) {
        if playlist_id <= 0 {
            self.set_status("Invalid playlist id.", window);
            return;
        }

        match db::delete_playlist(&self.db_conn, playlist_id as i64) {
            Ok(_) => {
                self.refresh_playlists_ui(window);
                self.set_temporary_status(
                    "Playlist deleted",
                    window,
                    std::time::Duration::from_secs(2),
                );
            }
            Err(err) => self.set_status(format!("Failed deleting playlist: {err}"), window),
        }
    }

    fn playlist_name_exists(&self, candidate: &str, exclude_id: Option<i64>) -> bool {
        let wanted = candidate.trim().to_ascii_lowercase();
        db::get_playlists(&self.db_conn)
            .ok()
            .map(|playlists| {
                playlists.into_iter().any(|p| {
                    if Some(p.id) == exclude_id {
                        return false;
                    }
                    p.name.trim().to_ascii_lowercase() == wanted
                })
            })
            .unwrap_or(false)
    }

    pub(crate) fn change_playlist_cover(&mut self, playlist_id: i32, window: &MainWindow) {
        if playlist_id <= 0 {
            self.set_status("Invalid playlist id.", window);
            return;
        }

        let picked = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_file();

        let Some(path) = picked else {
            return;
        };

        let cover_path = path.to_string_lossy().to_string();
        match db::update_playlist_cover(&self.db_conn, playlist_id as i64, Some(&cover_path)) {
            Ok(_) => {
                self.refresh_playlists_ui(window);
                self.set_temporary_status(
                    "Playlist cover updated",
                    window,
                    std::time::Duration::from_secs(2),
                );
            }
            Err(err) => self.set_status(format!("Failed updating cover: {err}"), window),
        }
    }

    pub(crate) fn refresh_scan_roots_ui(&mut self, window: &MainWindow) {
        let roots = self
            .load_scan_roots()
            .into_iter()
            .map(|p| slint::SharedString::from(p.to_string_lossy().to_string()))
            .collect::<Vec<_>>();
        self.scan_roots_model.set_vec(roots);
        window.global::<AppState>().set_scan_roots(self.scan_roots_model.clone().into());
    }

    pub(crate) fn set_app_blur_enabled(&mut self, enabled: bool, window: &MainWindow) {
        self.app_blur_enabled = enabled;
        if !enabled {
            self.blur_cache.clear();
            self.blur_cache_order.clear();
        }
        window.global::<AppState>().set_blur_enabled(enabled);
        self.persist_preferences();
    }

    pub(crate) fn schedule_thumbnail_warmup(&mut self) -> usize {
        let mut queued = 0usize;
        let mut seen_artwork = HashSet::new();

        for entry in &self.songs {
            let Some(artwork_path) = entry.song.artwork.clone() else {
                continue;
            };
            if !seen_artwork.insert(artwork_path.clone()) {
                continue;
            }

            if self
                .artwork_tx
                .try_send(ArtworkTask::WarmCache { artwork_path })
                .is_ok()
            {
                queued += 1;
            } else {
                break;
            }
        }

        self.total_thumbnails = queued;
        self.processed_thumbnails = 0;
        queued
    }

    pub(crate) fn rebuild_group_indexes(&mut self) {
        self.artists.clear();
        self.albums.clear();

        for (idx, entry) in self.songs.iter().enumerate() {
            let artist_name = primary_artist_display(&entry.song.artist);
            let artist_key = canonical_artist_key(&entry.song.artist);

            self.artists
                .entry(artist_key)
                .or_default()
                .song_indices
                .push(idx);

            let album_title = {
                let trimmed = entry.song.album.trim();
                let is_unknown = trimmed.is_empty() || {
                    let lower = trimmed.to_ascii_lowercase();
                    lower == "unknown" || lower == "none" || lower == "null" || lower == "n/a" || lower == "na" || lower == "-" || lower == "?"
                };

                if is_unknown {
                    let path = Path::new(&entry.song.path);
                    if let Some(parent) = path.parent() {
                        let _parent_path = parent.to_string_lossy().to_string();
                        let roots = self.load_scan_roots();
                        let is_root = roots.iter().any(|r| {
                            let r_norm = crate::features::app_utils::normalize_root_string(r);
                            let p_norm = crate::features::app_utils::normalize_root_string(parent);
                            r_norm == p_norm
                        });

                        if is_root {
                            "[Singles]".to_string()
                        } else {
                            parent.file_name()
                                .map(|os| os.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Unknown Album".to_string())
                        }
                    } else {
                        "Unknown Album".to_string()
                    }
                } else {
                    trimmed.to_string()
                }
            };

            let album_artist = {
                let normalized = normalize_unknown_text(&entry.song.album_artist, "Unknown Artist");
                if normalized == "Unknown Artist" {
                    artist_name.clone()
                } else {
                    primary_artist_display(&normalized)
                }
            };
            let album_key = format!(
                "{}\u{1f}{}",
                album_title.to_ascii_lowercase(),
                album_artist.to_ascii_lowercase()
            );

            let album = self.albums.entry(album_key).or_default();
            if album.title.is_empty() {
                album.title = album_title;
            }
            if album.artist.is_empty() {
                album.artist = album_artist;
            }
            album.song_indices.push(idx);
        }

        // Ensure stable artist display keys preserve readable primary names.
        let mut remapped = std::collections::HashMap::new();
        for (key, data) in std::mem::take(&mut self.artists) {
            let display = if key.trim().is_empty() {
                "Unknown Artist".to_string()
            } else {
                key
            };
            remapped.insert(display, data);
        }
        self.artists = remapped;
    }

    pub(crate) fn selected_song_index(&self) -> Option<usize> {
        self.selected_filtered_index
            .and_then(|row| self.filtered_indices.get(row).copied())
    }

    pub(crate) fn select_song_in_filtered(&mut self, song_idx: usize) {
        self.selected_filtered_index = self
            .filtered_indices
            .iter()
            .position(|idx| *idx == song_idx);
    }

    pub(crate) fn refresh_filter(&mut self) {
        let previous = self.selected_song_index();
        let query = self.search_query.trim();

        self.filtered_indices = if query.is_empty() {
            (0..self.songs.len()).collect()
        } else {
            self.songs
                .iter()
                .enumerate()
                .filter_map(|(idx, entry)| {
                    if matches_song_query(&entry.song, query) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect()
        };

            self.apply_sort_to_filtered();

        if self.filtered_indices.is_empty() {
            self.selected_filtered_index = None;
            return;
        }

        if let Some(prev_song_idx) = previous {
            if let Some(new_row) = self
                .filtered_indices
                .iter()
                .position(|idx| *idx == prev_song_idx)
            {
                self.selected_filtered_index = Some(new_row);
                return;
            }
        }

        self.selected_filtered_index = Some(0);
    }

    pub(crate) fn apply_search(&mut self, query: String, window: &MainWindow) {
        if self.search_query == query {
            return;
        }
        self.search_query = query;
        self.refresh_filter();
        window.global::<AppState>().set_search_text(self.search_query.clone().into());
        self.refresh_song_model(window);
        self.persist_preferences();
    }

    pub(crate) fn filter_by_artist(&mut self, artist: String, window: &MainWindow) {
        window.global::<AppState>().set_active_nav_index(0);
        self.apply_search(artist.clone(), window);
        self.set_status(format!("Filtered by artist: {artist}"), window);
    }

    pub(crate) fn filter_by_album(&mut self, title: String, artist: String, window: &MainWindow) {
        window.global::<AppState>().set_active_nav_index(0);
        self.filtered_indices = self
            .songs
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                if normalize_unknown_text(&entry.song.album, "Unknown Album").eq_ignore_ascii_case(&title)
                    && primary_artist_display(&entry.song.album_artist).eq_ignore_ascii_case(&artist)
                {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        // Album drill-down should respect track order by disc/track.
        self.filtered_indices.sort_by(|a, b| {
            let sa = &self.songs[*a].song;
            let sb = &self.songs[*b].song;
            sa.disc_number
                .unwrap_or(i32::MAX)
                .cmp(&sb.disc_number.unwrap_or(i32::MAX))
                .then_with(|| sa.track_number.unwrap_or(i32::MAX).cmp(&sb.track_number.unwrap_or(i32::MAX)))
                .then_with(|| sa.title.to_ascii_lowercase().cmp(&sb.title.to_ascii_lowercase()))
        });

        self.search_query = format!("album:{} artist:{}", title, artist);
        window.global::<AppState>().set_search_text(self.search_query.clone().into());
        if self.filtered_indices.is_empty() {
            self.selected_filtered_index = None;
        } else {
            self.selected_filtered_index = Some(0);
        }
        self.refresh_song_model(window);
        self.set_status(format!("Filtered by album: {title}"), window);
    }

    pub(crate) fn clear_filter(&mut self, window: &MainWindow) {
        if self.search_query.is_empty() {
            return;
        }
        self.apply_search(String::new(), window);
        self.set_status("Filter cleared", window);
    }

    pub(crate) fn load_scan_roots(&self) -> Vec<PathBuf> {
        db::get_setting(&self.db_conn, crate::SETTING_LIBRARY_SCAN_ROOTS)
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .unwrap_or_default()
            .into_iter()
            .map(PathBuf::from)
            .filter(|path| path.exists() && path.is_dir())
            .collect()
    }

    pub(crate) fn infer_scan_roots_from_library(&self) -> Vec<PathBuf> {
        let mut dirs = self
            .songs
            .iter()
            .filter_map(|entry| Path::new(&entry.song.path).parent().map(|p| p.to_path_buf()))
            .collect::<Vec<_>>();
        dirs.sort();
        dirs.dedup();
        dirs.sort_by_key(|p| p.components().count());

        let mut roots = Vec::new();
        for dir in dirs {
            if roots.iter().any(|root: &PathBuf| dir.starts_with(root)) {
                continue;
            }
            roots.push(dir);
        }
        roots
    }

    pub(crate) fn persist_scan_roots(&self, roots: &[PathBuf]) {
        let encoded_roots = roots
            .iter()
            .map(|path| normalize_root_string(path))
            .collect::<Vec<_>>();
        if let Ok(encoded) = serde_json::to_string(&encoded_roots) {
            let _ = db::set_setting(&self.db_conn, crate::SETTING_LIBRARY_SCAN_ROOTS, &encoded);
        }
    }

    pub(crate) fn reload_songs_from_db(&mut self) -> Result<(), String> {
        let previous_current_path = self
            .current_song_index
            .and_then(|idx| self.songs.get(idx).map(|entry| entry.song.path.clone()));

        self.songs = build_placeholder_song_entries(load_songs_from_db(&self.db_conn)?);

        self.current_song_index = previous_current_path
            .and_then(|path| self.songs.iter().position(|entry| entry.song.path == path));

        self.refresh_filter();
        self.rebuild_group_indexes();
        if let Some(song_idx) = self.current_song_index {
            self.select_song_in_filtered(song_idx);
        }
        Ok(())
    }

    pub(crate) fn remove_scan_root(&mut self, root: String, window: &MainWindow) {
        if self.scan_in_progress {
            self.set_status("Cannot remove folders while scan is in progress.", window);
            return;
        }

        let mut roots = self.load_scan_roots();
        let target_norm = normalize_root_string(Path::new(&root));
        let before_roots = roots.len();
        roots.retain(|r| normalize_root_string(r) != target_norm);
        if roots.len() == before_roots {
            self.set_status("Folder not found in scan roots.", window);
            return;
        }

        let removed_artwork_paths = self
            .songs
            .iter()
            .filter(|entry| !song_path_is_under_any_root(&entry.song.path, &roots))
            .filter_map(|entry| entry.song.artwork.clone())
            .collect::<HashSet<_>>();

        self.persist_scan_roots(&roots);
        self.refresh_scan_roots_ui(window);

        let to_remove_paths = self
            .songs
            .iter()
            .filter(|entry| !song_path_is_under_any_root(&entry.song.path, &roots))
            .map(|entry| entry.song.path.clone())
            .collect::<Vec<_>>();

        for path in &to_remove_paths {
            let _ = db::delete_song_by_path(&self.db_conn, path);
        }

        for art in removed_artwork_paths {
            let cache_path = cached_thumbnail_path(
                &self.artwork_dir.join(format!("thumbs-{}", crate::THUMB_SIZE)),
                crate::THUMB_SIZE,
                &art,
            );
            let _ = std::fs::remove_file(cache_path);
        }

        self.thumbnail_cache.clear();
        self.thumbnail_order.clear();
        self.thumbnail_inflight.clear();

        if let Err(err) = self.reload_songs_from_db() {
            self.set_status(format!("Failed reloading library: {err}"), window);
            return;
        }

        self.refresh_song_model(window);
        self.update_now_playing(window);
        self.set_temporary_status(
            format!(
                "Removed folder. {} track(s) removed from library.",
                to_remove_paths.len()
            ),
            window,
            std::time::Duration::from_secs(3),
        );
    }

    pub(crate) fn jump_to_letter(&mut self, letter: String, window: &MainWindow) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let letter_upper = letter.to_uppercase();
        let mut target_index: Option<usize> = None;

        for (row_idx, &master_idx) in self.filtered_indices.iter().enumerate() {
            let song = &self.songs[master_idx].song;
            let title = song.title.trim().to_uppercase();
            
            if letter == "#" {
                if !title.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                    target_index = Some(row_idx);
                    break;
                }
            } else if title.starts_with(&letter_upper) {
                target_index = Some(row_idx);
                break;
            }
        }

        if let Some(idx) = target_index {
            // Row height is 36px in SongListView
            let target_y = -(idx as f32 * 36.0);
            self.scroller.stop();
            window.global::<AppState>().set_viewport_y_manual_songs(target_y.into());
            
            // Trigger viewport update for thumbnail hydration
            self.update_song_grid_viewport(target_y, window.global::<AppState>().get_song_list_visible_height(), window);
        }
    }

    pub(crate) fn scan_saved_roots(&mut self, window: &MainWindow) {
        if self.scan_in_progress {
            self.set_status("Library scan already in progress...", window);
            return;
        }

        let mut scan_roots = self.load_scan_roots();
        if scan_roots.is_empty() {
            scan_roots = self.infer_scan_roots_from_library();
            if !scan_roots.is_empty() {
                self.persist_scan_roots(&scan_roots);
            }
        }
        if scan_roots.is_empty() {
            self.set_status(
                "No known library folders. Import one from desktop first.",
                window,
            );
            return;
        }

        self.refresh_scan_roots_ui(window);

        if let Err(err) = std::fs::create_dir_all(&self.artwork_dir) {
            self.set_status(
                format!("Rescan failed: artwork directory error ({err})"),
                window,
            );
            return;
        }

        self.scan_in_progress = true;
        self.set_status("Rescanning library in background...", window);

        let artwork_dir = self.artwork_dir.clone();
        let scan_tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            let mut unique_scanned = Vec::new();
            let mut seen_paths = HashSet::new();
            let mut scanned_roots = 0usize;

            for root in &scan_roots {
                match library::scan_music_folder(root, &artwork_dir) {
                    Ok(scanned) => {
                        scanned_roots += 1;
                        for mut song in scanned {
                            song.path = normalize_song_path(Path::new(&song.path));
                            if seen_paths.insert(song.path.clone()) {
                                unique_scanned.push(song);
                            }
                        }
                    }
                    Err(err) => {
                        let _ = scan_tx.send(crate::ScanWorkResult::Error(format!(
                            "Rescan failed for {}: {err}",
                            root.display()
                        )));
                        return;
                    }
                }
            }

            if scanned_roots == 0 {
                let _ = scan_tx.send(crate::ScanWorkResult::Error(
                    "No valid scan roots were found.".to_string(),
                ));
                return;
            }

            let scanned_root_norms = scan_roots
                .iter()
                .map(|root| normalize_root_string(root))
                .collect::<Vec<_>>();

            let _ = scan_tx.send(crate::ScanWorkResult::Success {
                scanned_roots,
                scanned_root_norms,
                unique_scanned,
            });
        });
    }
}

pub(crate) fn matches_song_query(song: &orca_core::library::LocalSong, query: &str) -> bool {
    query
        .split_whitespace()
        .all(|token| token_matches(song, token))
}

fn token_matches(song: &orca_core::library::LocalSong, token: &str) -> bool {
    let token = token.to_lowercase();
    if let Some(value) = token.strip_prefix("artist:") {
        return fuzzy_contains(song.artist.to_lowercase().as_str(), value);
    }
    if let Some(value) = token.strip_prefix("albumartist:") {
        return fuzzy_contains(song.album_artist.to_lowercase().as_str(), value);
    }
    if let Some(value) = token.strip_prefix("title:") {
        return fuzzy_contains(song.title.to_lowercase().as_str(), value);
    }
    if let Some(value) = token.strip_prefix("album:") {
        return fuzzy_contains(song.album.to_lowercase().as_str(), value);
    }
    if let Some(value) = token.strip_prefix("path:") {
        return fuzzy_contains(song.path.to_lowercase().as_str(), value);
    }
    if token.contains(':') {
        return false;
    }

    fuzzy_contains(song.title.to_lowercase().as_str(), token.as_str())
        || fuzzy_contains(song.artist.to_lowercase().as_str(), token.as_str())
        || fuzzy_contains(song.album_artist.to_lowercase().as_str(), token.as_str())
        || fuzzy_contains(song.album.to_lowercase().as_str(), token.as_str())
}

fn normalize_unknown_text(value: &str, unknown: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return unknown.to_string();
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower == "unknown"
        || lower == "none"
        || lower == "null"
        || lower == "n/a"
        || lower == "na"
        || lower == "-"
        || lower == "?"
    {
        return unknown.to_string();
    }

    trimmed.to_string()
}

fn primary_artist_display(value: &str) -> String {
    let normalized = normalize_unknown_text(value, "Unknown Artist");
    let lower = normalized.to_ascii_lowercase();
    let mut cut = normalized.len();

    for sep in [" feat.", " ft.", " featuring ", " & ", " and ", ";", "/", " x ", ","] {
        if let Some(pos) = lower.find(sep) {
            cut = cut.min(pos);
        }
    }

    let candidate = normalized[..cut].trim();
    if candidate.is_empty() {
        "Unknown Artist".to_string()
    } else {
        candidate.to_string()
    }
}

fn canonical_artist_key(value: &str) -> String {
    primary_artist_display(value).to_ascii_lowercase()
}

fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() || haystack.contains(needle) {
        return true;
    }

    let mut iter = needle.chars();
    let mut current = iter.next();
    for ch in haystack.chars() {
        if current == Some(ch) {
            current = iter.next();
            if current.is_none() {
                return true;
            }
        }
    }
    false
}
