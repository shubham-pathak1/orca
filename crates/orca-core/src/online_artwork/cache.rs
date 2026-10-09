//! Disposable lookup cache, separate from the user's library. Bound to 2048 rows.
use super::{ArtworkQuery, LookupError, LookupMode};
use crate::artwork_cache::ArtworkPaths;
use rusqlite::{Connection, OptionalExtension};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn open(directory: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let conn =
        Connection::open(directory.join("lookup-cache.sqlite3")).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS lookups(key TEXT PRIMARY KEY, saved_at INTEGER NOT NULL, full TEXT, thumb TEXT, preview TEXT); CREATE INDEX IF NOT EXISTS lookup_age ON lookups(saved_at);").map_err(|e| e.to_string())?;
    Ok(conn)
}
fn key(query: &ArtworkQuery) -> String {
    serde_json::json!([3, query]).to_string()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub(super) fn read(
    directory: &Path,
    query: &ArtworkQuery,
    mode: LookupMode,
) -> Option<Result<ArtworkPaths, LookupError>> {
    if mode == LookupMode::Manual {
        return None;
    }
    let conn = open(directory).ok()?;
    let cached: (i64, Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT saved_at,full,thumb,preview FROM lookups WHERE key=?1",
            [key(query)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .ok()??;
    let age = now().checked_sub(cached.0)?;
    if age < 0 {
        return None;
    }
    match (cached.1, cached.2, cached.3) {
        (Some(full), Some(thumb), Some(preview))
            if age < 30 * 86400
                && [&full, &thumb, &preview].iter().all(|p| {
                    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
                        && image::image_dimensions(p).is_ok()
                }) =>
        {
            Some(Ok(ArtworkPaths {
                full,
                thumb,
                preview,
            }))
        }
        (None, None, None) if mode == LookupMode::Automatic && age < 6 * 3600 => {
            Some(Err(LookupError::NotFound))
        }
        _ => None,
    }
}
pub(super) fn save(directory: &Path, query: &ArtworkQuery, paths: Option<&ArtworkPaths>) {
    let result = (|| -> Result<(), String> {
        let conn = open(directory)?;
        let tx =
            rusqlite::Transaction::new_unchecked(&conn, rusqlite::TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO lookups(key,saved_at,full,thumb,preview) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(key) DO UPDATE SET saved_at=excluded.saved_at,full=excluded.full,thumb=excluded.thumb,preview=excluded.preview", rusqlite::params![key(query), now(), paths.map(|p| &p.full), paths.map(|p| &p.thumb), paths.map(|p| &p.preview)]).map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM lookups WHERE key IN (SELECT key FROM lookups ORDER BY saved_at DESC,rowid DESC LIMIT -1 OFFSET 2048)", []).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        log::warn!("Artwork lookup cache: {error}");
    }
}
