use std::collections::HashMap;

use rusqlite::{params, Connection};

#[derive(serde::Serialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub cover_path: Option<String>,
    pub song_count: i64,
}

pub struct PlaylistSongExport {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration: u32,
}

pub fn create_playlist(
    conn: &Connection,
    name: &str,
    cover_path: Option<&str>,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO playlists (name, cover_path) VALUES (?1, ?2)",
        params![name, cover_path],
    )
    .map_err(|error| error.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_playlist_cover(
    conn: &Connection,
    playlist_id: i64,
    cover_path: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE playlists SET cover_path = ?1 WHERE id = ?2",
        params![cover_path, playlist_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_playlists(conn: &Connection) -> Result<Vec<Playlist>, String> {
    let mut statement = conn
        .prepare(
            "SELECT p.id, p.name, p.cover_path, COUNT(ps.id) AS song_count
             FROM playlists p
             LEFT JOIN playlist_songs ps ON p.id = ps.playlist_id
             GROUP BY p.id
             ORDER BY p.created_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                cover_path: row.get(2)?,
                song_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn add_to_playlist(conn: &Connection, playlist_id: i64, song_id: i64) -> Result<(), String> {
    let already_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?1 AND song_id = ?2",
            params![playlist_id, song_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if already_exists > 0 {
        return Ok(());
    }

    let current_max: Option<i64> = conn
        .query_row(
            "SELECT MAX(position) FROM playlist_songs WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .unwrap_or(None);
    let next_position = current_max.unwrap_or(0) + 1;

    conn.execute(
        "INSERT INTO playlist_songs (playlist_id, song_id, position) VALUES (?1, ?2, ?3)",
        params![playlist_id, song_id, next_position],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn remove_from_playlist(
    conn: &Connection,
    playlist_id: i64,
    song_id: i64,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1 AND song_id = ?2",
        params![playlist_id, song_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_playlist_song_ids(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>, String> {
    let mut statement = conn
        .prepare("SELECT song_id FROM playlist_songs WHERE playlist_id = ?1 ORDER BY position ASC")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![playlist_id], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn get_song_path_index(conn: &Connection) -> Result<HashMap<String, i64>, String> {
    let mut statement = conn
        .prepare("SELECT id, path FROM songs")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn get_playlist_export_songs(
    conn: &Connection,
    playlist_id: i64,
) -> Result<Vec<PlaylistSongExport>, String> {
    let mut statement = conn
        .prepare(
            "SELECT s.path, s.title, s.artist, s.duration
             FROM playlist_songs ps
             INNER JOIN songs s ON s.id = ps.song_id
             WHERE ps.playlist_id = ?1
             ORDER BY ps.position ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![playlist_id], |row| {
            Ok(PlaylistSongExport {
                path: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                duration: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn delete_playlist(conn: &Connection, playlist_id: i64) -> Result<(), String> {
    conn.execute(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1",
        params![playlist_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn rename_playlist(conn: &Connection, playlist_id: i64, new_name: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE playlists SET name = ?1 WHERE id = ?2",
        params![new_name, playlist_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
