pub mod artist_credits;
pub mod artwork_cache;
pub mod atomic_file;
pub mod audio_engine;
pub mod audio_output;
pub mod db;
pub mod lastfm;
pub mod library;
pub mod lyrics;
mod media_identity;
pub mod online_artwork;
pub mod scanner;

pub(crate) const USER_AGENT: &str = concat!(
    "Orca/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/shubham-pathak1/orca)"
);
