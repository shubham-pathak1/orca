//! Move the native profile to its permanent directory without losing local artwork.
use orca_core::{atomic_file, db};
use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

const LEGACY_NAME: &str = "OrcaSlintTauri";
const PROFILE_NAME: &str = "Orca";
const MIGRATION_KEY: &str = "native_profile_directory_v1";

pub fn migrate_profile_directory(base: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(base).map_err(|error| error.to_string())?;
    let old = base.join(LEGACY_NAME);
    let current = base.join(PROFILE_NAME);

    if old.exists() && !old.is_dir() {
        return Err("The existing Orca profile location is not a directory".into());
    }
    let old_has_library = old.join("orca.db").is_file();
    let current_has_library = current.join("orca.db").is_file();
    let prefer_old = old.exists()
        && (!current_has_library || (old_has_library && !migration_complete(&current)?));
    if prefer_old {
        if current.exists() {
            if !current.is_dir() {
                return Err("The new Orca profile location is not a directory".into());
            }
            if fs::read_dir(&current)
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
            {
                let archive = (1..)
                    .map(|number| base.join(format!("Orca.pre-migration-backup-{number}")))
                    .find(|path| !path.exists())
                    .ok_or("Could not reserve a backup folder for the existing Orca profile")?;
                fs::rename(&current, &archive).map_err(|error| {
                    format!("Could not back up the existing Orca profile before migration: {error}")
                })?;
                if let Err(error) = fs::rename(&old, &current) {
                    let restored = fs::rename(&archive, &current).is_ok();
                    return Err(format!(
                        "Could not move the existing Orca profile; close other Orca windows and retry: {error}; previous profile restored: {restored}"
                    ));
                }
            } else {
                fs::remove_dir(&current).map_err(|error| error.to_string())?;
                fs::rename(&old, &current).map_err(|error| {
                    format!("Could not move the existing Orca profile; close other Orca windows and retry: {error}")
                })?;
            }
        } else {
            fs::rename(&old, &current).map_err(|error| {
                format!("Could not move the existing Orca profile; close other Orca windows and retry: {error}")
            })?;
        }
    }

    if current.join("orca.db").is_file() {
        rebase_profile_paths(base, &old, &current)?;
    }
    Ok(current)
}

fn moved_path(value: &str, old: &Path, current: &Path) -> Option<String> {
    let relative = Path::new(value).strip_prefix(old).ok()?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let destination = current.join(relative);
    destination
        .exists()
        .then(|| destination.to_string_lossy().into_owned())
}

fn rebase_value(value: &str, roots: &[(PathBuf, PathBuf)]) -> Option<String> {
    roots
        .iter()
        .find_map(|(old, current)| moved_path(value, old, current))
}

fn rebase_json(value: &mut Value, roots: &[(PathBuf, PathBuf)]) -> bool {
    match value {
        Value::String(path) => {
            if let Some(updated) = rebase_value(path, roots) {
                *path = updated;
                true
            } else {
                false
            }
        }
        Value::Array(values) => {
            let mut changed = false;
            for value in values {
                changed |= rebase_json(value, roots);
            }
            changed
        }
        Value::Object(values) => {
            let mut changed = false;
            for value in values.values_mut() {
                changed |= rebase_json(value, roots);
            }
            changed
        }
        _ => false,
    }
}

fn rebase_settings_file(current: &Path, roots: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let path = current.join("slint-settings.json");
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    let Ok(mut settings) = serde_json::from_slice::<Value>(&bytes) else {
        return Ok(());
    };
    if rebase_json(&mut settings, roots) {
        let updated = serde_json::to_vec(&settings).map_err(|error| error.to_string())?;
        atomic_file::write(&path, &updated).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn rebase_profile_paths(base: &Path, old: &Path, current: &Path) -> Result<(), String> {
    let mut roots = vec![(old.to_path_buf(), current.to_path_buf())];
    if let Ok(canonical_base) = base.canonicalize() {
        let canonical = (
            canonical_base.join(LEGACY_NAME),
            canonical_base.join(PROFILE_NAME),
        );
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }
    if migration_complete(current)? {
        return Ok(());
    }
    super::prepare_data_dir(&current.to_string_lossy())?;
    let conn = db::init_db(current.to_path_buf())?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
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
        let rows = tx
            .prepare(&format!(
                "SELECT rowid,{column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(|error| error.to_string())?
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        for (rowid, original) in rows {
            if let Some(updated) = rebase_value(&original, &roots) {
                tx.execute(
                    &format!("UPDATE {table} SET {column}=?1 WHERE rowid=?2"),
                    params![updated, rowid],
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    if let Some(raw) = db::get_setting(&tx, "collection_overrides") {
        let mut overrides: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        if rebase_json(&mut overrides, &roots) {
            db::set_setting(&tx, "collection_overrides", &overrides.to_string())?;
        }
    }
    rebase_settings_file(current, &roots)?;
    db::set_setting(&tx, MIGRATION_KEY, "complete")?;
    tx.commit().map_err(|error| error.to_string())
}

fn migration_complete(current: &Path) -> Result<bool, String> {
    let conn =
        Connection::open_with_flags(current.join("orca.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
    Ok(db::get_setting(&conn, MIGRATION_KEY).as_deref() == Some("complete"))
}
