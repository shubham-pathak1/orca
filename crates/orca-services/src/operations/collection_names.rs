//! Collection name edits write one tag per file, with bounded per-file recovery.
use crate::{operation_types::CollectionIdentity, types::Query, Backend};
use orca_core::atomic_file::FileVersion;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) fn rename(
    backend: &Backend,
    identity: &CollectionIdentity,
    name: &str,
    stop: Option<&AtomicBool>,
) -> Result<(), String> {
    let kind = identity.kind.as_str();
    let paths = backend.context_paths(&Query {
        kind: kind.into(),
        key: identity.key.clone(),
        secondary: identity.secondary.clone(),
        ..Default::default()
    })?;
    // Snapshot membership before changing tags: paging a shrinking collection
    // while saving would skip tracks. Preflight every source before the first write.
    let mut versions = Vec::with_capacity(paths.len());
    for path in &paths {
        cancelled(stop)?;
        let file = Path::new(path);
        let version = FileVersion::read(file)
            .map_err(|_| "Collection edit stopped before saving: a music file is unavailable")?
            .token();
        let metadata = file
            .metadata()
            .map_err(|_| "Collection edit stopped before saving: a music file is unavailable")?;
        if metadata.permissions().readonly() {
            return Err("Collection edit stopped before saving: a music file is read-only".into());
        }
        orca_core::library::scan_music_file(file, &backend.data_dir.join("artwork"))
            .map_err(|_| "Collection edit stopped before saving: a music file could not be read")?;
        if FileVersion::read(file).map_err(|e| e.to_string())?.token() != version {
            return Err("Collection edit stopped before saving: a music file changed. Reopen the collection before retrying.".into());
        }
        versions.push(version);
    }
    for (index, (path, version)) in paths.iter().zip(&versions).enumerate() {
        let result = cancelled(stop)
            .and_then(|()| backend.write_collection_name(path, kind, &identity.key, name, version));
        if let Err(error) = result {
            eprintln!("Orca collection rename: {error}");
            return Err(format!("Collection edit stopped after {index} of {} songs. Some tags may already be saved. Reopen the collection before retrying.", paths.len()));
        }
    }
    Ok(())
}

fn cancelled(stop: Option<&AtomicBool>) -> Result<(), String> {
    if stop.is_some_and(|stop| stop.load(Ordering::Relaxed)) {
        Err("Operation cancelled".into())
    } else {
        Ok(())
    }
}
