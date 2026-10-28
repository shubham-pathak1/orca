//! Catalog queries, collection identity and artwork selection. SQL input stays bound.
use super::{ffi, Backend};
use orca_core::db;
use rusqlite::params;
const TRACK_COLUMNS: &str = "s.id,s.path,s.title,s.artist,s.album,s.album_artist,
    COALESCE(s.genre,''),s.duration,CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_preview_url,''),NULLIF(s.artwork_url,''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END,
    CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_thumb_url,''),NULLIF(s.artwork_preview_url,''),NULLIF(s.artwork_url,''),(SELECT COALESCE(NULLIF(artwork_thumb_path,'DELETED'),NULLIF(artwork_path,'DELETED')) FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END,
    COALESCE(s.format,''),COALESCE(s.sample_rate,0),COALESCE(s.bitrate,0),COALESCE(s.track_number,0),
    CASE WHEN s.artwork_url='DELETED' THEN '' ELSE COALESCE(NULLIF(s.artwork_url,''),NULLIF(s.artwork_preview_url,''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') END,
    COALESCE(s.explicit_advisory,0)";

pub(super) struct GroupCacheEntry {
    pub(super) revision: (i64, i64),
    pub(super) groups: Vec<ffi::Group>,
}

pub(super) fn filter_groups(mut groups: Vec<ffi::Group>, search: &str) -> Vec<ffi::Group> {
    let search = search.to_lowercase();
    groups.retain(|group| {
        group.title.to_lowercase().contains(&search)
            || group.subtitle.to_lowercase().contains(&search)
    });
    groups
}

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
        explicit: row.get(15)?,
        quality: quality.join(" | "),
        track_number: row.get(13)?,
    })
}

