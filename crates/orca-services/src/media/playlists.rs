use crate::Backend;
use orca_core::db;
use rusqlite::OptionalExtension;
use std::sync::atomic::Ordering;

impl Backend {
    pub(crate) fn small_artwork(&self, artwork: Option<&str>) -> Result<Option<String>, String> {
        let Some(path) = artwork.filter(|p| !p.is_empty() && *p != "DELETED") else {
            return Ok(None);
        };
        let thumbnail: Option<String> = self.conn.query_row(
            "SELECT NULLIF(NULLIF(artwork_thumb_url,''),'DELETED') AS thumb FROM songs WHERE ?1 IN (artwork_url,artwork_preview_url,artwork_thumb_url) AND NULLIF(NULLIF(artwork_thumb_url,''),'DELETED') IS NOT NULL
             UNION ALL SELECT NULLIF(NULLIF(artwork_thumb_path,''),'DELETED') FROM artist_artworks WHERE artwork_path=?1 AND NULLIF(NULLIF(artwork_thumb_path,''),'DELETED') IS NOT NULL
             UNION ALL SELECT NULLIF(NULLIF(artwork_thumb_path,''),'DELETED') FROM album_artworks WHERE artwork_path=?1 AND NULLIF(NULLIF(artwork_thumb_path,''),'DELETED') IS NOT NULL LIMIT 1",
            [path], |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        Ok(Some(thumbnail.unwrap_or_else(|| path.to_string())))
    }
    pub(crate) fn playlist_artwork(
        &self,
        id: i64,
        selected: Option<String>,
    ) -> Result<Option<String>, String> {
        if let Some(path) = selected.filter(|path| !path.trim().is_empty()) {
            return Ok(Some(path));
        }
        self.conn.query_row(
            "SELECT COALESCE(NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_url,'DELETED'),''),(SELECT NULLIF(NULLIF(artwork_path,'DELETED'),'') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album)) FROM playlist_songs ps JOIN songs s ON s.id=ps.song_id WHERE ps.playlist_id=?1 AND COALESCE(NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_url,'DELETED'),''),(SELECT NULLIF(NULLIF(artwork_path,'DELETED'),'') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album)) IS NOT NULL ORDER BY ps.position,ps.song_id LIMIT 1",
            [id], |row| row.get(0)).optional().map_err(|e| e.to_string()).map(Option::flatten)
    }
    pub fn playlist_action(
        &self,
        action: &str,
        id: i64,
        song_id: i64,
        name: &str,
    ) -> Result<(), String> {
        use crate::types::PlaylistEdit;
        let edit = match action {
            "create" => PlaylistEdit::Create { name: name.into() },
            "rename" => PlaylistEdit::Rename {
                id,
                name: name.into(),
            },
            "delete" => PlaylistEdit::Delete { id },
            "add" => PlaylistEdit::Add { id, song_id },
            "remove" => PlaylistEdit::Remove { id, song_id },
            _ => return Err("Unknown playlist action".into()),
        };
        self.edit_playlist(edit).map(|_| ())
    }
    pub fn edit_playlist(
        &self,
        edit: crate::types::PlaylistEdit,
    ) -> Result<crate::types::PlaylistChange, String> {
        use crate::types::{PlaylistChange, PlaylistEdit};
        let change = match edit {
            PlaylistEdit::Create { name } => {
                if name.trim().is_empty() {
                    return Err("Enter a playlist name".into());
                }
                db::create_playlist(&self.conn, name.trim(), None)?;
                PlaylistChange::Created
            }
            PlaylistEdit::Rename { id, name } => {
                if name.trim().is_empty() {
                    return Err("Enter a playlist name".into());
                }
                db::rename_playlist(&self.conn, id, name.trim())?;
                PlaylistChange::Renamed
            }
            PlaylistEdit::Delete { id } => {
                db::delete_playlist(&self.conn, id)?;
                PlaylistChange::Deleted
            }
            PlaylistEdit::Add { id, song_id } => {
                db::add_to_playlist(&self.conn, id, song_id)?;
                PlaylistChange::Membership
            }
            PlaylistEdit::Remove { id, song_id } => {
                db::remove_from_playlist(&self.conn, id, song_id)?;
                PlaylistChange::Membership
            }
        };
        self.scan.revision.fetch_add(1, Ordering::Relaxed);
        Ok(change)
    }
}
