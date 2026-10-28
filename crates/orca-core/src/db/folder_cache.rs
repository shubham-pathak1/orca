//! Bounded, persistent folder summaries. Library writes invalidate them atomically.
use rusqlite::{params, Connection, OptionalExtension};

pub fn folder_cache_revision(conn: &Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT revision FROM folder_cache_state WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

pub fn load_folder_cache(
    conn: &Connection,
    key: &str,
    revision: i64,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT payload FROM folder_cache WHERE key=?1 AND revision=?2
         AND revision=(SELECT revision FROM folder_cache_state WHERE id=1)",
        params![key, revision],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn save_folder_cache(
    conn: &Connection,
    key: &str,
    revision: i64,
    payload: &str,
) -> Result<(), String> {
    if payload.len() > 256 * 1024 {
        return Ok(());
    }
    // A derived cache must never wait behind the scanner's write transaction.
    let timeout: u64 = conn
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::ZERO)
        .map_err(|e| e.to_string())?;
    let result = save(conn, key, revision, payload);
    conn.busy_timeout(std::time::Duration::from_millis(timeout))
        .map_err(|e| e.to_string())?;
    result
}

fn save(conn: &Connection, key: &str, revision: i64, payload: &str) -> Result<(), String> {
    // A scanner on another connection may have changed the library during a query.
    conn.execute(
        "INSERT INTO folder_cache(key,revision,payload) SELECT ?1,?2,?3
         WHERE ?2=(SELECT revision FROM folder_cache_state WHERE id=1)
         ON CONFLICT(key) DO UPDATE SET revision=excluded.revision,payload=excluded.payload",
        params![key, revision, payload],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM folder_cache WHERE rowid NOT IN (SELECT rowid FROM folder_cache ORDER BY rowid DESC LIMIT 128)", [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
