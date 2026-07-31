use std::collections::HashMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::library::{scan_music_file, LocalSong};

pub fn scan_music_folder<F>(
    folder_path: &Path,
    artwork_dir: &Path,
    existing_map: &HashMap<String, (i64, u64, LocalSong)>,
    on_progress: F,
) -> Result<Vec<LocalSong>, String>
where
    F: Fn() + Send + Sync,
{
    let mut songs = Vec::new();

    for entry in WalkDir::new(folder_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file() || !is_supported_audio_file(path) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if let Some((stored_mtime, stored_size, cached_song)) = existing_map.get(&path_str) {
            if let Ok(metadata) = fs::metadata(path) {
                let current_size = metadata.len();
                let current_mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0);

                if current_size == *stored_size && current_mtime == *stored_mtime {
                    songs.push(cached_song.clone());
                    on_progress();
                    continue;
                }
            }
        }

        if let Ok(song) = scan_music_file(path, artwork_dir) {
            songs.push(song);
            on_progress();
        }
    }

    Ok(songs)
}

fn is_supported_audio_file(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };

    matches!(
        extension.to_string_lossy().to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "m4a" | "wav" | "ogg" | "opus" | "aiff" | "aif"
    )
}
