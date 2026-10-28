//! Database facade; domain modules own schema, songs, artwork, cache and playlists.
mod artist_index;
mod artwork;
pub use artist_index::refresh_artist_index;
mod cache;
mod folder_cache;
pub use folder_cache::{folder_cache_revision, load_folder_cache, save_folder_cache};
mod catalog;
mod likes;
mod media_artwork;
mod playlists;

pub use artwork::{
    collection_needs_artwork, get_albums_needing_artwork, get_artists_needing_artwork,
    next_artwork_collection, remove_album_artwork, remove_artist_artwork, update_album_artwork,
    update_artist_artwork,
};

pub use cache::{
    get_cached_waveform, get_lyrics, get_setting, lyrics_suppressed, save_waveform, set_lyrics,
    set_lyrics_with_suppression, set_setting,
};

pub use catalog::{
    get_album_collection, get_albums, get_artists, get_genres, AlbumEntry, ArtistEntry, GenreEntry,
};

pub use likes::{
    is_song_liked, liked_songs_summary, remove_many_likes, set_song_liked, LikedSongsSummary,
};

pub use media_artwork::migrate_inline_artwork_to_files;

pub use playlists::{
    add_to_playlist, create_playlist, delete_playlist, get_playlist_export_songs,
    get_playlist_song_ids, get_playlists, get_song_path_index, get_song_playlist_ids,
    remove_from_playlist, remove_many_from_playlist, rename_playlist, update_playlist_cover,
    Playlist,
};

mod schema;
mod songs;
pub use schema::init_db;
pub use songs::{
    apply_song_changes, delete_song_by_path, get_all_songs, get_existing_songs_map,
    migrate_legacy_songs_if_needed, replace_songs_in_db, save_edited_song, save_songs_to_db,
};
