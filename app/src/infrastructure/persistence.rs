//! Session/preferences file persistence. Schema defaults preserve older profiles.
use crate::navigation::Navigation;
use serde::{Deserialize, Serialize};
use std::fs;
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Session {
    pub(crate) path: String,
    pub(crate) position: u64,
    pub(crate) volume: f32,
    pub(crate) navigation: Navigation,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            path: String::new(),
            position: 0,
            volume: 1.0,
            navigation: Navigation::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SessionDocument {
    #[serde(flatten)]
    session: Session,
    #[serde(default)]
    checkpoint_generation: String,
}

#[derive(Serialize, Deserialize)]
struct PositionCheckpoint {
    checkpoint_generation: String,
    path: String,
    position: u64,
    volume: f32,
}

fn position_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".position");
    name.into()
}

/// A small position checkpoint belongs to exactly one saved queue context.
pub(crate) fn read_session(path: &std::path::Path) -> Option<Session> {
    for candidate in [path.to_path_buf(), backup_path(path)] {
        let Some(mut document) = read_bytes(&candidate)
            .and_then(|bytes| serde_json::from_slice::<SessionDocument>(&bytes).ok())
        else {
            continue;
        };
        if let Some(checkpoint) = read_json::<PositionCheckpoint>(&position_path(path)) {
            if !document.checkpoint_generation.is_empty()
                && checkpoint.checkpoint_generation == document.checkpoint_generation
                && checkpoint.path == document.session.path
                && checkpoint.volume.is_finite()
                && (0.0..=1.0).contains(&checkpoint.volume)
            {
                document.session.position = checkpoint.position;
                document.session.volume = checkpoint.volume;
            }
        }
        return Some(document.session);
    }
    None
}

