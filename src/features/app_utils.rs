use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use orca_core::library::LocalSong;

pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    format!("{minutes:02}:{seconds:02}")
}

pub fn load_songs_from_db(conn: &rusqlite::Connection) -> Result<Vec<LocalSong>, String> {
    let all_songs = orca_core::db::get_all_songs(conn)?;
    
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    
    for song in all_songs {
        let normalized = normalize_song_path(Path::new(&song.path));
        if seen.insert(normalized) {
            deduped.push(song);
        }
    }

    Ok(deduped)
}

pub fn get_app_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("io.github.shubhampathak1.orca")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".local/share/io.github.shubhampathak1.orca")
    }
}

pub fn get_artwork_dir() -> PathBuf {
    get_app_data_dir().join("artwork")
}

pub fn normalize_root_string(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = canonical.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let slash = value.replace('\\', "/");
        let without_prefix = slash.strip_prefix("//?/").unwrap_or(&slash);
        without_prefix.to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        value
    }
}

pub fn normalize_song_path(path: &Path) -> String {
    normalize_root_string(path)
}

pub fn thumbnail_cache_key(artwork_path: &str, thumb_size: u32) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    artwork_path.hash(&mut hasher);
    thumb_size.hash(&mut hasher);

    if let Ok(meta) = std::fs::metadata(artwork_path) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            if let Ok(delta) = modified.duration_since(std::time::UNIX_EPOCH) {
                delta.as_secs().hash(&mut hasher);
                delta.subsec_nanos().hash(&mut hasher);
            }
        }
    }

    format!("{:016x}", hasher.finish())
}

pub fn cached_thumbnail_path(artwork_dir: &Path, thumb_size: u32, artwork_path: &str) -> PathBuf {
    artwork_dir.join(format!("{}.bin", thumbnail_cache_key(artwork_path, thumb_size)))
}

pub fn song_path_is_under_any_root(song_path: &str, roots: &[PathBuf]) -> bool {
    let normalized_song = normalize_song_path(Path::new(song_path));
    roots.iter()
        .map(|root| normalize_root_string(root))
        .any(|root| normalized_song.starts_with(&root))
}
