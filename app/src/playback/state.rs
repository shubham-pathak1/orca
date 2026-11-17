//! UI-owned playback state. Catalog navigation cannot replace this session.
use crate::{lyric_render, playback_clock, LyricLine, Song};
use orca_services::types::Track;
use slint::VecModel;
use std::{cell::RefCell, rc::Rc, time::Instant};
pub(crate) struct SeekPreview {
    pub target: f32,
    pub path: String,
    pub started: Instant,
}
pub(crate) struct PlayerModels {
    pub now: Rc<RefCell<Track>>,
    pub lines: RefCell<Vec<LyricLine>>,
    pub preview: RefCell<Option<SeekPreview>>,
    pub playback_clock: RefCell<playback_clock::PlaybackClock>,
    pub lyric_renderer: RefCell<lyric_render::Renderer>,
    pub queue: Rc<VecModel<Song>>,
    pub queue_tracks: RefCell<Vec<Track>>,
    pub neighbors: RefCell<Vec<Track>>,
}
impl PlayerModels {
    pub fn new() -> Self {
        Self {
            now: Rc::new(RefCell::new(Track::default())),
            lines: RefCell::new(vec![]),
            preview: RefCell::new(None),
            playback_clock: RefCell::new(playback_clock::PlaybackClock::new()),
            lyric_renderer: RefCell::new(lyric_render::Renderer::new()),
            queue: Rc::new(VecModel::default()),
            queue_tracks: RefCell::new(vec![]),
            neighbors: RefCell::new(vec![]),
        }
    }
}
