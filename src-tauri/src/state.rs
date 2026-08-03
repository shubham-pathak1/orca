use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
};

use orca_core::{
    audio_engine::{self, AudioCommand, PlaybackState, VisualizerData},
    db,
    library::LocalSong,
};

pub(crate) struct OrcaState {
    pub(crate) db_conn: rusqlite::Connection,
    pub(crate) artwork_dir: PathBuf,
    pub(crate) songs: Vec<LocalSong>,
    pub(crate) audio_tx: mpsc::Sender<AudioCommand>,
    pub(crate) playback_state: Arc<Mutex<PlaybackState>>,
    #[allow(dead_code)]
    pub(crate) visualizer_data: VisualizerData,
    pub(crate) media_controls: Option<souvlaki::MediaControls>,
}

pub(crate) struct SharedOrcaState(pub(crate) Mutex<OrcaState>);

#[derive(serde::Serialize)]
pub(crate) struct LibrarySnapshot {
    pub(crate) songs: Vec<LocalSong>,
    pub(crate) playlists: Vec<db::Playlist>,
    pub(crate) artists: Vec<db::ArtistEntry>,
    pub(crate) albums: Vec<db::AlbumEntry>,
    pub(crate) genres: Vec<db::GenreEntry>,
    pub(crate) playback: PlaybackState,
    pub(crate) folder_count: usize,
}

pub(crate) fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("orca")
}

pub(crate) fn artwork_dir() -> PathBuf {
    app_data_dir().join("artwork")
}

pub(crate) fn normalize_path(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

pub(crate) fn playback_snapshot_from(state: &OrcaState) -> PlaybackState {
    state
        .playback_state
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default()
}

pub(crate) fn load_state() -> Result<OrcaState, String> {
    let app_dir = app_data_dir();
    let artwork_dir = artwork_dir();
    std::fs::create_dir_all(&app_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&artwork_dir).map_err(|error| error.to_string())?;

    let conn = db::init_db(app_dir)?;
    db::migrate_inline_artwork_to_files(&conn, &artwork_dir)?;
    let songs = db::get_all_songs(&conn)?;
    let (audio_tx, playback_state, visualizer_data) =
        audio_engine::spawn_audio_thread::<fn(&str, u64)>(None);

    Ok(OrcaState {
        db_conn: conn,
        artwork_dir,
        songs,
        audio_tx,
        playback_state,
        visualizer_data,
        media_controls: None,
    })
}
