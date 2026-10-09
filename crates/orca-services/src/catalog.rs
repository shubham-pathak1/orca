//! Catalog queries, collection identity and artwork selection. SQL input stays bound.
use super::{ffi, Backend};
use orca_core::db;
use rusqlite::params;
const TRACK_COLUMNS: &str = "s.id,s.path,s.title,s.artist,s.album,s.album_artist,
    COALESCE(s.genre,''),s.duration,CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_preview_url,''),NULLIF(s.artwork_url,''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END,
    CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_thumb_url,''),NULLIF(s.artwork_preview_url,''),NULLIF(s.artwork_url,''),(SELECT COALESCE(NULLIF(artwork_thumb_path,'DELETED'),NULLIF(artwork_path,'DELETED')) FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END,
    COALESCE(s.format,''),COALESCE(s.sample_rate,0),COALESCE(s.bitrate,0),COALESCE(s.track_number,0),
    CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_url,''),NULLIF(s.artwork_preview_url,''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END";

fn read_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<ffi::Track> {
    let format: String = row.get(10)?;
    let rate: u32 = row.get(11)?;
    let bitrate: u32 = row.get(12)?;
    let mut quality = vec![format];
    if rate > 0 {
        quality.push(format!("{} kHz", rate as f64 / 1000.0));
    }
    if bitrate > 0 {
        quality.push(format!("{bitrate} kbps"));
    }
    Ok(ffi::Track {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        album: row.get(4)?,
        album_artist: row.get(5)?,
        genre: row.get(6)?,
        duration_ms: row.get(7)?,
        artwork: row.get(8)?,
        artwork_thumb: row.get(9)?,
        artwork_original: row.get(14)?,
        quality: quality.join(" | "),
        track_number: row.get(13)?,
    })
}

// User input is always bound. Only allowlisted SQL identifiers are interpolated.
pub(super) fn predicate(kind: &str) -> &'static str {
    match kind {
        "artists" => "s.artist = ?2",
        "albums" => "s.album = ?2 AND s.album_artist = ?3",
        "genres" => "COALESCE(s.genre,'Unknown') = ?2",
        "folders" if cfg!(windows) => "substr(replace(s.path,char(92),'/'),1,length(rtrim(replace(?2,char(92),'/'),'/'))+1) = (rtrim(replace(?2,char(92),'/'),'/') || '/') COLLATE NOCASE",
        "folders" => "substr(s.path,1,length(rtrim(?2,'/'))+1) = (rtrim(?2,'/') || '/') COLLATE BINARY",
        "playlists" => {
            "s.id IN (SELECT song_id FROM playlist_songs WHERE playlist_id = CAST(?2 AS INTEGER))"
        }
        _ => "1=1",
    }
}

