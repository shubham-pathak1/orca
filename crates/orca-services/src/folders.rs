//! Folder browsing uses indexed songs, never a recursive filesystem walk.
use crate::{types::Group, Backend};
use orca_core::db;
use std::collections::BTreeMap;

impl Backend {
    pub(crate) fn library_roots(&self) -> Result<Vec<String>, String> {
        serde_json::from_str(
            &db::get_setting(&self.conn, "library_scan_roots")
                .or_else(|| db::get_setting(&self.conn, "qt_scan_roots"))
                .unwrap_or_else(|| "[]".into()),
        )
        .map_err(|error| error.to_string())
    }

    pub fn folder_groups(&self, parent: &str, search: &str) -> Result<Vec<Group>, String> {
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
        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT s.path,COALESCE(NULLIF(NULLIF(s.artwork_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album),'') FROM songs s WHERE ?1='' OR ({filter}) ORDER BY s.path COLLATE NOCASE,s.id"
            ))
            .map_err(|e| e.to_string())?;
        let paths = statement
            .query_map([&parent], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let prefix = format!("{}/", parent.trim_end_matches('/'));
        for path in paths {
            let (path, artwork) = path.map_err(|e| e.to_string())?;
            let path = path.replace('\\', "/");
            if parent.is_empty() {
                for (folder, (count, cover)) in &mut folders {
                    if in_folder(&path, folder) {
                        *count += 1;
                        if cover.is_empty() {
                            *cover = artwork.clone();
                        }
                    }
                }
            } else if in_folder(&path, &parent) {
                let relative = &path[prefix.len()..];
                if let Some((child, _)) = relative.split_once('/') {
                    let (count, cover) = folders.entry(format!("{prefix}{child}")).or_default();
                    *count += 1;
                    if cover.is_empty() {
                        *cover = artwork;
                    }
                }
            }
        }
        let search = search.to_lowercase();
        let overrides = self.collection_overrides()?;
        Ok(folders
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
            .filter(|group| {
                group.title.to_lowercase().contains(&search)
                    || group.key.to_lowercase().contains(&search)
            })
            .collect())
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
                &format!("SELECT EXISTS(SELECT 1 FROM songs s WHERE {filter})"),
                [&key],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
    pub(crate) fn folder_group(&self, key: &str) -> Result<Group, String> {
        use rusqlite::OptionalExtension;
        if !self.folder_exists(key)? {
            return Err("Folder no longer exists in the library".into());
        }
        let key = key.replace('\\', "/");
        let filter = super::catalog::predicate("folders").replace("?2", "?1");
        let artwork: Option<String> = self.conn.query_row(&format!("SELECT COALESCE(NULLIF(NULLIF(s.artwork_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_preview_url,'DELETED'),''),NULLIF(NULLIF(s.artwork_thumb_url,'DELETED'),''),(SELECT NULLIF(artwork_path,'DELETED') FROM album_artworks WHERE album_key=s.album_artist || ':' || s.album)) AS cover FROM songs s WHERE ({filter}) AND cover IS NOT NULL ORDER BY s.path COLLATE NOCASE,s.id LIMIT 1"), [&key], |row|row.get(0)).optional().map_err(|e|e.to_string())?;
        let overrides = self.collection_overrides()?;
        let entry = overrides.get(&serde_json::json!(["folders", key, ""]).to_string());
        Ok(Group {
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
        })
    }
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
