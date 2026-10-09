//! UI-independent native service data. No Slint or platform handles cross this boundary.
mod metadata;
mod playback;
pub use metadata::{
    MetadataAssetKind, MetadataDocument, MetadataEdit, MetadataFields, MetadataRequest,
};
pub use playback::{PlaybackCommand, PlaylistChange, PlaylistEdit};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub genre: String,
    pub duration_ms: u64,
    pub artwork: String,
    pub artwork_thumb: String,
    pub artwork_original: String,
    pub quality: String,
    pub track_number: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub search: String,
    pub sort: String,
    pub kind: String,
    pub key: String,
    pub secondary: String,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub total: u32,
    pub offset: u32,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default)]
pub struct Group {
    pub key: String,
    pub secondary: String,
    pub title: String,
    pub subtitle: String,
    pub artwork: String,
    pub artwork_tiles: Vec<String>,
    pub count: u32,
}

#[derive(Default)]
pub struct Snapshot {
    pub path: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    pub volume: f32,
    pub scanning: bool,
    pub scanned: u64,
    pub revision: u64,
    pub ended: u64,
    pub transitioned: u64,
    pub error: String,
    pub output_status: String,
    pub output_revision: u64,
    pub playback_error: String,
    pub playback_error_revision: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Statistics {
    pub songs: u32,
    pub artists: u32,
    pub albums: u32,
    pub genres: u32,
    pub playlists: u32,
    pub roots: Vec<String>,
}