fn search_pattern(search: &str) -> String {
    format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
fn query_filter(query: &ffi::Query) -> String {
    let search = if query.search.trim().is_empty() {
        "?1 IS NOT NULL"
    } else {
        "(s.title LIKE ?1 ESCAPE '\\' OR s.artist LIKE ?1 ESCAPE '\\' OR s.album LIKE ?1 ESCAPE '\\' OR s.format LIKE ?1 ESCAPE '\\')"
    };
    format!(
        "{search} AND ({}) AND (?2 IS NOT NULL AND ?3 IS NOT NULL)",
        predicate(&query.kind)
    )
}

impl Backend {
    pub fn locate(&self, query: &ffi::Query, letter: &str) -> Result<u32, String> {
        if letter == "#" {
            return Ok(0);
        }
        let sort = match query.sort.as_str() {
            "artist" => "s.artist",
            "album" => "s.album",
            _ => "s.title",
        };
        let sql = format!("SELECT COUNT(*) FROM songs s WHERE (s.title LIKE ?1 ESCAPE '\\' OR s.artist LIKE ?1 ESCAPE '\\' OR s.album LIKE ?1 ESCAPE '\\' OR s.format LIKE ?1 ESCAPE '\\') AND ({}) AND (?2 IS NOT NULL AND ?3 IS NOT NULL) AND {sort} < ?4 COLLATE NOCASE", predicate(&query.kind));
        self.conn
            .query_row(
                &sql,
                params![
                    search_pattern(query.search.trim()),
                    query.key,
                    query.secondary,
                    letter
                ],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
    pub fn context_paths(&self, query: &ffi::Query) -> Result<Vec<String>, String> {
        let filter = query_filter(query);
        let sql = format!(
            "SELECT s.path FROM songs s WHERE {filter} ORDER BY {}",
            query_order(query)
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                params![
                    search_pattern(query.search.trim()),
                    query.key,
                    query.secondary
                ],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn page(&self, query: &ffi::Query, offset: u32, limit: u32) -> Result<ffi::Page, String> {
        let filter = query_filter(query);
        let search = search_pattern(query.search.trim());
        let total = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM songs s WHERE {filter}"),
                params![search, query.key, query.secondary],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let order = query_order(query);
        // Select page IDs before resolving artwork fallback subqueries. Deep
        // offsets must not assemble every skipped track's media presentation.
        let sql = format!("SELECT {TRACK_COLUMNS} FROM songs s JOIN (SELECT s.id FROM songs s WHERE {filter} ORDER BY {order} LIMIT ?4 OFFSET ?5) page ON page.id=s.id ORDER BY {order}");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let tracks = stmt
            .query_map(
                params![
                    search,
                    query.key,
                    query.secondary,
                    limit.clamp(1, 256),
                    offset
                ],
                read_track,
            )
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ffi::Page {
            total,
            offset,
            tracks,
        })
    }

    pub fn track(&self, path: &str) -> Result<ffi::Track, String> {
        self.conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM songs s WHERE s.path=?1"),
                [path],
                read_track,
            )
            .map_err(|e| e.to_string())
    }

    pub fn statistics(&self) -> Result<ffi::Statistics, String> {
        let (songs, artists, albums, genres) = self.conn.query_row(
            "SELECT COUNT(*),COUNT(DISTINCT artist),COUNT(DISTINCT album_artist || char(0) || album),COUNT(DISTINCT COALESCE(genre,'Unknown')) FROM songs", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|e| e.to_string())?;
        let playlists = self
            .conn
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let roots = self.library_roots()?;
        Ok(ffi::Statistics {
            songs,
            artists,
            albums,
            genres,
            playlists,
            roots,
        })
    }

    pub fn groups(&self, kind: &str, search: &str) -> Result<Vec<ffi::Group>, String> {
        if kind == "folders" {
            return self.folder_groups("", search);
        }
        let mut groups = self.raw_groups(kind, "")?;
        let overrides = self.collection_overrides()?;
        for group in &mut groups {
            let key = serde_json::json!([kind, group.key, group.secondary]).to_string();
            if let Some(entry) = overrides.get(&key) {
                if let Some(name) = entry["name"].as_str() {
                    group.title = name.into();
                }
                if let Some(cover) = entry["cover"]
                    .as_str()
                    .filter(|cover| !matches!(kind, "albums" | "artists") || !cover.is_empty())
                {
                    group.artwork = cover.into();
                    group.artwork_tiles.clear();
                }
            }
        }
        let search = search.to_lowercase();
        groups.retain(|g| {
            g.title.to_lowercase().contains(&search) || g.subtitle.to_lowercase().contains(&search)
        });
        groups.sort_by_key(|g| g.title.to_lowercase());
        Ok(groups)
    }
    pub(super) fn collection_overrides(
        &self,
    ) -> Result<serde_json::Map<String, serde_json::Value>, String> {
        let text =
            db::get_setting(&self.conn, "collection_overrides").unwrap_or_else(|| "{}".into());
        serde_json::from_str(&text).map_err(|e| format!("Invalid collection settings: {e}"))
    }
    pub(super) fn clear_collection_cover_override(
        &self,
        kind: &str,
        key: &str,
        secondary: &str,
    ) -> Result<(), String> {
        let mut overrides = self.collection_overrides()?;
        if let Some(entry) = overrides
            .get_mut(&serde_json::json!([kind, key, secondary]).to_string())
            .and_then(serde_json::Value::as_object_mut)
        {
            entry.remove("cover");
            entry.remove("thumb");
            db::set_setting(
                &self.conn,
                "collection_overrides",
                &serde_json::to_string(&overrides).map_err(|e| e.to_string())?,
            )?;
        }
        Ok(())
    }
    pub(super) fn raw_groups(&self, kind: &str, search: &str) -> Result<Vec<ffi::Group>, String> {
        let search_lower = search.to_lowercase();
        // Preserve production cover overrides and DELETED markers.
        if kind == "artists" {
            return Ok(db::get_artists(&self.conn)?
                .into_iter()
                .filter(|a| a.name.to_lowercase().contains(&search_lower))
                .map(|a| ffi::Group {
                    key: a.name.clone(),
                    secondary: String::new(),
                    title: a.name,
                    subtitle: format!("{} songs", a.song_count),
                    artwork: a
                        .artwork_thumb
                        .or(a.artwork)
                        .or(a.song_artwork_thumb)
                        .or(a.song_artwork)
                        .unwrap_or_default(),
                    count: a.song_count as u32,
                    artwork_tiles: Vec::new(),
                })
                .collect());
        }
        if kind == "albums" {
            return Ok(db::get_albums(&self.conn)?
                .into_iter()
                .filter(|a| {
                    a.title.to_lowercase().contains(&search_lower)
                        || a.artist.to_lowercase().contains(&search_lower)
                })
                .map(|a| ffi::Group {
                    key: a.title.clone(),
                    secondary: a.artist.clone(),
                    title: a.title,
                    subtitle: a.artist,
                    artwork: a.artwork.or(a.artwork_thumb).unwrap_or_default(),
                    count: a.song_count as u32,
                    artwork_tiles: Vec::new(),
                })
                .collect());
        }
        if kind == "genres" {
            let mut tiles = self.conn.prepare(
                "SELECT COALESCE(artwork_preview_url,artwork_thumb_url,artwork_url) AS cover FROM songs WHERE genre=?1 AND COALESCE(artwork_preview_url,artwork_thumb_url,artwork_url) IS NOT NULL GROUP BY cover ORDER BY MIN(title) COLLATE NOCASE LIMIT 4")
                .map_err(|e| e.to_string())?;
            return db::get_genres(&self.conn)?
                .into_iter()
                .filter(|g| g.name.to_lowercase().contains(&search_lower))
                .map(|g| {
                    let mut artwork_tiles = tiles
                        .query_map([&g.name], |row| row.get::<_, String>(0))
                        .map_err(|e| e.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?;
                    if artwork_tiles.is_empty() {
                        if let Some(path) = &g.song_artwork {
                            artwork_tiles.push(path.clone());
                        }
                    }
                    Ok(ffi::Group {
                        key: g.name.clone(),
                        secondary: String::new(),
                        title: g.name,
                        subtitle: format!("{} songs", g.song_count),
                        artwork: g.song_artwork.or(g.song_artwork_thumb).unwrap_or_default(),
                        count: g.song_count as u32,
                        artwork_tiles,
                    })
                })
                .collect();
        }
        if kind == "playlists" {
            return db::get_playlists(&self.conn)?
                .into_iter()
                .filter(|p| p.name.to_lowercase().contains(&search.to_lowercase()))
                .map(|p| {
                    let artwork = self.playlist_artwork(p.id, p.cover_path)?;
                    Ok(ffi::Group {
                        key: p.id.to_string(),
                        secondary: String::new(),
                        title: p.name,
                        subtitle: format!("{} songs", p.song_count),
                        artwork: artwork.unwrap_or_default(),
                        count: p.song_count as u32,
                        artwork_tiles: Vec::new(),
                    })
                })
                .collect::<Result<Vec<_>, String>>();
        }
        let column = match kind {
            "artists" => "artist",
            "albums" => "album",
            "genres" => "COALESCE(genre,'Unknown')",
            _ => return Err("Unknown group type".into()),
        };
        let secondary = if kind == "albums" {
            "album_artist"
        } else {
            "''"
        };
        let sql = format!("SELECT {column},{secondary},COUNT(*),COALESCE(MAX(artwork_preview_url),MAX(artwork_url),'') FROM songs WHERE {column} LIKE ?1 ESCAPE '\\' GROUP BY {column},{secondary} ORDER BY {column} COLLATE NOCASE");
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([search_pattern(search)], |r| {
                let key: String = r.get(0)?;
                let secondary: String = r.get(1)?;
                let count: u32 = r.get(2)?;
                Ok(ffi::Group {
                    title: key.clone(),
                    subtitle: if kind == "albums" {
                        secondary.clone()
                    } else {
                        format!("{count} songs")
                    },
                    key,
                    secondary,
                    artwork: r.get(3)?,
                    count,
                    artwork_tiles: Vec::new(),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}

fn query_order(query: &ffi::Query) -> String {
    if query.kind == "folders" {
        return "s.path COLLATE NOCASE,s.id".into();
    }
    // Songs have no insertion timestamp; IDs retain their insertion order.
    if query.sort == "recent" && query.kind.is_empty() {
        return "s.id DESC".into();
    }
    let sort = match query.sort.as_str() {
        "artist" => "s.artist",
        "album" => "s.album",
        _ => "s.title",
    };
    match query.kind.as_str() {
            "albums" => "COALESCE(s.track_number,999),s.title COLLATE NOCASE,s.id".to_string(),
            "playlists" => "(SELECT ps.position FROM playlist_songs ps WHERE ps.playlist_id=CAST(?2 AS INTEGER) AND ps.song_id=s.id),s.id".to_string(),
            _ if sort=="s.title"=>"s.title COLLATE NOCASE,s.id".into(),
            _ => format!("{sort} COLLATE NOCASE,s.title COLLATE NOCASE,s.id"),
        }
}
