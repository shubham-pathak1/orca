//! Derived artist membership. Pure SQL dirty markers also cover older writers.
use rusqlite::Connection;

pub fn refresh_artist_index(conn: &Connection) -> Result<(), String> {
    let pending: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artist_index_pending)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !pending {
        return Ok(());
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    // Stream dirty records through the shared parser without retaining a
    // full-library Rust allocation or preparing statements for each song.
    tx.execute_batch(
        "DELETE FROM song_primary_artists WHERE song_id IN (SELECT song_id FROM artist_index_pending);
         INSERT OR IGNORE INTO song_primary_artists(song_id,name)
         SELECT s.id,n.value FROM artist_index_pending p
         JOIN songs s ON s.id=p.song_id
         CROSS JOIN json_each(primary_artist_names(s.artist,s.title)) n;
         DELETE FROM artist_index_pending;",
    ).map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}
