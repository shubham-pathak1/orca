//! Detach imported profiles from external artwork caches without touching sources.
use orca_core::db;
use rusqlite::Connection;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

pub(super) fn localize(conn: &Connection, directory: &Path) -> Result<(), String> {
    if db::get_setting(conn, "native_artwork_migration_v1").as_deref() == Some("complete") {
        return Ok(());
    }
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let mut copies = HashMap::new();
    let mut complete = true;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch(
        "CREATE TEMP TABLE native_artwork_paths(original TEXT PRIMARY KEY, local TEXT NOT NULL)",
    )
    .map_err(|e| e.to_string())?;
    for (table, column) in [
        ("songs", "artwork_url"),
        ("songs", "artwork_thumb_url"),
        ("songs", "artwork_preview_url"),
        ("artist_artworks", "artwork_path"),
        ("artist_artworks", "artwork_thumb_path"),
        ("album_artworks", "artwork_path"),
        ("album_artworks", "artwork_thumb_path"),
        ("playlists", "cover_path"),
    ] {
        let paths = tx
            .prepare(&format!(
                "SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(|e| e.to_string())?
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        for original in paths {
            if let Some(local) = copy(&original, &directory, &mut copies, &mut complete)? {
                tx.execute(
                    "INSERT OR IGNORE INTO native_artwork_paths(original,local) VALUES(?1,?2)",
                    [&original, &local],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        // One indexed mapping pass avoids rescanning a large library per image.
        tx.execute_batch(&format!("UPDATE {table} SET {column}=(SELECT local FROM native_artwork_paths WHERE original={table}.{column}) WHERE {column} IN (SELECT original FROM native_artwork_paths)"))
            .map_err(|e| e.to_string())?;
    }
    if let Some(raw) = db::get_setting(&tx, "collection_overrides") {
        let mut overrides: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if let Some(entries) = overrides.as_object_mut() {
            for entry in entries.values_mut() {
                for field in ["cover", "thumb"] {
                    if let Some(original) = entry[field].as_str() {
                        if let Some(local) = copy(original, &directory, &mut copies, &mut complete)?
                        {
                            entry[field] = local.into();
                        }
                    }
                }
            }
        }
        db::set_setting(&tx, "collection_overrides", &overrides.to_string())?;
    }
    if complete {
        db::set_setting(&tx, "native_artwork_migration_v1", "complete")?;
    }
    tx.execute_batch("DROP TABLE native_artwork_paths")
        .map_err(|e| e.to_string())?;
    // Files are published before references commit; interrupted copies can be
    // retried, and a failed transaction keeps every original reference intact.
    tx.commit().map_err(|e| e.to_string())
}

fn copy(
    original: &str,
    directory: &Path,
    copies: &mut HashMap<String, String>,
    complete: &mut bool,
) -> Result<Option<String>, String> {
    let source = Path::new(original);
    if !source.is_absolute() || source.starts_with(directory) {
        return Ok(None);
    }
    if let Some(local) = copies.get(original) {
        return Ok(Some(local.clone()));
    }
    let resolved = match source.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            *complete = false;
            return Ok(None);
        }
        Err(error) => return Err(format!("Artwork migration: {error}")),
    };
    if resolved.starts_with(directory) {
        return Ok(None);
    }
    let bytes = fs::read(&resolved).map_err(|e| format!("Artwork migration: {e}"))?;
    let mut hash = DefaultHasher::new();
    bytes.hash(&mut hash);
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("img");
    let destination = directory
        .join("artwork/imported")
        .join(format!("{:016x}.{extension}", hash.finish()));
    fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
    if destination.exists() {
        if fs::read(&destination).map_err(|e| e.to_string())? != bytes {
            return Err("Artwork migration: conflicting cached file".into());
        }
    } else {
        orca_core::atomic_file::write(&destination, &bytes).map_err(|e| e.to_string())?;
    }
    let local = destination.to_string_lossy().into_owned();
    copies.insert(original.into(), local.clone());
    Ok(Some(local))
}
