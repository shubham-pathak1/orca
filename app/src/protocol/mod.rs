//! Messages crossing the UI/service thread boundary. No Slint handles or borrowed UI data.
mod actions;
pub use actions::{CollectionAction, PlayerAction, QueueAction};
pub use orca_services::operation_types::{OperationError, OperationRequest, OperationResult};
use orca_services::types::{Group, Query, Snapshot, Statistics, Track};
pub use orca_services::types::{MetadataAssetKind, MetadataDocument, MetadataRequest};
pub use orca_services::types::{PlaylistChange, PlaylistEdit};
use orca_services::FolderNode;
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
    FolderPreview {
        root: String,
        current: String,
        adding: bool,
    },
    AddFolder(String, Vec<(String, String)>),
    SetFolderRule {
        root: String,
        path: String,
        mode: String,
    },
    Refresh,
    RefreshLyrics,
    Reconcile,
    ArtworkRefresh(u64, Vec<u32>),
    SourceChanged,
    CollectionDraft(String),
    Queue(QueueAction),
    Operation(OperationRequest),
    Metadata(MetadataRequest),
    Context(String),
    Playlist(PlaylistEdit),
    PlaylistChoices(i64),
    LikedStatus(i64),
    ToggleLiked(i64),
    Settings(Settings),
    Font(String),
    Shutdown,
}
pub enum Event {
    Page(u64, u32, u32, Vec<Track>),
    PagesSkipped(u64, Vec<u32>),
    Groups(u64, Vec<Group>),
    Statistics(Statistics),
    FolderChildren {
        root: String,
        current: String,
        adding: bool,
        open: bool,
        nodes: Vec<FolderNode>,
    },
    Playback(Snapshot),
    Now(Track),
    GroupGone(u64, String, String, String),
    CollectionDraft(String),
    Queue(Vec<Track>),
    Neighbors(String, Vec<Track>),
    Modes(bool, u8),
    Analysis(u64, String, String, Vec<f32>),
    Lyrics(u64, String, String, bool),
    Metadata(MetadataDocument, bool, i32),
    MetadataAsset(MetadataAssetKind, String, String, i32),
    Context(Track),
    Playlists(Vec<Group>),
    PlaylistChoices(i64, Vec<Group>, Vec<i64>),
    LikedStatus(i64, bool),
    Jump(u64, u32),
    Operation(OperationRequest, OperationResult),
    CatalogDetail(u64, OperationRequest, OperationResult),
    CatalogError(u64, String),
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
    pub lyrics_left_aligned: bool,
    pub show_liked_songs: bool,
    pub dynamic_accent: bool,
    pub blurred_background: bool,
    pub full_player_background: String,
    pub gapless: bool,
    pub replaygain: bool,
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
            lyrics_left_aligned: false,
            show_liked_songs: true,
            dynamic_accent: true,
            blurred_background: true,
            full_player_background: String::new(),
            gapless: true,
            replaygain: false,
            auto_artwork: false,
            icon_sidebar: true,
            seek_style: "waveform".into(),
            font_family: "Plus Jakarta Sans".into(),
            font_path: String::new(),
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::Settings;

    #[test]
    fn lyrics_alignment_defaults_centered_and_survives_settings_round_trip() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.lyrics_left_aligned);
        settings.lyrics_left_aligned = true;
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert!(restored.lyrics_left_aligned);
    }

    #[test]
    fn replaygain_defaults_off_and_survives_settings_round_trip() {
        let legacy: Settings = serde_json::from_str("{}").unwrap();
        assert!(!legacy.replaygain);

        let mut enabled = legacy;
        enabled.replaygain = true;
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&enabled).unwrap()).unwrap();
        assert!(restored.replaygain);
    }
}
