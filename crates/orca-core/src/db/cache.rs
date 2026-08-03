use rusqlite::{params, Connection, OptionalExtension};

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_lyrics(conn: &Connection, song_path: &str) -> Option<String> {
    conn.query_row(
        "SELECT lyrics_text FROM lyrics WHERE song_path = ?1",
        params![song_path],
        |row| row.get(0),
    )
    .ok()
}

pub fn set_lyrics(conn: &Connection, song_path: &str, lyrics_text: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO lyrics (song_path, lyrics_text) VALUES (?1, ?2)
         ON CONFLICT(song_path) DO UPDATE SET lyrics_text = excluded.lyrics_text",
        params![song_path, lyrics_text],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_cached_waveform(
    conn: &Connection,
    song_path: &str,
    buckets: usize,
) -> Result<Option<Vec<f32>>, String> {
    let peaks: Option<String> = conn
        .query_row(
            "SELECT peaks FROM waveforms WHERE song_path = ?1 AND buckets = ?2",
            params![song_path, buckets as i64],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    Ok(peaks.map(|value| {
        value
            .split(',')
            .filter_map(|peak| peak.parse::<f32>().ok())
            .collect()
    }))
}

pub fn save_waveform(
    conn: &Connection,
    song_path: &str,
    buckets: usize,
    peaks: &[f32],
) -> Result<(), String> {
    let serialized_peaks = peaks
        .iter()
        .map(|peak| peak.to_string())
        .collect::<Vec<_>>()
        .join(",");

    conn.execute(
        "INSERT INTO waveforms (song_path, buckets, peaks)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(song_path, buckets) DO UPDATE SET peaks = excluded.peaks",
        params![song_path, buckets as i64, serialized_peaks],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
