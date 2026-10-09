//! Metadata writes and recovery coordination.
mod lyrics;
mod playlists;
mod waveform;
use super::{
    types::{MetadataDocument, MetadataEdit, MetadataFields},
    Backend,
};
use orca_core::{db, library};
use std::{fs, path::Path, sync::atomic::Ordering};
pub(super) fn read_text(path: &Path, limit: u64) -> Result<String, String> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > limit {
        return Err("Text file exceeds the supported size".into());
    }
    Ok(text)
}
#[derive(serde::Serialize, serde::Deserialize)]
struct MetadataRecovery {
    path: String,
    before_version: String,
    cover_removed: bool,
}
fn recovery_path(directory: &Path) -> std::path::PathBuf {
    directory.join("metadata-save-recovery.json")
}
static METADATA_SAVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub(super) fn recover_metadata(
    conn: &rusqlite::Connection,
    directory: &Path,
) -> Result<(), String> {
    let _guard = METADATA_SAVE_LOCK
        .lock()
        .map_err(|_| "Metadata recovery lock unavailable")?;
    recover_metadata_locked(conn, directory)
}
fn recover_metadata_locked(conn: &rusqlite::Connection, directory: &Path) -> Result<(), String> {
    let journal = recovery_path(directory);
    use std::io::Read;
    let file = match fs::File::open(&journal) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Metadata recovery document is too large".into());
    }
    let recovery: MetadataRecovery = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Cannot read metadata recovery document: {e}"))?;
    let path = Path::new(&recovery.path);
    let indexed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM songs WHERE path=?1)",
            [&recovery.path],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if indexed
        && path.is_file()
        && orca_core::atomic_file::FileVersion::read(path)
            .map_err(|e| e.to_string())?
            .token()
            != recovery.before_version
    {
        // Recover the file that actually committed, never a stale editor draft.
        let song = library::scan_music_file(path, &directory.join("artwork"))?;
        let lyrics = song.lyrics.as_deref().unwrap_or_default();
        db::save_edited_song(
            conn,
            &song,
            lyrics,
            recovery.cover_removed && song.artwork.is_none(),
        )?;
    }
    fs::remove_file(journal).map_err(|e| e.to_string())
}

impl Backend {
    pub fn metadata(&self, path: &str) -> Result<String, String> {
        serde_json::to_string(&self.read_metadata(path)?).map_err(|e| e.to_string())
    }
    pub fn read_metadata(&self, path: &str) -> Result<MetadataDocument, String> {
        self.track(path)?;
        let file_version = orca_core::atomic_file::FileVersion::read(Path::new(path))
            .map_err(|e| e.to_string())?
            .token();
        let mut song = library::scan_music_file(Path::new(path), &self.data_dir.join("artwork"))?;
        let lyrics = self.lyrics(path)?;
        song.lyrics = if lyrics.trim().is_empty() {
            None
        } else {
            Some(lyrics)
        };
        // The editor scans the file afresh. Reconcile the index too, so the
        // library cannot keep showing artwork that was removed from the file.
        db::save_songs_to_db(&self.conn, &[song.clone()])?;
        self.scan.revision.fetch_add(1, Ordering::Relaxed);
        if song.artwork.is_none() {
            let artwork = self.track(path)?.artwork_original;
            song.artwork = (!artwork.is_empty()).then_some(artwork);
        }
        if orca_core::atomic_file::FileVersion::read(Path::new(path))
            .map_err(|e| e.to_string())?
            .token()
            != file_version
        {
            return Err(
                "This audio file changed while the editor was opening. Reopen it and try again."
                    .into(),
            );
        }
        Ok(MetadataDocument {
            fields: MetadataFields {
                path: song.path,
                title: song.title,
                artist: song.artist,
                album: song.album,
                album_artist: song.album_artist,
                year: song.year,
                track_number: song.track_number,
                disc_number: song.disc_number,
                genre: song.genre,
                lyrics: song.lyrics,
            },
            artwork: song.artwork,
            file_version,
        })
    }

