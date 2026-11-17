//! Typed audio commands. Numeric values are validated at the backend boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OutputPreference {
    pub device_id: String,
    pub exclusive: bool,
    pub unprocessed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default)]
pub struct OutputStatus {
    pub unprocessed: bool,
    pub device_name: String,
    pub exclusive: bool,
    pub sample_rate: u32,
    pub source_sample_rate: u32,
    pub notice: String,
    pub channels: u16,
    pub sample_format: String,
    pub switching: bool,
    pub error: String,
}

#[derive(Clone, Debug)]
pub enum PlaybackCommand {
    Play(String),
    QueueNext(String),
    LoadPaused { path: String, position_ms: u64 },
    Pause,
    Resume,
    Stop,
    ClearQueued,
    Seek(u64),
    Volume(f32),
    ReplayGain(bool),
}
#[derive(Clone, Debug)]
pub enum PlaylistEdit {
    Create { name: String },
    Rename { id: i64, name: String },
    Delete { id: i64 },
    Add { id: i64, song_id: i64 },
    Remove { id: i64, song_id: i64 },
    RemoveMany { id: Option<i64>, song_ids: Vec<i64> },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistChange {
    Created,
    Renamed,
    Deleted,
    Membership,
}
