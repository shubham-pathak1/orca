//! Typed audio commands. Numeric values are validated at the backend boundary.
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
}
#[derive(Clone, Debug)]
pub enum PlaylistEdit {
    Create { name: String },
    Rename { id: i64, name: String },
    Delete { id: i64 },
    Add { id: i64, song_id: i64 },
    Remove { id: i64, song_id: i64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistChange {
    Created,
    Renamed,
    Deleted,
    Membership,
}
