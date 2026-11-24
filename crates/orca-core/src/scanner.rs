use std::collections::HashMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::library::{scan_music_file_with_metadata, LocalSong};

pub fn scan_music_folder<F, G>(
    folder_path: &Path,
    artwork_dir: &Path,
    existing_map: &HashMap<String, (i64, u64, LocalSong)>,
    on_progress: F,
    on_song_indexed: G,
) -> Result<Vec<LocalSong>, String>
where
    F: Fn() + Send + Sync,
    G: Fn(LocalSong) + Send + Sync,
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
                let current_mtime = modification_time(&metadata);

                if current_size == *stored_size && current_mtime == Some(*stored_mtime) {
                    songs.push(cached_song.clone());
                    on_progress();
                    continue;
                }
            }
        }

        if let Ok(song) = scan_music_file_with_metadata(path, artwork_dir, |metadata| {
            let mut indexed_song = metadata.clone();
            // Embedded lyrics can be large and are not needed to populate library rows.
            indexed_song.lyrics = None;
            on_song_indexed(indexed_song);
        }) {
            songs.push(song);
            on_progress();
        }
    }

    Ok(songs)
}

/// Nanoseconds relative to the Unix epoch, preserving the filesystem's precision.
pub fn modification_time(metadata: &fs::Metadata) -> Option<i64> {
    let time = metadata.modified().ok()?;
    match time.duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_nanos()).ok(),
        Err(error) => i64::try_from(error.duration().as_nanos())
            .ok()
            .map(|value| -value),
    }
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    let Some(extension) = path.extension() else {
        return false;
    };

    matches!(
        extension.to_string_lossy().to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "m4a" | "wav" | "ogg" | "opus" | "aiff" | "aif"
    )
}

/// Identify a file across renames on the same volume.
#[cfg(windows)]
pub fn file_identity(path: &Path, _: &fs::Metadata) -> Option<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let file = fs::File::open(path).ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // The open file owns the handle for the lifetime of this call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return None;
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    (index != 0).then(|| format!("{}:{index}", info.dwVolumeSerialNumber))
}

#[cfg(unix)]
pub fn file_identity(_: &Path, metadata: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    (metadata.ino() != 0).then(|| format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(any(windows, unix)))]
pub fn file_identity(_: &Path, _: &fs::Metadata) -> Option<String> {
    None
}
