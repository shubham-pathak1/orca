//! Database facade; domain modules own schema, songs, artwork, cache and playlists.
mod artwork;
mod cache;
mod catalog;
mod media_artwork;
mod playlists;

pub use artwork::{
    get_albums_needing_artwork, get_artists_needing_artwork, remove_album_artwork,
    remove_artist_artwork, update_album_artwork, update_artist_artwork,
};

pub use cache::{
    get_cached_waveform, get_lyrics, get_setting, save_waveform, set_lyrics, set_setting,
};

pub use catalog::{get_albums, get_artists, get_genres, AlbumEntry, ArtistEntry, GenreEntry};

pub use media_artwork::migrate_inline_artwork_to_files;

pub use playlists::{
    add_to_playlist, create_playlist, delete_playlist, get_playlist_export_songs,
    get_playlist_song_ids, get_playlists, get_song_path_index, remove_from_playlist,
    rename_playlist, update_playlist_cover, Playlist,
};

mod schema;
mod songs;
pub use schema::init_db;
pub use songs::{
    apply_song_changes, delete_song_by_path, get_all_songs, get_existing_songs_map,
    migrate_legacy_songs_if_needed, replace_songs_in_db, save_edited_song, save_songs_to_db,
};
