use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::AppHandle;

use crate::{
    commands::library::{refresh_watched_library, rescan_watched_library},
    state::{LibraryWatchMessage, OrcaState},
};

const DEBOUNCE_DELAY: Duration = Duration::from_secs(2);
const IDLE_WAIT: Duration = Duration::from_secs(60);

pub(crate) fn start_library_watcher(
    app: AppHandle,
    state: Arc<Mutex<OrcaState>>,
    receiver: mpsc::Receiver<LibraryWatchMessage>,
    sender: mpsc::Sender<LibraryWatchMessage>,
) {
    thread::spawn(move || {
        let callback_sender = sender.clone();
        let mut watcher = match RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| match event {
                Ok(event) => match event.kind {
                    EventKind::Access(_) => {}
                    EventKind::Any | EventKind::Other => {
                        let _ = callback_sender.send(LibraryWatchMessage::FullRescan);
                    }
                    _ => {
                        let _ = callback_sender
                            .send(LibraryWatchMessage::FilesystemChanged(event.paths));
                    }
                },
                Err(error) => {
                    eprintln!("Library watcher event failed: {error}");
                    let _ = callback_sender.send(LibraryWatchMessage::FullRescan);
                }
            },
            Config::default(),
        ) {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!("Unable to start the library watcher: {error}");
                return;
            }
        };

        let mut watched_roots = Vec::new();
        let mut refresh_deadline: Option<Instant> = None;
        let mut changed_paths = HashSet::new();
        let mut needs_full_rescan = false;

        loop {
            let wait = refresh_deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(IDLE_WAIT);

            match receiver.recv_timeout(wait) {
                Ok(LibraryWatchMessage::UpdateRoots(roots)) => {
                    replace_watches(&mut watcher, &mut watched_roots, roots);
                    refresh_deadline = None;
                    changed_paths.clear();
                    needs_full_rescan = false;
                }
                Ok(LibraryWatchMessage::FilesystemChanged(paths)) => {
                    changed_paths.extend(paths);
                    refresh_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                }
                Ok(LibraryWatchMessage::FullRescan) => {
                    needs_full_rescan = true;
                    refresh_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if refresh_deadline.is_some() {
                        let result = if needs_full_rescan {
                            rescan_watched_library(&app, &state)
                        } else {
                            refresh_watched_library(&app, &state, changed_paths.drain().collect())
                        };
                        if let Err(error) = result {
                            eprintln!("Automatic library refresh failed: {error}");
                        }
                        refresh_deadline = None;
                        needs_full_rescan = false;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

fn replace_watches(
    watcher: &mut RecommendedWatcher,
    watched_roots: &mut Vec<PathBuf>,
    roots: Vec<PathBuf>,
) {
    for root in watched_roots.drain(..) {
        let _ = watcher.unwatch(&root);
    }

    for root in roots {
        match watcher.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => watched_roots.push(root),
            Err(error) => eprintln!("Unable to watch library folder {}: {error}", root.display()),
        }
    }
}
