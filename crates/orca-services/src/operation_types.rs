//! Owned operation contracts shared by the native service and its callers.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionKind {
    Artists,
    Albums,
    Genres,
    Playlists,
    Folders,
}
impl CollectionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Artists => "artists",
            Self::Albums => "albums",
            Self::Genres => "genres",
            Self::Playlists => "playlists",
            Self::Folders => "folders",
        }
    }
}
impl TryFrom<&str> for CollectionKind {
    type Error = OperationError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "artists" => Ok(Self::Artists),
            "albums" => Ok(Self::Albums),
            "genres" => Ok(Self::Genres),
            "playlists" => Ok(Self::Playlists),
            "folders" => Ok(Self::Folders),
            _ => Err(OperationError::InvalidRequest(
                "Unknown collection type".into(),
            )),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionIdentity {
    pub kind: CollectionKind,
    pub key: String,
    #[serde(default)]
    pub secondary: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionEdit {
    #[serde(flatten)]
    pub identity: CollectionIdentity,
    pub name: Option<String>,
    pub cover: Option<String>,
    pub draft_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionPreview {
    #[serde(flatten)]
    pub identity: CollectionIdentity,
    #[serde(default)]
    pub collection_seed: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverFetch {
    pub kind: CollectionKind,
    pub key: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub title: Option<String>,
    pub track_artist: Option<String>,
    pub duration_ms: Option<u64>,
    pub editor_generation: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_path: Option<String>,
    #[serde(default)]
    pub collection_draft: bool,
    #[serde(default)]
    pub collection_seed: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsFetch {
    pub key: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(default = "cache_default")]
    pub cache: bool,
    pub editor_path: Option<String>,
    pub editor_generation: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverKind {
    Artists,
    Albums,
    Playlists,
    Song,
}
impl CoverKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Artists => "artists",
            Self::Albums => "albums",
            Self::Playlists => "playlists",
            Self::Song => "song",
        }
    }
}
fn cache_default() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum OperationRequest {
    CollectionEdit(CollectionEdit),
    LibrarySources,
    RemoveSource {
        key: String,
    },
    MissingArtwork,
    ArtistDetail {
        key: String,
        #[serde(default)]
        secondary: String,
    },
    CollectionPreview(CollectionPreview),
    GroupDetail(CollectionIdentity),
    ArtistQueue {
        key: String,
        #[serde(default)]
        secondary: String,
    },
    GroupQueue(CollectionIdentity),
    FetchCover(CoverFetch),
    FetchCoverAuto(CoverFetch),
    FetchLyrics(LyricsFetch),
    Cover {
        kind: CoverKind,
        key: String,
        file: String,
        #[serde(default)]
        id: i64,
    },
    RemoveCover {
        kind: CoverKind,
        key: String,
        #[serde(default)]
        id: i64,
    },
    ReadLyrics {
        key: String,
        file: String,
    },
    ImportPlaylist {
        file: String,
    },
    ExportPlaylist {
        id: i64,
        file: String,
    },
}
impl OperationRequest {
    pub(crate) fn validate(&self) -> Result<(), OperationError> {
        match self {
            Self::RemoveSource { key } if key.trim().is_empty() => Err(
                OperationError::InvalidRequest("Missing music folder".into()),
            ),
            Self::FetchCover(fetch) | Self::FetchCoverAuto(fetch)
                if !matches!(fetch.kind, CollectionKind::Artists | CollectionKind::Albums) =>
            {
                Err(OperationError::InvalidRequest(
                    "Online covers are available for artists and albums".into(),
                ))
            }
            _ => Ok(()),
        }
    }
    pub fn is_local_read(&self) -> bool {
        matches!(
            self,
            Self::ArtistDetail { .. } | Self::GroupDetail(_) | Self::CollectionPreview(_)
        )
    }
    pub fn removes_source(&self) -> bool {
        matches!(self, Self::RemoveSource { .. })
    }
    pub fn from_value(value: Value) -> Result<Self, OperationError> {
        serde_json::from_value(value)
            .map_err(|error| OperationError::InvalidRequest(error.to_string()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationError {
    InvalidRequest(String),
    InvalidResult(String),
    Failed(String),
    Busy,
    Unavailable(String),
}
impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "Invalid library operation: {message}"),
            Self::InvalidResult(message) => write!(f, "Invalid library response: {message}"),
            Self::Failed(message) | Self::Unavailable(message) => f.write_str(message),
            Self::Busy => f.write_str("Library operation queue is busy"),
        }
    }
}
impl std::error::Error for OperationError {}
impl From<String> for OperationError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlbumDetail {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub artwork: Option<String>,
    pub song_count: u32,
    pub duration: u64,
    pub artwork_thumb: Option<String>,
    #[serde(rename = "navigationTitle")]
    pub navigation_title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionDetail {
    #[serde(flatten)]
    pub identity: CollectionIdentity,
    pub title: String,
    pub artwork: Option<String>,
    pub backdrop: Option<String>,
    pub count: u64,
    pub duration: u64,
    #[serde(default, rename = "albumCount")]
    pub album_count: u64,
    pub albums: Vec<AlbumDetail>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibrarySource {
    pub path: String,
    pub available: bool,
    #[serde(rename = "songCount")]
    pub song_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sources {
    pub sources: Vec<LibrarySource>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRemoved {
    #[serde(rename = "removedPaths")]
    pub removed_paths: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissingArtwork {
    pub jobs: Vec<OperationRequest>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtworkResult {
    pub key: String,
    pub kind: CollectionKind,
    pub artwork: String,
    pub thumbnail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsStatus {
    Ok,
    Cached,
    NotFound,
    Offline,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LyricsResult {
    pub path: String,
    pub lyrics: Option<String>,
    pub status: LyricsStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadLyrics {
    pub path: String,
    pub lyrics: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistImported {
    pub name: String,
    pub imported: usize,
    pub unavailable: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistExported {
    pub exported: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueSong {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub artwork: String,
    pub duration: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Songs {
    pub songs: Vec<QueueSong>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum OperationResult {
    CollectionEdited,
    Sources(Sources),
    SourceRemoved(SourceRemoved),
    MissingArtwork(MissingArtwork),
    Detail(CollectionDetail),
    Songs(Songs),
    Artwork(ArtworkResult),
    Lyrics(LyricsResult),
    // Legacy song-cover operations return the existing metadata document.
    CoverUpdated(Option<crate::types::MetadataDocument>),
    ReadLyrics(ReadLyrics),
    PlaylistImported(PlaylistImported),
    PlaylistExported(PlaylistExported),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn malformed_requests_cannot_become_empty_operations() {
        for value in [
            json!({"action":"unknown"}),
            json!({"action":"remove-source"}),
            json!({"action":"group-detail","kind":"oops","key":"x"}),
            json!({"action":"export-playlist","id":"1","file":"x"}),
        ] {
            assert!(matches!(
                OperationRequest::from_value(value),
                Err(OperationError::InvalidRequest(_))
            ));
        }
    }
    #[test]
    fn source_removal_rejects_empty_path_before_database_work() {
        let request = OperationRequest::RemoveSource { key: " ".into() };
        assert!(matches!(
            request.validate(),
            Err(OperationError::InvalidRequest(_))
        ));
    }
    #[test]
    fn detail_reads_bypass_the_network_operation_queue() {
        assert!(OperationRequest::ArtistDetail {
            key: "Artist".into(),
            secondary: String::new()
        }
        .is_local_read());
        assert!(!OperationRequest::MissingArtwork.is_local_read());
        assert!(!OperationRequest::RemoveSource {
            key: "music".into()
        }
        .is_local_read());
    }
    #[test]
    fn optional_edit_fields_preserve_unchanged_and_removed_cover() {
        let unchanged = OperationRequest::from_value(
            json!({"action":"collection-edit","kind":"albums","key":"x"}),
        )
        .unwrap();
        let removed = OperationRequest::from_value(
            json!({"action":"collection-edit","kind":"albums","key":"x","cover":""}),
        )
        .unwrap();
        assert!(matches!(
            unchanged,
            OperationRequest::CollectionEdit(CollectionEdit { cover: None, .. })
        ));
        assert!(
            matches!(removed, OperationRequest::CollectionEdit(CollectionEdit { cover: Some(cover), .. }) if cover.is_empty())
        );
    }
}
