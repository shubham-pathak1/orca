//! Persisted, hierarchical inclusion rules for library roots.
use crate::Backend;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderNode {
    pub path: String,
    pub name: String,
    pub explicit: String,
    pub included: bool,
    pub has_children: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FolderPolicy {
    rules: Vec<(String, bool)>,
}

fn key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    let value = value.trim_end_matches('/');
    let value = if value.is_empty() { "/" } else { value };
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into()
    }
}

fn inside(path: &str, parent: &str) -> bool {
    path == parent
        || (parent == "/" && path.starts_with('/'))
        || path.starts_with(&format!("{parent}/"))
}

impl FolderPolicy {
    pub(crate) fn load(conn: &Connection) -> Result<Self, String> {
        let mut stmt = conn
            .prepare("SELECT path,included FROM folder_rules")
            .map_err(|e| e.to_string())?;
        let rules = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(Self { rules })
    }

    pub(crate) fn included(&self, path: &Path) -> bool {
        let path = key(path);
        self.rules
            .iter()
            .filter(|(rule, _)| inside(&path, rule))
            .max_by_key(|(rule, _)| rule.len())
            .is_none_or(|(_, included)| *included)
    }

    fn explicit(&self, path: &Path) -> Option<bool> {
        let path = key(path);
        self.rules
            .iter()
            .find(|(rule, _)| rule == &path)
            .map(|(_, included)| *included)
    }

    pub(crate) fn has_included_descendant(&self, path: &Path) -> bool {
        let path = key(path);
        self.rules
            .iter()
            .any(|(rule, included)| *included && inside(rule, &path))
    }
}

impl Backend {
    /// Enumerate one directory level. This does not parse audio or descend into large trees.
    pub fn folder_children(
        &self,
        parent: &str,
        prospective_root: Option<&str>,
    ) -> Result<Vec<FolderNode>, String> {
        let parent = PathBuf::from(parent)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !parent.is_dir() {
            return Err("Choose a music folder".into());
        }
        let policy = FolderPolicy::load(&self.conn)?;
        let mut roots = self.library_roots()?;
        if let Some(root) = prospective_root {
            roots.push(
                PathBuf::from(root)
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let parent_key = key(&parent);
        if !roots
            .iter()
            .any(|root| inside(&parent_key, &key(Path::new(root))))
        {
            return Err("Folder is not in the library".into());
        }
        let mut nodes = Vec::new();
        for entry in std::fs::read_dir(&parent).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let path = entry.path();
            let has_children = std::fs::read_dir(&path).ok().is_some_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|child| child.file_type().is_ok_and(|kind| kind.is_dir()))
            });
            let explicit = match policy.explicit(&path) {
                Some(true) => "include",
                Some(false) => "exclude",
                None => "inherit",
            };
            nodes.push(FolderNode {
                path: path.to_string_lossy().into_owned(),
                name: entry.file_name().to_string_lossy().into_owned(),
                explicit: explicit.into(),
                included: policy.included(&path),
                has_children,
            });
        }
        nodes.sort_by_key(|node| node.name.to_lowercase());
        Ok(nodes)
    }

    /// Persist the root and all reviewed rules before the worker starts its first scan.
    pub fn add_folder_with_rules(
        &mut self,
        root: &str,
        rules: &[(String, String)],
    ) -> Result<String, String> {
        let root = PathBuf::from(root)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("Choose a music folder".into());
        }
        let root_key = key(&root);
        let mut normalized = Vec::with_capacity(rules.len());
        for (path, mode) in rules {
            if !matches!(mode.as_str(), "include" | "exclude") {
                return Err("Invalid folder rule".into());
            }
            let path = PathBuf::from(path)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !path.is_dir() || !inside(&key(&path), &root_key) || key(&path) == root_key {
                return Err("Choose a subfolder of the music root".into());
            }
            normalized.push((key(&path), mode == "include"));
        }
        let mut roots = self.library_roots()?;
        let root_string = root.to_string_lossy().into_owned();
        if !roots
            .iter()
            .any(|existing| key(Path::new(existing)) == root_key)
        {
            roots.push(root_string.clone());
        }
        self.cancel_and_join_scan();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        for (path, included) in normalized {
            tx.execute("INSERT INTO folder_rules(path,included) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET included=excluded.included", params![path, included]).map_err(|e| e.to_string())?;
        }
        let policy = FolderPolicy::load(&tx)?;
        refresh_visibility(&tx, &policy)?;
        orca_core::db::set_setting(
            &tx,
            "library_scan_roots",
            &serde_json::to_string(&roots).map_err(|e| e.to_string())?,
        )?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(root_string)
    }

    pub fn set_folder_rule(&mut self, path: &str, mode: &str) -> Result<(), String> {
        if !matches!(mode, "include" | "exclude" | "inherit") {
            return Err("Invalid folder rule".into());
        }
        let path = PathBuf::from(path)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.is_dir() {
            return Err("Choose a music folder".into());
        }
        let path_key = key(&path);
        let roots = self.library_roots()?;
        if !roots.iter().any(|root| {
            let root_key = key(Path::new(root));
            path_key != root_key && inside(&path_key, &root_key)
        }) {
            return Err("Choose a subfolder of a music root".into());
        }
        self.cancel_and_join_scan();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        if mode == "inherit" {
            tx.execute("DELETE FROM folder_rules WHERE path=?1", [&path_key])
                .map_err(|e| e.to_string())?;
        } else {
            tx.execute("INSERT INTO folder_rules(path,included) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET included=excluded.included", params![path_key, mode == "include"])
                .map_err(|e| e.to_string())?;
        }
        let policy = FolderPolicy::load(&tx)?;
        refresh_visibility(&tx, &policy)?;
        tx.commit().map_err(|e| e.to_string())
    }
}

fn refresh_visibility(conn: &Connection, policy: &FolderPolicy) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id,path,library_visible FROM songs WHERE scan_missing=0")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (id, song_path, visible) in rows {
        let included = policy.included(Path::new(&song_path));
        if included == visible {
            continue;
        }
        conn.execute(
            "UPDATE songs SET library_visible=?1 WHERE id=?2",
            params![included, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::inside;

    #[test]
    fn root_and_sibling_paths_respect_directory_boundaries() {
        assert!(inside("/music/album", "/"));
        assert!(inside("/music/album", "/music"));
        assert!(!inside("/music-extra", "/music"));
        assert!(!inside("/elsewhere", "/music"));
    }
}
