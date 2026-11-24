//! Native window input, media integration and renderer selection.
pub(crate) mod desktop;
pub(crate) mod hotkey;
pub(crate) mod input_modality;
pub(crate) mod media_session;
pub(crate) mod render_mode;

#[cfg(windows)]
pub(crate) mod frame_pacing;

#[cfg(windows)]
mod media_cover;

#[cfg(windows)]
mod precision_scroll;