#[derive(Default)]
pub(crate) struct SessionWriter {
    saved: Option<SessionDocument>,
}
impl SessionWriter {
    pub(crate) fn save(
        &mut self,
        file: &std::path::Path,
        path: &str,
        position: u64,
        volume: f32,
        navigation: &Navigation,
    ) -> Result<(), String> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("Invalid session volume".into());
        }
        if self.saved.as_ref().is_none_or(|document| {
            document.session.path != path || &document.session.navigation != navigation
        }) {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let document = SessionDocument {
                session: Session {
                    path: path.into(),
                    position,
                    volume,
                    navigation: navigation.clone(),
                },
                checkpoint_generation: format!("{}-{timestamp}-{sequence}", std::process::id()),
            };
            write_json(file, &document)?;
            self.saved = Some(document);
        }
        let document = self
            .saved
            .as_ref()
            .expect("saved context exists after successful write");
        write_json(
            &position_path(file),
            &PositionCheckpoint {
                checkpoint_generation: document.checkpoint_generation.clone(),
                path: path.into(),
                position,
                volume,
            },
        )
    }
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    [path.to_path_buf(), backup_path(path)]
        .into_iter()
        .find_map(|candidate| {
            read_bytes(&candidate).and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
}
fn read_bytes(path: &std::path::Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 16 * 1024 * 1024).then_some(bytes)
}
fn backup_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    name.into()
}
pub fn write_json<T: Serialize + serde::de::DeserializeOwned>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Saved session exceeds 16 MB".into());
    }
    if let Some(previous) = read_bytes(path) {
        if previous == bytes {
            return Ok(());
        }
        // Never replace the known-good backup with a truncated/corrupt document.
        if serde_json::from_slice::<T>(&previous).is_ok() {
            orca_services::write_state_file(&backup_path(path), &previous)
                .map_err(|e| e.to_string())?;
        }
    }
    orca_services::write_state_file(path, &bytes).map_err(|e| e.to_string())?;
    if !backup_path(path).exists() {
        orca_services::write_state_file(&backup_path(path), &bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_updates_recover_without_rewriting_large_queue_context() {
        let directory =
            std::env::temp_dir().join(format!("orca-position-checkpoint-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        let mut navigation = Navigation::default();
        navigation.order = (0..20000).map(|i| format!("track-{i}.flac")).collect();
        let mut writer = SessionWriter::default();
        writer
            .save(&path, "track-0.flac", 1000, 0.4, &navigation)
            .unwrap();
        let base = fs::read(&path).unwrap();
        let version = fs::metadata(&path).unwrap().modified().unwrap();
        writer
            .save(&path, "track-0.flac", 3000, 0.7, &navigation)
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), base);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), version);
        assert!(fs::metadata(position_path(&path)).unwrap().len() < 1024);
        let restored = read_session(&path).unwrap();
        assert_eq!(restored.position, 3000);
        assert_eq!(restored.volume, 0.7);
        assert_eq!(restored.navigation.order, navigation.order);
        fs::write(position_path(&path), b"{broken").unwrap();
        assert_eq!(
            read_session(&path).unwrap().position,
            1000,
            "checkpoint backup retains the previous valid position"
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn changed_context_rejects_old_checkpoint_and_legacy_sessions_still_load() {
        let directory =
            std::env::temp_dir().join(format!("orca-context-checkpoint-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        let mut navigation = Navigation::default();
        navigation.context(vec!["same.flac".into(), "old.flac".into()], "same.flac");
        let mut writer = SessionWriter::default();
        writer
            .save(&path, "same.flac", 1000, 0.4, &navigation)
            .unwrap();
        let old = fs::read(position_path(&path)).unwrap();
        navigation.context(vec!["same.flac".into(), "new.flac".into()], "same.flac");
        writer
            .save(&path, "same.flac", 5000, 0.6, &navigation)
            .unwrap();
        fs::write(position_path(&path), old).unwrap();
        let restored = read_session(&path).unwrap();
        assert_eq!(restored.position, 5000);
        assert_eq!(restored.navigation.order, navigation.order);
        write_json(
            &path,
            &Session {
                path: "legacy.flac".into(),
                position: 8000,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(read_session(&path).unwrap().position, 8000);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn typed_recovery_rejects_wrong_schema_and_unchanged_saves_do_not_rewrite() {
        let directory =
            std::env::temp_dir().join(format!("orca-typed-state-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        let session = Session {
            volume: 0.25,
            ..Default::default()
        };
        assert!(read_json::<Session>(&path).is_none());
        write_json(&path, &session).unwrap();
        let version = fs::metadata(&path).unwrap().modified().unwrap();
        let backup = fs::read(backup_path(&path)).unwrap();
        write_json(&path, &session).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), version);
        assert_eq!(fs::read(backup_path(&path)).unwrap(), backup);
        fs::write(&path, br#"{"volume":"invalid"}"#).unwrap();
        assert_eq!(read_json::<Session>(&path).unwrap().volume, 0.25);
        write_json(
            &path,
            &Session {
                volume: 0.75,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(fs::read(backup_path(&path)).unwrap(), backup);
        assert_eq!(read_json::<Session>(&path).unwrap().volume, 0.75);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn refused_state_write_keeps_the_valid_document() {
        let directory =
            std::env::temp_dir().join(format!("orca-readonly-state-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        write_json(
            &path,
            &Session {
                volume: 0.25,
                ..Default::default()
            },
        )
        .unwrap();
        let original = fs::read(&path).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        assert!(write_json(
            &path,
            &Session {
                volume: 0.75,
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(read_json::<Session>(&path).unwrap().volume, 0.25);
        fs::set_permissions(&path, permissions).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn corrupt_or_missing_state_recovers_the_previous_valid_document() {
        let directory =
            std::env::temp_dir().join(format!("orca-state-recovery-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.json");
        write_json(&path, &serde_json::json!({"volume":0.3})).unwrap();
        write_json(&path, &serde_json::json!({"volume":0.8})).unwrap();
        fs::write(&path, b"{broken").unwrap();
        assert_eq!(
            read_json::<serde_json::Value>(&path).unwrap()["volume"],
            0.3
        );
        write_json(&path, &serde_json::json!({"volume":0.5})).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(
            read_json::<serde_json::Value>(&path).unwrap()["volume"],
            0.3
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
