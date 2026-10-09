//! UI-specific controllers translate Slint callbacks into service requests.
//! They hold weak window handles and never open a database or decode audio.
pub(crate) mod browse;
pub(crate) mod collections;
pub(crate) mod editor;
pub(crate) mod input;
pub(crate) mod playlists;
pub(crate) mod preferences;
