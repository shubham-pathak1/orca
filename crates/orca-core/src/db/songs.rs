//! Song persistence and incremental library changes.
use crate::library::LocalSong;
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use std::path::PathBuf;

pub fn save_songs_to_db(conn: &Connection, songs: &[LocalSong]) -> Result<(), String> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for song in songs {
        upsert_song(&tx, song)?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// A metadata save updates the song, explicit cover removal and lyrics cache together.
pub fn save_edited_song(
    conn: &Connection,
    song: &LocalSong,
    lyrics: &str,
    cover_removed: bool,
) -> Result<(), String> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    upsert_song(&tx, song)?;
    if cover_removed {
        tx.execute("UPDATE songs SET artwork_url='DELETED',artwork_preview_url='DELETED',artwork_thumb_url='DELETED' WHERE path=?1", [&song.path]).map_err(|e| e.to_string())?;
    }
    super::set_lyrics(&tx, &song.path, lyrics)?;
    tx.commit().map_err(|e| e.to_string())
}

pub fn apply_song_changes(
    conn: &Connection,
    updated_songs: &[LocalSong],
    removed_paths: &[String],
) -> Result<(), String> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for song in updated_songs {
        upsert_song(&tx, song)?;
    }
    for path in removed_paths {
        tx.execute("DELETE FROM songs WHERE path = ?1", params![path])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn replace_songs_in_db(conn: &Connection, songs: &[LocalSong]) -> Result<(), String> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for song in songs {
        upsert_song(&tx, song)?;
    }
    let scanned_paths = songs
        .iter()
        .map(|song| song.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let existing = {
        let mut stmt = tx
            .prepare("SELECT id, path FROM songs")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (song_id, path) in existing {
        if scanned_paths.contains(path.as_str()) {
            continue;
        }
        tx.execute(
            "DELETE FROM playlist_songs WHERE song_id = ?1",
            params![song_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM songs WHERE id = ?1", params![song_id])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

fn upsert_song(conn: &Connection, song: &LocalSong) -> Result<(), String> {
    conn.execute(
        "INSERT INTO songs (title, artist, album_artist, album, year, track_number, disc_number, genre, path, duration, artwork_url, artwork_thumb_url, artwork_preview_url, lyrics, sample_rate, bitrate, bit_depth, format, modified_at, file_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT DO UPDATE SET
             title = excluded.title,
             artist = excluded.artist,
             album_artist = excluded.album_artist,
             album = excluded.album,
             year = excluded.year,
             track_number = excluded.track_number,
             disc_number = excluded.disc_number,
             genre = excluded.genre,
             path = excluded.path,
             duration = excluded.duration,
             artwork_url = CASE WHEN songs.artwork_url='DELETED' AND excluded.artwork_url IS NULL THEN 'DELETED' ELSE excluded.artwork_url END,
             artwork_thumb_url = CASE WHEN songs.artwork_url='DELETED' AND excluded.artwork_url IS NULL THEN 'DELETED' ELSE excluded.artwork_thumb_url END,
             artwork_preview_url = CASE WHEN songs.artwork_url='DELETED' AND excluded.artwork_url IS NULL THEN 'DELETED' ELSE excluded.artwork_preview_url END,
             lyrics = excluded.lyrics,
             sample_rate = excluded.sample_rate,
             bitrate = excluded.bitrate,
             bit_depth = excluded.bit_depth,
             format = excluded.format,
             modified_at = excluded.modified_at,
             file_size = excluded.file_size",
        params![
            song.title,
            song.artist,
            song.album_artist,
            song.album,
            song.year,
            song.track_number,
            song.disc_number,
            song.genre,
            song.path,
            song.duration,
            song.artwork,
            song.artwork_thumb,
            song.artwork_preview,
            song.lyrics,
            song.sample_rate,
            song.bitrate,
            song.bit_depth,
            song.format,
            song.modified_at,
            song.file_size
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_all_songs(conn: &Connection) -> Result<Vec<LocalSong>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.title, s.artist, s.album_artist, s.album, s.year, s.track_number, s.disc_number,
                    s.genre, s.path, s.duration,
                    CASE WHEN s.artwork_url='DELETED' THEN NULL ELSE COALESCE(s.artwork_url, awa.artwork_path) END,
                    CASE WHEN s.artwork_url='DELETED' THEN NULL ELSE COALESCE(s.artwork_thumb_url, awa.artwork_thumb_path) END,
                    CASE WHEN s.artwork_url='DELETED' THEN NULL ELSE COALESCE(s.artwork_preview_url, awa.artwork_path) END,
                    s.lyrics, s.sample_rate, s.bitrate, s.bit_depth, s.format, s.modified_at, s.file_size
             FROM songs s
             LEFT JOIN album_artworks awa ON (s.album_artist || ':' || s.album) = awa.album_key"
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LocalSong {
                id: Some(row.get(0)?),
                title: row.get(1)?,
                artist: row.get(2)?,
                album_artist: row.get(3)?,
                album: row.get(4)?,
                year: row.get(5)?,
                track_number: row.get(6)?,
                disc_number: row.get(7)?,
                genre: row.get(8)?,
                path: row.get(9)?,
                duration: row.get(10)?,
                artwork: row.get(11)?,
                artwork_thumb: row.get(12)?,
                artwork_preview: row.get(13)?,
                lyrics: row.get(14)?,
                sample_rate: row.get(15)?,
                bitrate: row.get(16)?,
                bit_depth: row.get(17)?,
                format: row.get(18)?,
                modified_at: row.get(19)?,
                file_size: row.get(20)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut songs = Vec::new();
    for row in rows {
        songs.push(row.map_err(|e| e.to_string())?);
    }
    Ok(songs)
}

pub fn get_existing_songs_map(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, (i64, u64, LocalSong)>, String> {
    let songs = get_all_songs(conn)?;
    let mut map = std::collections::HashMap::new();
    for song in songs {
        let mtime = song.modified_at.unwrap_or(0);
        let size = song.file_size.unwrap_or(0);
        map.insert(song.path.clone(), (mtime, size, song));
    }
    Ok(map)
}

pub fn migrate_legacy_songs_if_needed(
    conn: &Connection,
    legacy_app_data_dir: PathBuf,
) -> Result<usize, String> {
    let legacy_db_path = legacy_app_data_dir.join("orca.db");
    if !legacy_db_path.exists() {
        return Ok(0);
    }

    let current_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM songs", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if current_count > 0 {
        return Ok(0);
    }

    let legacy_conn = Connection::open(legacy_db_path).map_err(|e| e.to_string())?;
    let legacy_has_songs_table: i64 = legacy_conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'songs'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if legacy_has_songs_table == 0 {
        return Ok(0);
    }

    let mut stmt = legacy_conn
        .prepare("SELECT title, artist, path, duration, artwork_url FROM songs")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LocalSong {
                path: row.get(2)?,
                title: row.get(0)?,
                artist: row.get(1)?,
                album_artist: row.get(1)?,
                album: "Unknown Album".to_string(),
                year: None,
                track_number: None,
                disc_number: None,
                genre: None,
                duration: row.get(3)?,
                artwork: row.get(4)?,
                artwork_thumb: None,
                artwork_preview: None,
                lyrics: None,
                sample_rate: None,
                bitrate: None,
                bit_depth: None,
                format: None,
                id: None,
                modified_at: None,
                file_size: None,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut imported = 0usize;
    for row in rows {
        let song = row.map_err(|e| e.to_string())?;
        upsert_song(conn, &song)?;
        imported += 1;
    }

    Ok(imported)
}

pub fn delete_song_by_path(conn: &Connection, path: &str) -> Result<(), String> {
    conn.execute("DELETE FROM songs WHERE path = ?1", params![path])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        directory: PathBuf,
        connection: Option<Connection>,
    }

    impl Fixture {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "orca-song-transactions-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let connection = super::super::init_db(directory.clone()).unwrap();
            Self {
                directory,
                connection: Some(connection),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.connection.take());
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn song(path: &str) -> LocalSong {
        serde_json::from_value(serde_json::json!({
            "path": path, "title": path, "artist": "Artist",
            "album_artist": "Artist", "album": "Album", "duration": 1000
        }))
        .unwrap()
    }

    #[test]
    fn failed_batches_roll_back_and_leave_connection_reusable() {
        let fixture = Fixture::new();
        let conn = fixture.connection.as_ref().unwrap();
        save_songs_to_db(conn, &[song("original")]).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_bad_song BEFORE INSERT ON songs
            WHEN NEW.path='bad' BEGIN SELECT RAISE(ABORT,'injected write failure'); END;",
        )
        .unwrap();
        let batch = [song("new"), song("bad")];
        for action in [0, 1, 2] {
            let result = match action {
                0 => save_songs_to_db(conn, &batch),
                1 => apply_song_changes(conn, &batch, &["original".into()]),
                _ => replace_songs_in_db(conn, &batch),
            };
            assert!(result.is_err());
            assert!(
                conn.is_autocommit(),
                "failed write must release its transaction"
            );
            let songs = get_all_songs(conn).unwrap();
            assert_eq!(songs.len(), 1);
            assert_eq!(songs[0].path, "original");
        }
        save_songs_to_db(conn, &[song("recovered")]).unwrap();
        assert_eq!(get_all_songs(conn).unwrap().len(), 2);
    }

    #[test]
    fn failed_removal_rolls_back_new_tracks_too() {
        let fixture = Fixture::new();
        let conn = fixture.connection.as_ref().unwrap();
        save_songs_to_db(conn, &[song("original")]).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER reject_removal BEFORE DELETE ON songs
            BEGIN SELECT RAISE(ABORT,'injected delete failure'); END;",
        )
        .unwrap();
        for replace in [false, true] {
            let result = if replace {
                replace_songs_in_db(conn, &[song("new")])
            } else {
                apply_song_changes(conn, &[song("new")], &["original".into()])
            };
            assert!(result.is_err());
            assert!(conn.is_autocommit());
            let songs = get_all_songs(conn).unwrap();
            assert_eq!(songs.len(), 1);
            assert_eq!(songs[0].path, "original");
        }
    }

    #[test]
    fn metadata_cache_failure_rolls_back_song_and_cover_changes() {
        let fixture = Fixture::new();
        let conn = fixture.connection.as_ref().unwrap();
        let mut original = song("original");
        original.title = "Before".into();
        save_songs_to_db(conn, &[original.clone()]).unwrap();
        super::super::set_lyrics(conn, "original", "Old lyrics").unwrap();
        conn.execute_batch("CREATE TRIGGER fail_edited_lyrics BEFORE INSERT ON lyrics BEGIN SELECT RAISE(ABORT,'injected lyrics failure'); END;").unwrap();
        let mut changed = original;
        changed.title = "After".into();
        assert!(save_edited_song(conn, &changed, "New lyrics", true).is_err());
        assert!(conn.is_autocommit());
        assert_eq!(get_all_songs(conn).unwrap()[0].title, "Before");
        assert_eq!(
            super::super::get_lyrics(conn, "original").unwrap(),
            "Old lyrics"
        );
        conn.execute_batch("DROP TRIGGER fail_edited_lyrics")
            .unwrap();
        save_edited_song(conn, &changed, "New lyrics", true).unwrap();
        assert_eq!(get_all_songs(conn).unwrap()[0].title, "After");
        assert_eq!(
            super::super::get_lyrics(conn, "original").unwrap(),
            "New lyrics"
        );
    }
}
