//! Stage writes beside their destination, flush them, then replace in one rename.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileVersion {
    pub size: u64,
    pub modified: SystemTime,
}
impl FileVersion {
    pub fn token(&self) -> String {
        let modified = self
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        format!(
            "{}:{}:{}",
            self.size,
            modified.as_secs(),
            modified.subsec_nanos()
        )
    }
    pub fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected a regular file",
            ));
        }
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn temporary(destination: &Path) -> io::Result<(Temporary, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    for _ in 0..100 {
        let path = parent.join(format!(
            ".orca-write-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((Temporary(path), file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Could not reserve a temporary file",
    ))
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source = wide(source);
    let destination = wide(destination);
    // Both paths are on the same filesystem. Do not allow copy-and-delete fallback.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn replace_audio(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
    let (backup, file) = temporary(destination)?;
    drop(file);
    fs::remove_file(&backup.0)?;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source_wide = wide(source);
    let destination_wide = wide(destination);
    let backup_wide = wide(&backup.0);
    // Preserve the destination's Windows ACLs and attributes; retain a recovery
    // copy if Windows reports a partial replacement failure.
    if unsafe {
        ReplaceFileW(
            destination_wide.as_ptr(),
            source_wide.as_ptr(),
            backup_wide.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        if backup.0.exists() {
            if !destination.exists() && replace(&backup.0, destination).is_ok() {
                return Err(error);
            }
            let recovery = backup.0.clone();
            std::mem::forget(backup);
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "{error}; original file recovery copy: {}",
                    recovery.display()
                ),
            ));
        }
        return Err(error);
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace_audio(source: &Path, destination: &Path) -> io::Result<()> {
    replace(source, destination)
}
#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

pub fn write(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    if fs::metadata(destination).is_ok_and(|metadata| metadata.len() == bytes.len() as u64)
        && fs::read(destination).is_ok_and(|existing| existing == bytes)
    {
        return Ok(());
    }
    if fs::metadata(destination).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Destination is read-only",
        ));
    }
    let (temporary, mut file) = temporary(destination)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    replace(&temporary.0, destination)
}

/// The editor can fail freely on the staged copy. Never remove the original to
/// make replacement succeed, and reject changes made during preparation.
pub fn edit<T>(
    destination: &Path,
    edit: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    edit_checked(destination, None, edit)
}
/// Reject a stale editor version before staging, then check for changes before replacement.
pub fn edit_checked<T>(
    destination: &Path,
    expected_version: Option<&str>,
    edit: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let destination = fs::canonicalize(destination).map_err(|e| e.to_string())?;
    let permissions = fs::metadata(&destination)
        .map_err(|e| e.to_string())?
        .permissions();
    if permissions.readonly() {
        return Err("Permission denied: this audio file is read-only".into());
    }
    let version = FileVersion::read(&destination).map_err(|e| e.to_string())?;
    if expected_version.is_some_and(|expected| expected != version.token()) {
        return Err(
            "This audio file changed since the editor opened. Reopen the editor and try again."
                .into(),
        );
    }
    let (temporary, mut staged) = temporary(&destination).map_err(|e| e.to_string())?;
    let mut original = File::open(&destination).map_err(|e| e.to_string())?;
    io::copy(&mut original, &mut staged).map_err(|e| e.to_string())?;
    drop(original);
    staged.sync_all().map_err(|e| e.to_string())?;
    drop(staged);
    let result = edit(&temporary.0)?;
    OpenOptions::new()
        .write(true)
        .open(&temporary.0)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::set_permissions(&temporary.0, permissions).map_err(|e| e.to_string())?;
    if FileVersion::read(&destination).map_err(|e| e.to_string())? != version {
        return Err(
            "This audio file changed during editing. Reopen the editor and try again.".into(),
        );
    }
    replace_audio(&temporary.0, &destination).map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "orca-atomic-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }
    #[test]
    fn failed_edit_preserves_original_and_cleans_staged_file() {
        let dir = directory("failure");
        let path = dir.join("song.wav");
        fs::write(&path, b"original audio").unwrap();
        let result = edit(&path, |staged| {
            fs::write(staged, b"partially written").unwrap();
            Err::<(), _>("write failed".into())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original audio");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn external_changes_are_preserved_instead_of_overwritten() {
        let dir = directory("conflict");
        let path = dir.join("song.wav");
        fs::write(&path, b"original").unwrap();
        assert!(edit(&path, |staged| {
            fs::write(staged, b"our edit").unwrap();
            fs::write(&path, b"external changed file").unwrap();
            Ok(())
        })
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external changed file");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn replacement_and_unchanged_writes_keep_the_destination_valid() {
        let dir = directory("replace");
        let path = dir.join("settings.json");
        write(&path, b"first").unwrap();
        write(&path, b"second").unwrap();
        let version = FileVersion::read(&path).unwrap();
        write(&path, b"second").unwrap();
        assert_eq!(FileVersion::read(&path).unwrap(), version);
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn stale_editor_version_rejects_writing_before_preparation() {
        let dir = directory("stale");
        let path = dir.join("song.wav");
        fs::write(&path, b"before").unwrap();
        let version = FileVersion::read(&path).unwrap().token();
        fs::write(&path, b"changed externally").unwrap();
        let result: Result<(), String> = edit_checked(&path, Some(&version), |_| {
            panic!("stale edits must not run")
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"changed externally");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn read_only_audio_is_never_replaced() {
        let dir = directory("readonly");
        let path = dir.join("song.wav");
        fs::write(&path, b"original").unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = edit(&path, |_| Ok(()));
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        fs::set_permissions(&path, original_permissions).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn locked_audio_keeps_original_when_windows_rejects_replacement() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = directory("locked");
        let path = dir.join("song.wav");
        fs::write(&path, b"original").unwrap();
        // Permit reading/writing but deny deletion/replacement, like an external player.
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        assert!(edit(&path, |staged| fs::write(staged, b"new tags")
            .map_err(|e| e.to_string()))
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        drop(locked);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
