mod metadata;
mod scan;
pub use metadata::{
    remove_song_cover, replace_song_cover, update_collection_name_checked, update_song_metadata,
    update_song_metadata_checked, update_song_metadata_cover_action,
    update_song_metadata_with_cover,
};
pub use scan::{scan_music_file, scan_music_file_with_metadata};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalSong {
    pub id: Option<i64>,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<String>,
    pub duration: u32,
    pub artwork: Option<String>,
    pub artwork_thumb: Option<String>,
    pub artwork_preview: Option<String>,
    pub lyrics: Option<String>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub format: Option<String>,
    pub modified_at: Option<i64>,
    pub file_size: Option<u64>,
}

#[derive(Deserialize)]
pub struct SongMetadataUpdate {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub genre: Option<String>,
    pub lyrics: Option<String>,
}
