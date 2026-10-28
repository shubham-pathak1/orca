//! Folder browsing uses indexed songs, never a recursive filesystem walk.
use crate::{types::Group, Backend};
use orca_core::db;
use std::collections::BTreeMap;

impl Backend {
    pub fn library_roots(&self) -> Result<Vec<String>, String> {
        serde_json::from_str(
            &db::get_setting(&self.conn, "library_scan_roots")
                .or_else(|| db::get_setting(&self.conn, "qt_scan_roots"))
                .unwrap_or_else(|| "[]".into()),
        )
        .map_err(|error| error.to_string())
    }

    pub fn folder_groups(&self, parent: &str, search: &str) -> Result<Vec<Group>, String> {
        let revision = db::folder_cache_revision(&self.conn)?;
        let cache_key = format!("folders:{parent}");
        if let Some(groups) = db::load_folder_cache(&self.conn, &cache_key, revision)?
            .and_then(|payload| serde_json::from_str(&payload).ok())
        {
            return Ok(filter_folders(groups, search));
        }
        if let Some(entry) = self.group_cache.borrow().get(&cache_key) {
            if entry.revision == (revision, 0) {
                return Ok(filter_folders(entry.groups.clone(), search));
            }
        }
        let mut folders = BTreeMap::<String, (u32, String)>::new();
        if parent.is_empty() {
            for root in self.library_roots()? {
                let normalized = root.replace('\\', "/");
                let key = normalized.trim_end_matches('/');
                folders.insert(
                    if key.is_empty() {
                        "/".into()
                    } else {
                        key.into()
                    },
                    (0, String::new()),
                );
            }
        }
        let parent = parent.replace('\\', "/");
        let filter = super::catalog::predicate("folders").replace("?2", "?1");
        if parent.is_empty() {
            // Root cards need only indexed counts and a representative cover.
            // Do not materialize every descendant path just to show one card.
            let mut counts = self
                .conn
                .prepare(&format!(
                    "SELECT COUNT(*) FROM active_songs s WHERE {filter}"
                ))
                .map_err(|error| error.to_string())?;
            for (folder, (count, _)) in &mut folders {
                *count = counts
                    .query_row([folder], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
            }
        } else {
            let (path_column, collation) = if cfg!(windows) {
                ("replace(s.path,char(92),'/')", "NOCASE")
            } else {
                ("s.path", "BINARY")
            };
            let prefix = format!("{}/", parent.trim_end_matches('/'));
            let mut statement = self
            .conn
            .prepare(&format!(
                "WITH descendants AS (SELECT substr({path_column},length(?2)+1) AS relative FROM active_songs s WHERE {filter})
                 SELECT substr(relative,1,instr(relative,'/')-1) AS child,COUNT(*) FROM descendants
                 WHERE instr(relative,'/')>0 GROUP BY child COLLATE {collation}"
            ))
            .map_err(|e| e.to_string())?;
            let children = statement
                .query_map(rusqlite::params![&parent, &prefix], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for child in children {
                let (child, count) = child.map_err(|e| e.to_string())?;
                folders.insert(format!("{prefix}{child}"), (count, String::new()));
            }
        }
        // Resolve one available cover per directory, rather than materializing
        // artwork metadata for every descendant song during folder counting.
        for (folder, (_, cover)) in &mut folders {
            *cover = self.folder_group(folder)?.artwork;
        }
        let overrides = self.collection_overrides()?;
        let groups: Vec<_> = folders
            .into_iter()
            .map(|(key, (count, artwork))| {
                let entry = overrides.get(&serde_json::json!(["folders", key, ""]).to_string());
                Group {
                    title: entry
                        .and_then(|e| e["name"].as_str())
                        .unwrap_or_else(|| {
                            key.rsplit('/')
                                .find(|part| !part.is_empty())
                                .unwrap_or(&key)
                        })
                        .into(),
                    subtitle: format!("{count} {}", if count == 1 { "song" } else { "songs" }),
                    key,
                    secondary: parent.clone(),
                    count,
                    artwork: entry
                        .and_then(|e| e["cover"].as_str())
                        .filter(|cover| !cover.is_empty())
                        .unwrap_or(&artwork)
                        .into(),
                    ..Default::default()
                }
            })
            .collect();
        let _ = db::save_folder_cache(
            &self.conn,
            &cache_key,
            revision,
            &serde_json::to_string(&groups).map_err(|e| e.to_string())?,
        );
        if db::folder_cache_revision(&self.conn)? == revision {
            let mut cache = self.group_cache.borrow_mut();
            if cache
                .keys()
                .filter(|key| key.starts_with("folders:"))
                .count()
                >= 16
            {
                cache.retain(|key, _| !key.starts_with("folders:"));
            }
            cache.insert(
                cache_key,
                super::catalog::GroupCacheEntry {
                    revision: (revision, 0),
                    groups: groups.clone(),
                },
            );
        }
        Ok(filter_folders(groups, search))
    }

    pub fn folder_exists(&self, key: &str) -> Result<bool, String> {
        let key = key.replace('\\', "/");
        let roots = self.library_roots()?;
        let root = roots.iter().map(|r| r.replace('\\', "/")).any(|r| {
            if cfg!(windows) {
                key.eq_ignore_ascii_case(r.trim_end_matches('/'))
            } else {
                key == r.trim_end_matches('/')
            }
        });
        if root {
            return Ok(true);
        }
        if !roots.iter().any(|r| in_folder(&key, &r.replace('\\', "/"))) {
            return Ok(false);
        }
        let filter = super::catalog::predicate("folders").replace("?2", "?1");
        self.conn
            .query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM active_songs s WHERE {filter})"),
                [&key],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
    pub(crate) fn folder_group(&self, key: &str) -> Result<Group, String> {
        use rusqlite::OptionalExtension;
        let revision = db::folder_cache_revision(&self.conn)?;
        let cache_key = format!("folder-header:{key}");
        if let Some(group) = db::load_folder_cache(&self.conn, &cache_key, revision)?
            .and_then(|payload| serde_json::from_str(&payload).ok())
        {
            return Ok(group);
        }
        if !self.folder_exists(key)? {
            return Err("Folder no longer exists in the library".into());
        }
        let key = key.replace('\\', "/");
        let filter = super::catalog::predicate("folders").replace("?2", "?1");
        let order = if cfg!(windows) {
            "replace(s.path,char(92),'/') COLLATE NOCASE,s.id"
        } else {
            "s.path COLLATE BINARY,s.id"
        };
        let artwork: Option<String> = self.conn.query_row(&format!("SELECT COALESCE(NULLIF(NULLIF(s.artwork_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album)) AS cover FROM active_songs s WHERE ({filter}) AND cover IS NOT NULL ORDER BY {order} LIMIT 1"), [&key], |row|row.get(0)).optional().map_err(|e|e.to_string())?;
        let overrides = self.collection_overrides()?;
        let entry = overrides.get(&serde_json::json!(["folders", key, ""]).to_string());
        let group = Group {
            title: key
                .rsplit('/')
                .find(|part| !part.is_empty())
                .unwrap_or(&key)
                .into(),
            artwork: entry
                .and_then(|e| e["cover"].as_str())
                .filter(|cover| !cover.is_empty())
                .or(artwork.as_deref())
                .unwrap_or_default()
                .into(),
            key,
            ..Default::default()
        };
        let _ = db::save_folder_cache(
            &self.conn,
            &cache_key,
            revision,
            &serde_json::to_string(&group).map_err(|e| e.to_string())?,
        );
        Ok(group)
    }

    pub(crate) fn folder_totals(&self, key: &str) -> Result<(i64, i64), String> {
        let revision = db::folder_cache_revision(&self.conn)?;
        let cache_key = format!("folder-totals:{key}");
        if let Some(totals) = db::load_folder_cache(&self.conn, &cache_key, revision)?
            .and_then(|payload| serde_json::from_str(&payload).ok())
        {
            return Ok(totals);
        }
        let filter = super::catalog::predicate("folders").replace("?2", "?1");
        let totals = self.conn.query_row(
            &format!("SELECT COUNT(*),COALESCE(SUM(s.duration),0) FROM active_songs s WHERE {filter}"),
            [key], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        ).map_err(|e| e.to_string())?;
        let _ = db::save_folder_cache(
            &self.conn,
            &cache_key,
            revision,
            &serde_json::to_string(&totals).map_err(|e| e.to_string())?,
        );
        Ok(totals)
    }
}

fn filter_folders(mut groups: Vec<Group>, search: &str) -> Vec<Group> {
    let search = search.to_lowercase();
    groups.retain(|group| {
        group.title.to_lowercase().contains(&search) || group.key.to_lowercase().contains(&search)
    });
    groups
}

fn in_folder(path: &str, folder: &str) -> bool {
    let prefix = format!("{}/", folder.trim_end_matches('/'));
    path.get(..prefix.len()).is_some_and(|start| {
        if cfg!(windows) {
            start.eq_ignore_ascii_case(&prefix)
        } else {
            start == prefix
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folder_paths_respect_directory_boundaries() {
        assert!(in_folder("C:/Music/Artist/song.flac", "C:/Music/"));
        assert!(!in_folder("C:/Music-copy/song.flac", "C:/Music"));
        assert!(!in_folder("C:/Music.flac", "C:/Music"));
    }
}
