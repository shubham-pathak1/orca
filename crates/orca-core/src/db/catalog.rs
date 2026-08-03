use rusqlite::Connection;

#[derive(serde::Serialize, Debug)]
pub struct ArtistEntry {
    pub name: String,
    pub song_count: i64,
    pub artwork: Option<String>,
    pub artwork_thumb: Option<String>,
    pub song_artwork: Option<String>,
    pub song_artwork_thumb: Option<String>,
}

#[derive(serde::Serialize, Debug)]
pub struct AlbumEntry {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub song_count: i64,
    pub duration: i64,
    pub artwork: Option<String>,
    pub artwork_thumb: Option<String>,
}

#[derive(serde::Serialize, Debug)]
pub struct GenreEntry {
    pub name: String,
    pub song_count: i64,
    pub song_artwork: Option<String>,
    pub song_artwork_thumb: Option<String>,
}

pub fn get_artists(conn: &Connection) -> Result<Vec<ArtistEntry>, String> {
    let mut statement = conn
        .prepare(
            "SELECT s.artist, COUNT(*), NULLIF(aa.artwork_path, 'DELETED'), NULLIF(aa.artwork_thumb_path, 'DELETED'),
                    (SELECT COALESCE(s2.artwork_url, NULLIF(awa.artwork_path, 'DELETED')) FROM songs s2 LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album WHERE s2.artist = s.artist AND COALESCE(s2.artwork_url, NULLIF(awa.artwork_path, 'DELETED')) IS NOT NULL LIMIT 1),
                    (SELECT COALESCE(s2.artwork_thumb_url, NULLIF(awa.artwork_thumb_path, 'DELETED')) FROM songs s2 LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album WHERE s2.artist = s.artist AND COALESCE(s2.artwork_thumb_url, NULLIF(awa.artwork_thumb_path, 'DELETED')) IS NOT NULL LIMIT 1)
             FROM songs s
             LEFT JOIN artist_artworks aa ON s.artist = aa.artist_name
             GROUP BY s.artist
             ORDER BY s.artist COLLATE NOCASE ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(ArtistEntry {
                name: row.get(0)?,
                song_count: row.get(1)?,
                artwork: row.get(2)?,
                artwork_thumb: row.get(3)?,
                song_artwork: row.get(4)?,
                song_artwork_thumb: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string())).collect()
}

pub fn get_albums(conn: &Connection) -> Result<Vec<AlbumEntry>, String> {
    let mut statement = conn
        .prepare(
            "SELECT album_artist || ':' || album AS key, album, album_artist, COUNT(*), SUM(duration),
                    COALESCE(NULLIF(awa.artwork_path, 'DELETED'), MAX(s.artwork_preview_url)),
                    COALESCE(NULLIF(awa.artwork_thumb_path, 'DELETED'), MAX(s.artwork_thumb_url))
             FROM songs s
             LEFT JOIN album_artworks awa ON (s.album_artist || ':' || s.album) = awa.album_key
             GROUP BY album_artist, album
             ORDER BY album COLLATE NOCASE ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(AlbumEntry {
                key: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                song_count: row.get(3)?,
                duration: row.get(4)?,
                artwork: row.get(5)?,
                artwork_thumb: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string())).collect()
}

pub fn get_genres(conn: &Connection) -> Result<Vec<GenreEntry>, String> {
    let mut statement = conn
        .prepare(
            "SELECT genre, COUNT(*),
                    (SELECT COALESCE(s2.artwork_preview_url, NULLIF(awa.artwork_path, 'DELETED'))
                     FROM songs s2
                     LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album
                     WHERE s2.genre = s.genre
                       AND COALESCE(s2.artwork_preview_url, NULLIF(awa.artwork_path, 'DELETED')) IS NOT NULL
                     LIMIT 1),
                    (SELECT COALESCE(s2.artwork_thumb_url, NULLIF(awa.artwork_thumb_path, 'DELETED'))
                     FROM songs s2
                     LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album
                     WHERE s2.genre = s.genre
                       AND COALESCE(s2.artwork_thumb_url, NULLIF(awa.artwork_thumb_path, 'DELETED')) IS NOT NULL
                     LIMIT 1)
             FROM songs s
             WHERE genre IS NOT NULL AND genre != ''
             GROUP BY genre
             ORDER BY genre COLLATE NOCASE ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(GenreEntry {
                name: row.get(0)?,
                song_count: row.get(1)?,
                song_artwork: row.get(2)?,
                song_artwork_thumb: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string())).collect()
}
