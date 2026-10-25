use rusqlite::Connection;
use std::collections::BTreeMap;

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
            "SELECT s.artist, s.title,
                    COALESCE(NULLIF(NULLIF(s.artwork_url,'DELETED'),''), NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''), NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''), NULLIF(NULLIF(awa.artwork_path,'DELETED'),'')),
                    COALESCE(NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''), NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''), NULLIF(NULLIF(s.artwork_url,'DELETED'),''), NULLIF(NULLIF(awa.artwork_thumb_path,'DELETED'),''))
             FROM active_songs s
             LEFT JOIN album_artworks awa ON awa.album_key = s.album_artist || ':' || s.album
             ORDER BY s.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut artists = BTreeMap::new();
    for row in rows {
        let (credit, title, cover, thumb) = row.map_err(|error| error.to_string())?;
        for name in crate::artist_credits::primary_artists(&credit, &title) {
            let entry = artists.entry(name.clone()).or_insert_with(|| ArtistEntry {
                name,
                song_count: 0,
                artwork: None,
                artwork_thumb: None,
                song_artwork: None,
                song_artwork_thumb: None,
            });
            entry.song_count += 1;
            if entry.song_artwork.is_none() {
                entry.song_artwork = cover.clone();
            }
            if entry.song_artwork_thumb.is_none() {
                entry.song_artwork_thumb = thumb.clone();
            }
        }
    }
    let mut artwork = conn
        .prepare("SELECT artist_name, NULLIF(NULLIF(artwork_path,'DELETED'),''), NULLIF(NULLIF(artwork_thumb_path,'DELETED'),'') FROM artist_artworks")
        .map_err(|error| error.to_string())?;
    let covers = artwork
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for cover in covers {
        let (name, full, thumb) = cover.map_err(|error| error.to_string())?;
        if let Some(entry) = artists.get_mut(&name) {
            entry.artwork = full;
            entry.artwork_thumb = thumb;
        }
    }
    let mut artists: Vec<_> = artists.into_values().collect();
    artists.sort_by_key(|artist| artist.name.to_lowercase());
    Ok(artists)
}

pub fn get_albums(conn: &Connection) -> Result<Vec<AlbumEntry>, String> {
    let mut statement = conn
        .prepare(
            "SELECT album_artist || ':' || album AS key, album, album_artist, COUNT(*), SUM(duration),
                    COALESCE(NULLIF(NULLIF(awa.artwork_path, 'DELETED'), ''), MAX(NULLIF(NULLIF(s.artwork_preview_url, 'DELETED'), '')), MAX(NULLIF(NULLIF(s.artwork_url, 'DELETED'), '')), MAX(NULLIF(NULLIF(s.artwork_thumb_url, 'DELETED'), ''))),
                    COALESCE(NULLIF(NULLIF(awa.artwork_thumb_path, 'DELETED'), ''), MAX(NULLIF(NULLIF(s.artwork_thumb_url, 'DELETED'), '')), MAX(NULLIF(NULLIF(s.artwork_preview_url, 'DELETED'), '')), MAX(NULLIF(NULLIF(s.artwork_url, 'DELETED'), '')))
             FROM active_songs s
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

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn get_genres(conn: &Connection) -> Result<Vec<GenreEntry>, String> {
    let mut statement = conn
        .prepare(
            "SELECT genre, COUNT(*),
                    (SELECT COALESCE(s2.artwork_preview_url, NULLIF(awa.artwork_path, 'DELETED'))
                     FROM active_songs s2
                     LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album
                     WHERE s2.genre = s.genre
                       AND COALESCE(s2.artwork_preview_url, NULLIF(awa.artwork_path, 'DELETED')) IS NOT NULL
                     LIMIT 1),
                    (SELECT COALESCE(NULLIF(NULLIF(s2.artwork_thumb_url,'DELETED'),''), NULLIF(NULLIF(s2.artwork_preview_url,'DELETED'),''), NULLIF(NULLIF(s2.artwork_url,'DELETED'),''), NULLIF(NULLIF(awa.artwork_thumb_path,'DELETED'),''))
                     FROM active_songs s2
                     LEFT JOIN album_artworks awa ON awa.album_key = s2.album_artist || ':' || s2.album
                     WHERE s2.genre = s.genre
                       AND COALESCE(NULLIF(NULLIF(s2.artwork_thumb_url,'DELETED'),''), NULLIF(NULLIF(s2.artwork_preview_url,'DELETED'),''), NULLIF(NULLIF(s2.artwork_url,'DELETED'),''), NULLIF(NULLIF(awa.artwork_thumb_path,'DELETED'),'')) IS NOT NULL
                     LIMIT 1)
             FROM active_songs s
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

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}