// User input is always bound. Only allowlisted SQL identifiers are interpolated.
pub(super) fn predicate(kind: &str) -> &'static str {
    match kind {
        "artists" => "s.id IN (SELECT song_id FROM song_primary_artists WHERE name=?2 COLLATE NOCASE)",
        "albums" => "s.album = ?2 AND s.album_artist = ?3",
        "genres" => "(s.genre = ?2 OR (?2='Unknown' AND s.genre IS NULL))",
        "folders" if cfg!(windows) => "replace(s.path,char(92),'/') >= (rtrim(replace(?2,char(92),'/'),'/') || '/') COLLATE NOCASE AND replace(s.path,char(92),'/') < (rtrim(replace(?2,char(92),'/'),'/') || '0') COLLATE NOCASE",
        "folders" => "s.path >= (rtrim(?2,'/') || '/') COLLATE BINARY AND s.path < (rtrim(?2,'/') || '0') COLLATE BINARY",
        "playlists" => {
            "((?2='liked' AND s.id IN (SELECT song_id FROM liked_songs)) OR (?2!='liked' AND s.id IN (SELECT song_id FROM playlist_songs WHERE playlist_id = CAST(?2 AS INTEGER))))"
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
    pub fn group_exists(&self, kind: &str, key: &str, secondary: &str) -> Result<bool, String> {
        if kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        if kind == "folders" {
            return self.folder_exists(key);
        }
        if kind == "playlists" {
            if key == "liked" {
                return Ok(true);
            }
            let Ok(id) = key.parse::<i64>() else {
                return Ok(false);
            };
            return self
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                    [id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string());
        }
        if !matches!(kind, "artists" | "albums" | "genres") {
            return Ok(false);
        }
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM active_songs s WHERE {} AND (?3 IS NOT NULL) LIMIT 1)",
            predicate(kind)
        );
        self.conn
            .query_row(&sql, params!["", key, secondary], |row| row.get(0))
            .map_err(|error| error.to_string())
    }
    pub fn locate(&self, query: &ffi::Query, letter: &str) -> Result<u32, String> {
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        if letter == "#" {
            return Ok(0);
        }
        let sort = match query.sort.as_str() {
            "artist" => "s.artist",
            "album" => "s.album",
            _ => "s.title",
        };
        let sql = format!("SELECT COUNT(*) FROM active_songs s WHERE (s.title LIKE ?1 ESCAPE '\\' OR s.artist LIKE ?1 ESCAPE '\\' OR s.album LIKE ?1 ESCAPE '\\' OR s.format LIKE ?1 ESCAPE '\\') AND ({}) AND (?2 IS NOT NULL AND ?3 IS NOT NULL) AND {sort} < ?4 COLLATE NOCASE", predicate(&query.kind));
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
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        let filter = query_filter(query);
        let sql = format!(
            "SELECT s.path FROM active_songs s WHERE {filter} ORDER BY {}",
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

    pub fn context_paths_limited(
        &self,
        query: &ffi::Query,
        limit: u32,
    ) -> Result<Vec<String>, String> {
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        let sql = format!(
            "SELECT s.path FROM active_songs s WHERE {} ORDER BY {} LIMIT ?4",
            query_filter(query),
            query_order(query)
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![
                    search_pattern(query.search.trim()),
                    query.key,
                    query.secondary,
                    limit
                ],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    /// Playback order uses compact database IDs. The UI never needs to hold
    /// or serialize every file path in a large collection.
    pub fn context_ids(&self, query: &ffi::Query) -> Result<Vec<i64>, String> {
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        let sql = format!(
            "SELECT s.id FROM active_songs s WHERE {} ORDER BY {}",
            query_filter(query),
            query_order(query)
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let ids = stmt
            .query_map(
                params![
                    search_pattern(query.search.trim()),
                    query.key,
                    query.secondary
                ],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ids)
    }

    /// Resolve one visible page from a previously ordered ID snapshot. Missing
    /// rows retain their slots until the next library revision refreshes it.
    pub fn tracks_by_ids(&self, ids: &[i64]) -> Result<Vec<ffi::Track>, String> {
        use rusqlite::OptionalExtension;
        let mut statement = self
            .conn
            .prepare_cached(&format!(
                "SELECT {TRACK_COLUMNS} FROM active_songs s WHERE s.id=?1"
            ))
            .map_err(|error| error.to_string())?;
        ids.iter()
            .map(|id| {
                statement
                    .query_row([id], read_track)
                    .optional()
                    .map(|track| track.unwrap_or_default())
                    .map_err(|error| error.to_string())
            })
            .collect()
    }

    pub fn path_for_id(&self, id: i64) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.conn
            .query_row("SELECT path FROM active_songs WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn id_for_path(&self, path: &str) -> Result<Option<i64>, String> {
        use rusqlite::OptionalExtension;
        self.conn
            .query_row("SELECT id FROM active_songs WHERE path=?1", [path], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn ids_for_paths(&self, paths: &[String]) -> Result<Vec<i64>, String> {
        use rusqlite::OptionalExtension;
        let mut lookup = self
            .conn
            .prepare_cached("SELECT id FROM active_songs WHERE path=?1")
            .map_err(|error| error.to_string())?;
        let mut ids = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(id) = lookup
                .query_row([path], |row| row.get(0))
                .optional()
                .map_err(|error| error.to_string())?
            {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub fn page(&self, query: &ffi::Query, offset: u32, limit: u32) -> Result<ffi::Page, String> {
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        let filter = query_filter(query);
        let search = search_pattern(query.search.trim());
        let total = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM active_songs s WHERE {filter}"),
                params![search, query.key, query.secondary],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let tracks = self.page_tracks(query, offset, limit)?;
        Ok(ffi::Page {
            total,
            offset,
            tracks,
        })
    }

    /// Fetch visible rows without waiting for collection counts or navigation IDs.
    pub fn page_tracks(
        &self,
        query: &ffi::Query,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<ffi::Track>, String> {
        if query.kind == "artists" {
            db::refresh_artist_index(&self.conn)?;
        }
        let filter = query_filter(query);
        let search = search_pattern(query.search.trim());
        let order = query_order(query);
        // Select page IDs before resolving artwork fallback subqueries. Deep
        // offsets must not assemble every skipped track's media presentation.
        let sql = format!("SELECT {TRACK_COLUMNS} FROM active_songs s JOIN (SELECT s.id FROM active_songs s WHERE {filter} ORDER BY {order} LIMIT ?4 OFFSET ?5) page ON page.id=s.id ORDER BY {order}");
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
        Ok(tracks)
    }

    pub fn track(&self, path: &str) -> Result<ffi::Track, String> {
        self.conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM active_songs s WHERE s.path=?1"),
                [path],
                read_track,
            )
            .map_err(|e| e.to_string())
    }

    pub fn statistics(&self) -> Result<ffi::Statistics, String> {
        // Separate counts can use their covering indexes instead of scanning
        // wide song rows for a combined metadata aggregate.
        let songs = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM songs INDEXED BY idx_active_id WHERE library_visible=1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let albums = self.conn.query_row("SELECT COUNT(*) FROM (SELECT 1 FROM songs INDEXED BY idx_active_album_summary WHERE library_visible=1 GROUP BY album_artist,album)", [], |row| row.get(0)).map_err(|error| error.to_string())?;
        let genres = self.conn.query_row("SELECT COUNT(DISTINCT COALESCE(genre,'Unknown')) FROM songs INDEXED BY idx_active_genre WHERE library_visible=1", [], |row| row.get(0)).map_err(|error| error.to_string())?;
        db::refresh_artist_index(&self.conn)?;
        let artists = self.conn.query_row("SELECT COUNT(DISTINCT pa.name) FROM song_primary_artists pa JOIN songs s INDEXED BY idx_active_id ON s.id=pa.song_id WHERE s.library_visible=1", [], |row| row.get(0)).map_err(|error| error.to_string())?;
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
        let revision = self.group_revision()?;
        if let Some(cached) = self.group_cache.borrow().get(kind) {
            if cached.revision == revision {
                return Ok(filter_groups(cached.groups.clone(), search));
            }
        }
        let mut groups = self.raw_groups(kind, "")?;
        let overrides = self.collection_overrides()?;
        for group in &mut groups {
            let key = serde_json::json!([kind, group.key, group.secondary]).to_string();
            if let Some(entry) = overrides.get(&key) {
                if let Some(name) = entry["name"].as_str() {
                    group.title = name.into();
                }
                if let Some(cover) = entry["cover"].as_str().filter(|cover| {
                    !(matches!(kind, "albums" | "artists")
                        || kind == "playlists" && group.key == "liked")
                        || !cover.is_empty()
                }) {
                    group.artwork = cover.into();
                    group.artwork_tiles.clear();
                }
            }
        }
        groups.sort_by_key(|g| {
            (
                kind == "playlists" && g.key != "liked",
                g.title.to_lowercase(),
            )
        });
        if self.group_revision()? == revision {
            self.group_cache.borrow_mut().insert(
                kind.into(),
                GroupCacheEntry {
                    revision,
                    groups: groups.clone(),
                },
            );
        }
        Ok(filter_groups(groups, search))
    }
    pub(super) fn group_revision(&self) -> Result<(i64, i64), String> {
        let local = self
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        let external = self
            .conn
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        Ok((local, external))
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
        // Honor saved cover overrides and explicit artwork removals.
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
                "SELECT COALESCE(artwork_preview_url,artwork_thumb_url,artwork_url) AS cover FROM active_songs WHERE genre=?1 AND COALESCE(artwork_preview_url,artwork_thumb_url,artwork_url) IS NOT NULL GROUP BY cover ORDER BY MIN(title) COLLATE NOCASE LIMIT 4")
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
            let liked = db::liked_songs_summary(&self.conn)?;
            let mut groups = vec![ffi::Group {
                key: "liked".into(),
                secondary: String::new(),
                title: "Liked Songs".into(),
                subtitle: format!("{} songs", liked.count),
                artwork: liked.artwork,
                count: liked.count,
                artwork_tiles: Vec::new(),
            }];
            groups.extend(
                db::get_playlists(&self.conn)?
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
                    .collect::<Result<Vec<_>, String>>()?,
            );
            return Ok(groups);
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
        let sql = format!("SELECT {column},{secondary},COUNT(*),COALESCE(MAX(artwork_preview_url),MAX(artwork_url),'') FROM active_songs WHERE {column} LIKE ?1 ESCAPE '\\' GROUP BY {column},{secondary} ORDER BY {column} COLLATE NOCASE");
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
        return if cfg!(windows) {
            "replace(s.path,char(92),'/') COLLATE NOCASE,s.id"
        } else {
            "s.path COLLATE BINARY,s.id"
        }
        .into();
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
            "playlists" if query.key == "liked" => "(SELECT l.liked_at FROM liked_songs l WHERE l.song_id=s.id) DESC,s.id DESC".to_string(),
            "playlists" => "(SELECT ps.position FROM playlist_songs ps WHERE ps.playlist_id=CAST(?2 AS INTEGER) AND ps.song_id=s.id),s.id".to_string(),
            _ if sort=="s.title"=>"s.title COLLATE NOCASE,s.id".into(),
            _ => format!("{sort} COLLATE NOCASE,s.title COLLATE NOCASE,s.id"),
        }
}
