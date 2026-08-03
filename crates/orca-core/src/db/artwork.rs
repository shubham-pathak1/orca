use rusqlite::{params, Connection};

pub fn update_artist_artwork(
    conn: &Connection,
    artist_name: &str,
    artwork_path: Option<&str>,
    artwork_thumb_path: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO artist_artworks (artist_name, artwork_path, artwork_thumb_path)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(artist_name) DO UPDATE SET
           artwork_path = excluded.artwork_path,
           artwork_thumb_path = excluded.artwork_thumb_path",
        params![artist_name, artwork_path, artwork_thumb_path],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove_artist_artwork(conn: &Connection, artist_name: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO artist_artworks (artist_name, artwork_path, artwork_thumb_path)
         VALUES (?1, 'DELETED', 'DELETED')
         ON CONFLICT(artist_name) DO UPDATE SET
           artwork_path = 'DELETED',
           artwork_thumb_path = 'DELETED'",
        params![artist_name],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_artists_needing_artwork(conn: &Connection) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT DISTINCT s.artist FROM songs s
             LEFT JOIN artist_artworks aa ON s.artist = aa.artist_name
             WHERE aa.artwork_path IS NULL
             ORDER BY s.artist COLLATE NOCASE ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn update_album_artwork(
    conn: &Connection,
    album_key: &str,
    artwork_path: Option<&str>,
    artwork_thumb_path: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO album_artworks (album_key, artwork_path, artwork_thumb_path)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(album_key) DO UPDATE SET
           artwork_path = excluded.artwork_path,
           artwork_thumb_path = excluded.artwork_thumb_path",
        params![album_key, artwork_path, artwork_thumb_path],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove_album_artwork(conn: &Connection, album_key: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO album_artworks (album_key, artwork_path, artwork_thumb_path)
         VALUES (?1, 'DELETED', 'DELETED')
         ON CONFLICT(album_key) DO UPDATE SET
           artwork_path = 'DELETED',
           artwork_thumb_path = 'DELETED'",
        params![album_key],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_albums_needing_artwork(
    conn: &Connection,
) -> Result<Vec<(String, String, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT DISTINCT s.album_artist || ':' || s.album, s.album, s.album_artist
             FROM songs s
             LEFT JOIN album_artworks aa ON (s.album_artist || ':' || s.album) = aa.album_key
             WHERE aa.artwork_path IS NULL
             ORDER BY s.album COLLATE NOCASE ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}
