//! Messages crossing the UI/service thread boundary. No Slint handles or borrowed UI data.
mod actions;
pub use actions::{CollectionAction, PlayerAction, QueueAction};
pub use orca_services::operation_types::{OperationError, OperationRequest, OperationResult};
use orca_services::types::{Group, Query, Snapshot, Statistics, Track};
pub use orca_services::types::{MetadataAssetKind, MetadataDocument, MetadataRequest};
pub use orca_services::types::{PlaylistChange, PlaylistEdit};
use serde::{Deserialize, Serialize};
pub enum Request {
    Browse(u64, Query, String),
    Page(u64, u32),
    Play(String, bool, Query),
    Collection(CollectionAction, Query),
    Command(PlayerAction),
    Seek(f64),
    Volume(f64),
    Folder(String),
    Refresh,
    Reconcile,
    SourceChanged,
    CollectionDraft(String),
    Queue(QueueAction),
    Operation(OperationRequest),
    Metadata(MetadataRequest),
    Context(String),
    Playlist(PlaylistEdit),
    Settings(Settings),
    Font(String),
    Shutdown,
}
pub enum Event {
    Page(u64, u32, u32, Vec<Track>),
    Groups(u64, Vec<Group>),
    Statistics(Statistics),
    Playback(Snapshot),
    Now(Track),
    GroupGone(String, String, String),
    CollectionDraft(String),
    Queue(Vec<Track>),
    Neighbors(String, Vec<Track>),
    Modes(bool, u8),
    Analysis(u64, String, String, Vec<f32>),
    Lyrics(u64, String, String),
    Metadata(MetadataDocument, bool, i32),
    MetadataAsset(MetadataAssetKind, String, String, i32),
    Context(Track),
    Playlists(Vec<Group>),
    Jump(u64, u32),
    Operation(OperationRequest, OperationResult),
    OperationFailed(OperationRequest, OperationError),
    MetadataSaved(String, i32),
    MetadataFailed(i32, String),
    PlaylistChanged(PlaylistChange),
    AutomaticArtworkUpdated,
    Error(String),
    Ready,
    LibraryChanged,
    Font(String, Vec<u8>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub light_theme: bool,
    pub browse_artwork: bool,
    pub compact_artwork: bool,
    pub full_artwork: bool,
    pub grid: bool,
    pub folder_grid: bool,
    pub sort: String,
    pub scale: f32,
    pub quality_info: bool,
    pub dynamic_accent: bool,
    pub blurred_background: bool,
    pub gapless: bool,
    pub auto_artwork: bool,
    pub icon_sidebar: bool,
    pub seek_style: String,
    pub font_family: String,
    pub font_path: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            light_theme: false,
            browse_artwork: true,
            compact_artwork: true,
            full_artwork: true,
            grid: true,
            folder_grid: true,
            sort: "artist".into(),
            scale: 1.0,
            quality_info: true,
            dynamic_accent: true,
            blurred_background: true,
            gapless: true,
            auto_artwork: false,
            icon_sidebar: true,
            seek_style: "waveform".into(),
            font_family: "Plus Jakarta Sans".into(),
            font_path: String::new(),
        }
    }
}