    pub fn save_metadata(&self, json: &str) -> Result<(), String> {
        if json.len() > 1024 * 1024 {
            return Err("Metadata document exceeds 1 MB".into());
        }
        let edit: MetadataEdit = serde_json::from_str(json).map_err(|e| e.to_string())?;
        self.write_metadata(edit)
    }
    pub fn write_metadata(&self, edit: MetadataEdit) -> Result<(), String> {
        let _guard = METADATA_SAVE_LOCK
            .lock()
            .map_err(|_| "Metadata save lock unavailable")?;
        let cover = edit
            .cover_to_embed
            .as_deref()
            .filter(|path| !path.is_empty())
            .map(Path::new);
        let fields = edit.fields;
        let bytes = [
            &fields.path,
            &fields.title,
            &fields.artist,
            &fields.album,
            &fields.album_artist,
        ]
        .iter()
        .map(|value| value.len())
        .sum::<usize>()
            + fields.genre.as_ref().map_or(0, String::len)
            + fields.lyrics.as_ref().map_or(0, String::len);
        if bytes > 1024 * 1024 {
            return Err("Metadata document exceeds 1 MB".into());
        }
        let update = library::SongMetadataUpdate {
            path: fields.path,
            title: fields.title,
            artist: fields.artist,
            album: fields.album,
            album_artist: fields.album_artist,
            year: fields.year,
            track_number: fields.track_number,
            disc_number: fields.disc_number,
            genre: fields.genre,
            lyrics: fields.lyrics,
        };
        if [
            &update.path,
            &update.title,
            &update.artist,
            &update.album,
            &update.album_artist,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err("Enter a title, artist, album, and album artist".into());
        }
        if [update.year, update.track_number, update.disc_number]
            .into_iter()
            .flatten()
            .any(|value| value <= 0)
        {
            return Err("Invalid year, track, or disc: use positive whole numbers".into());
        }
        self.track(&update.path)?;
        let path = update.path.clone();
        let saved_lyrics = update
            .lyrics
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        recover_metadata_locked(&self.conn, &self.data_dir)?;
        let before_version = orca_core::atomic_file::FileVersion::read(Path::new(&path))
            .map_err(|e| e.to_string())?
            .token();
        let expected_version = edit.file_version.as_deref().filter(|s| !s.is_empty());
        if expected_version.is_some_and(|expected| expected != before_version) {
            return Err(
                "This audio file changed since the editor opened. Reopen it and try again.".into(),
            );
        }
        let remove_cover = edit.remove_cover && cover.is_none();
        let journal = recovery_path(&self.data_dir);
        let recovery = MetadataRecovery {
            path: path.clone(),
            before_version: before_version.clone(),
            cover_removed: remove_cover,
        };
        orca_core::atomic_file::write(
            &journal,
            &serde_json::to_vec(&recovery).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if let Err(error) = library::update_song_metadata_checked(
            update,
            cover,
            remove_cover,
            Some(&before_version),
        ) {
            let _ = fs::remove_file(journal);
            return Err(error);
        }
        self.scan.revision.fetch_add(1, Ordering::Relaxed);
        let finish = (|| {
            let song = library::scan_music_file(Path::new(&path), &self.data_dir.join("artwork"))?;
            db::save_edited_song(&self.conn, &song, &saved_lyrics, remove_cover)?;
            fs::remove_file(journal).map_err(|e| e.to_string())
        })();
        finish.map_err(|error| format!("Metadata saved but library update failed: {error}"))?;
        Ok(())
    }

    pub(super) fn write_collection_name(
        &self,
        path: &str,
        kind: &str,
        old_name: &str,
        name: &str,
        before_version: &str,
    ) -> Result<(), String> {
        let _guard = METADATA_SAVE_LOCK
            .lock()
            .map_err(|_| "Metadata save lock unavailable")?;
        recover_metadata_locked(&self.conn, &self.data_dir)?;
        let lyrics = self.lyrics(path)?;
        let removed_cover = self
            .conn
            .query_row(
                "SELECT artwork_url='DELETED' FROM songs WHERE path=?1",
                [path],
                |row| row.get::<_, Option<bool>>(0),
            )
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
        let journal = recovery_path(&self.data_dir);
        orca_core::atomic_file::write(
            &journal,
            &serde_json::to_vec(&MetadataRecovery {
                path: path.into(),
                before_version: before_version.into(),
                cover_removed: removed_cover,
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if let Err(error) = library::update_collection_name_checked(
            Path::new(path),
            kind,
            old_name,
            name,
            before_version,
        ) {
            let _ = fs::remove_file(journal);
            return Err(error);
        }
        self.scan.revision.fetch_add(1, Ordering::Relaxed);
        let song = library::scan_music_file(Path::new(path), &self.data_dir.join("artwork"))?;
        db::save_edited_song(&self.conn, &song, &lyrics, removed_cover)?;
        fs::remove_file(journal).map_err(|e| e.to_string())
    }
}
