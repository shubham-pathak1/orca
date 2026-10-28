use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};

/// Read one identity after a durable cursor, without a library-sized job queue.
pub fn next_artwork_collection(
    conn: &Connection,
    artists: bool,
    key: &str,
    secondary: &str,
) -> Result<Option<(String, String)>, String> {
    let result = if artists {
        conn.query_row("SELECT DISTINCT pa.name,'' FROM song_primary_artists pa JOIN songs s ON s.id=pa.song_id WHERE s.library_visible=1 AND pa.name COLLATE NOCASE>?1 COLLATE NOCASE ORDER BY pa.name COLLATE NOCASE LIMIT 1", [key], |row| Ok((row.get(0)?, row.get(1)?)))
    } else {
        conn.query_row("SELECT album,album_artist FROM active_songs WHERE (album_artist,album)>(?1,?2) GROUP BY album_artist,album ORDER BY album_artist,album LIMIT 1", params![secondary,key], |row| Ok((row.get(0)?, row.get(1)?)))
    };
    result.optional().map_err(|error| error.to_string())
}

/// Existing local images and deliberate removals both suppress automatic lookup.
pub fn collection_needs_artwork(
    conn: &Connection,
    artist: &str,
    album: Option<&str>,
) -> Result<bool, String> {
    let result = if let Some(album) = album {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM active_songs WHERE album_artist=?1 AND album=?2)
             AND NOT EXISTS(SELECT 1 FROM album_artworks WHERE album_key=?1 || ':' || ?2 AND NULLIF(artwork_path,'') IS NOT NULL)
             AND NOT EXISTS(SELECT 1 FROM active_songs WHERE album_artist=?1 AND album=?2 AND
             (NULLIF(artwork_url,'') IS NOT NULL OR NULLIF(artwork_preview_url,'') IS NOT NULL OR NULLIF(artwork_thumb_url,'') IS NOT NULL))",
            params![artist, album], |row| row.get(0),
        )
    } else {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM song_primary_artists pa JOIN songs s ON s.id=pa.song_id WHERE pa.name=?1 COLLATE NOCASE AND s.library_visible=1)
             AND NOT EXISTS(SELECT 1 FROM artist_artworks WHERE artist_name=?1 AND NULLIF(artwork_path,'') IS NOT NULL)",
            [artist], |row| row.get(0),
        )
    };
    result.map_err(|error| error.to_string())
}

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
        .prepare("SELECT artist_name FROM artist_artworks WHERE artwork_path IS NOT NULL")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let covered = rows
        .collect::<rusqlite::Result<std::collections::HashSet<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(super::get_artists(conn)?
        .into_iter()
        .filter(|artist| !covered.contains(&artist.name))
        .map(|artist| artist.name)
        .collect())
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
             FROM active_songs s
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
