//! Metadata contracts contain owned data, never widgets or database handles.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MetadataFields {
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetadataDocument {
    #[serde(flatten)]
    pub fields: MetadataFields,
    pub artwork: Option<String>,
    #[serde(default)]
    pub file_version: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MetadataEdit {
    #[serde(flatten)]
    pub fields: MetadataFields,
    pub cover_to_embed: Option<String>,
    #[serde(default)]
    pub remove_cover: bool,
    pub file_version: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataAssetKind {
    Cover,
    Lyrics,
}
#[derive(Clone, Debug)]
pub enum MetadataRequest {
    Load {
        path: String,
        fetch_cover: bool,
        generation: i32,
    },
    Save {
        edit: MetadataEdit,
        generation: i32,
    },
    SelectAsset {
        kind: MetadataAssetKind,
        target: String,
        file: String,
        generation: i32,
    },
}
impl MetadataRequest {
    pub fn generation(&self) -> i32 {
        match self {
            Self::Load { generation, .. }
            | Self::Save { generation, .. }
            | Self::SelectAsset { generation, .. } => *generation,
        }
    }
}
