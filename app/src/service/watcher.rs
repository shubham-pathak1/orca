use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Changes {
    roots: HashSet<PathBuf>,
    dirty: HashMap<PathBuf, (Instant, Instant)>,
}
impl Changes {
    fn receive(&mut self, event: notify::Result<Event>, now: Instant) {
        if matches!(&event,Ok(event) if matches!(event.kind,EventKind::Access(_))) {
            return;
        }
        for root in &self.roots {
            let relevant = event.as_ref().map_or(true, |event| {
                event.paths.iter().any(|path| inside(path, root))
            });
            if relevant {
                self.dirty
                    .entry(root.clone())
                    .and_modify(|v| v.1 = now)
                    .or_insert((now, now));
            }
        }
    }
    fn due(&mut self, now: Instant) -> Vec<String> {
        let ready: Vec<_> = self
            .dirty
            .iter()
            .filter(|(_, (first, last))| {
                now.saturating_duration_since(*last) >= Duration::from_millis(750)
                    || now.saturating_duration_since(*first) >= Duration::from_secs(3)
            })
            .map(|(root, _)| root.clone())
            .collect();
        for root in &ready {
            self.dirty.remove(root);
        }
        ready
            .into_iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect()
    }
}
fn inside(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let key = |path: &Path| {
            path.to_string_lossy()
                .replace(char::from(92), "/")
                .trim_start_matches("//?/")
                .to_lowercase()
        };
        let path = key(path);
        let root = key(root);
        path == root || path.starts_with(&format!("{}/", root.trim_end_matches('/')))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}
pub struct LibraryWatcher {
    watcher: RecommendedWatcher,
    changes: Arc<Mutex<Changes>>,
}
impl LibraryWatcher {
    pub fn new() -> Result<Self, String> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let observed = changes.clone();
        let watcher = notify::recommended_watcher(move |event| {
            if let Ok(mut changes) = observed.lock() {
                changes.receive(event, Instant::now());
            }
        })
        .map_err(|e| e.to_string())?;
        Ok(Self { watcher, changes })
    }
    pub fn sync(&mut self, roots: &[String]) -> Vec<String> {
        let wanted: HashSet<_> = roots
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .collect();
        let existing = self.changes.lock().unwrap().roots.clone();
        let mut errors = Vec::new();
        for root in existing.difference(&wanted) {
            let _ = self.watcher.unwatch(root);
            let mut changes = self.changes.lock().unwrap();
            changes.roots.remove(root);
            changes.dirty.remove(root);
        }
        for root in wanted.difference(&existing) {
            match self.watcher.watch(root, RecursiveMode::Recursive) {
                Ok(()) => {
                    self.changes.lock().unwrap().roots.insert(root.clone());
                }
                Err(error) => errors.push(format!("Folder watching unavailable: {error}")),
            }
        }
        errors
    }
    pub fn due(&mut self) -> Vec<String> {
        self.changes.lock().unwrap().due(Instant::now())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changes_are_batched_bounded_and_ignore_reads() {
        let root = PathBuf::from("music");
        let now = Instant::now();
        let mut changes = Changes::default();
        changes.roots.insert(root.clone());
        for _ in 0..1000 {
            changes.receive(
                Ok(Event::new(EventKind::Any).add_path(root.join("song.flac"))),
                now,
            );
        }
        assert_eq!(changes.dirty.len(), 1);
        assert!(changes.due(now + Duration::from_millis(500)).is_empty());
        assert_eq!(changes.due(now + Duration::from_millis(800)).len(), 1);
        assert!(changes.dirty.is_empty());
        changes.receive(
            Ok(
                Event::new(EventKind::Access(notify::event::AccessKind::Any))
                    .add_path(root.join("song.flac")),
            ),
            now,
        );
        assert!(changes.dirty.is_empty());
        changes.receive(
            Ok(Event::new(EventKind::Any).add_path(PathBuf::from("unrelated/song.flac"))),
            now,
        );
        assert!(changes.dirty.is_empty());
    }
    #[test]
    fn continuous_changes_flush_without_waiting_forever() {
        let root = PathBuf::from("music");
        let now = Instant::now();
        let mut changes = Changes::default();
        changes.roots.insert(root.clone());
        changes.receive(Ok(Event::new(EventKind::Any).add_path(root.join("a"))), now);
        changes.receive(
            Ok(Event::new(EventKind::Any).add_path(root.join("b"))),
            now + Duration::from_secs(3),
        );
        assert_eq!(changes.due(now + Duration::from_secs(3)).len(), 1);
    }
    #[test]
    fn native_notifications_detect_add_delete_restore_and_release_folder() {
        let root = std::env::temp_dir().join(format!("orca-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let song = root.join("song.flac");
        let mut watcher = LibraryWatcher::new().unwrap();
        assert!(watcher
            .sync(&[root.to_string_lossy().into_owned()])
            .is_empty());
        fn wait(watcher: &mut LibraryWatcher) {
            let deadline = Instant::now() + Duration::from_secs(6);
            loop {
                if !watcher.due().is_empty() {
                    return;
                }
                assert!(Instant::now() < deadline, "no native file notification");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        std::fs::write(&song, b"fixture").unwrap();
        wait(&mut watcher);
        std::fs::remove_file(&song).unwrap();
        wait(&mut watcher);
        std::fs::write(&song, b"restored").unwrap();
        wait(&mut watcher);
        assert!(watcher.sync(&[]).is_empty());
        std::fs::remove_file(&song).unwrap();
        assert!(watcher.changes.lock().unwrap().roots.is_empty());
        drop(watcher);
        std::fs::remove_dir(&root).unwrap();
    }
}
