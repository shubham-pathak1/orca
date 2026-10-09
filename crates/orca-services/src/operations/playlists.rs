use super::path_key;
use crate::{
    operation_types::{OperationRequest, OperationResult, PlaylistExported, PlaylistImported},
    Backend,
};
use orca_core::db;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub(super) fn run(
    backend: &Backend,
    operation: &OperationRequest,
    _stop: Option<&std::sync::atomic::AtomicBool>,
) -> Result<OperationResult, String> {
    let response = match operation {
        OperationRequest::ImportPlaylist { file } => {
            let file = Path::new(file);
            let text = crate::media::read_text(file, 16 * 1024 * 1024)?;
            let base = file.parent().unwrap_or(Path::new("."));
            let paths: HashMap<_, _> = db::get_song_path_index(&backend.conn)?
                .into_iter()
                .map(|(p, id)| (path_key(Path::new(&p)), id))
                .collect();
            let mut seen = HashSet::new();
            let mut ids = Vec::new();
            let mut missing = 0;
            for line in text.lines() {
                let line = line.trim().trim_start_matches('\u{feff}');
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let line = line
                    .strip_prefix("file:///")
                    .or_else(|| line.strip_prefix("file://"))
                    .unwrap_or(line);
                let path = Path::new(line);
                let path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    base.join(path)
                };
                match paths.get(&path_key(&path)) {
                    Some(&id) if seen.insert(id) => ids.push(id),
                    Some(_) => {}
                    None => missing += 1,
                }
            }
            if ids.is_empty() {
                return Err("No tracks from this playlist are in your library".into());
            }
            let name = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported Playlist");
            let tx = backend
                .conn
                .unchecked_transaction()
                .map_err(|e| e.to_string())?;
            let playlist = db::create_playlist(&tx, name, None)?;
            for id in &ids {
                db::add_to_playlist(&tx, playlist, *id)?;
            }
            tx.commit().map_err(|e| e.to_string())?;
            OperationResult::PlaylistImported(PlaylistImported {
                name: name.into(),
                imported: ids.len(),
                unavailable: missing,
            })
        }
        OperationRequest::ExportPlaylist { id, file } => {
            let songs = db::get_playlist_export_songs(&backend.conn, *id)?;
            let mut content = String::from("#EXTM3U\n");
            for song in &songs {
                content.push_str(&format!(
                    "#EXTINF:{},{} - {}\n{}\n",
                    song.duration / 1000,
                    song.artist.replace(['\r', '\n'], " "),
                    song.title.replace(['\r', '\n'], " "),
                    song.path
                ));
            }
            orca_core::atomic_file::write(Path::new(file), content.as_bytes())
                .map_err(|e| e.to_string())?;
            OperationResult::PlaylistExported(PlaylistExported {
                exported: songs.len(),
            })
        }
        _ => return Err("Invalid playlists operation routing".into()),
    };
    Ok(response)
}
