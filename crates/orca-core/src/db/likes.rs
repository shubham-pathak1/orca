use rusqlite::{Connection, OptionalExtension};

pub struct LikedSongsSummary {
    pub count: u32,
    pub artwork: String,
}

pub fn is_song_liked(conn: &Connection, song_id: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM liked_songs WHERE song_id=?1)",
        [song_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

pub fn set_song_liked(conn: &Connection, song_id: i64, liked: bool) -> Result<(), String> {
    if song_id <= 0 {
        return Err("Invalid song".into());
    }
    let visible: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM active_songs WHERE id=?1)",
            [song_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !visible {
        return Err("Song is no longer in the library".into());
    }
    if liked {
        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO liked_songs(song_id) SELECT id FROM songs WHERE id=?1",
                [song_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 && !is_song_liked(conn, song_id)? {
            return Err("Song is no longer in the library".into());
        }
    } else {
        conn.execute("DELETE FROM liked_songs WHERE song_id=?1", [song_id])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn remove_many_likes(conn: &Connection, song_ids: &[i64]) -> Result<(), String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    {
        let mut statement = transaction
            .prepare("DELETE FROM liked_songs WHERE song_id = ?1")
            .map_err(|error| error.to_string())?;
        for song_id in song_ids {
            statement
                .execute([song_id])
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn liked_songs_summary(conn: &Connection) -> Result<LikedSongsSummary, String> {
    let count = conn
        .query_row(
            "SELECT COUNT(*) FROM liked_songs l CROSS JOIN active_songs s ON s.id=l.song_id",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if count == 0 {
        return Ok(LikedSongsSummary {
            count,
            artwork: String::new(),
        });
    }
    let artwork = conn
        .query_row(
            "SELECT COALESCE(NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),(SELECT NULLIF(NULLIF(a.artwork_path,'DELETED'),'') FROM album_artworks a WHERE a.album_key=s.album_artist || ':' || s.album),'')
             FROM liked_songs l CROSS JOIN active_songs s ON s.id=l.song_id
             WHERE COALESCE(NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),(SELECT NULLIF(NULLIF(a.artwork_path,'DELETED'),'') FROM album_artworks a WHERE a.album_key=s.album_artist || ':' || s.album),'')!=''
             ORDER BY l.liked_at DESC,l.song_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(LikedSongsSummary { count, artwork })
}
