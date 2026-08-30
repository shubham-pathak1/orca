use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use notify::{event::ModifyKind, Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::AppHandle;

use crate::{
    commands::library::{refresh_watched_library, rescan_watched_library},
    state::{LibraryWatchMessage, OrcaState},
};

const DEBOUNCE_DELAY: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_secs(3);
const MAX_FILE_RETRIES: u8 = 5;
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
                    EventKind::Any | EventKind::Other | EventKind::Modify(ModifyKind::Name(_)) => {
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
        let mut retry_attempts = HashMap::<PathBuf, u8>::new();
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
                    retry_attempts.clear();
                    needs_full_rescan = false;
                }
                Ok(LibraryWatchMessage::FilesystemChanged(paths)) => {
                    for path in paths {
                        retry_attempts.remove(&path);
                        changed_paths.insert(path);
                    }
                    refresh_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                }
                Ok(LibraryWatchMessage::FullRescan) => {
                    needs_full_rescan = true;
                    refresh_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if refresh_deadline.is_some() {
                        let now = Instant::now();
                        if needs_full_rescan {
                            if let Err(error) = rescan_watched_library(&app, &state) {
                                eprintln!("Automatic library refresh failed: {error}");
                            }
                            retry_attempts.clear();
                            refresh_deadline = None;
                            needs_full_rescan = false;
                        } else {
                            match refresh_watched_library(
                                &app,
                                &state,
                                changed_paths.drain().collect(),
                            ) {
                                Ok(retry_paths) => {
                                    for path in retry_paths {
                                        let should_retry = {
                                            let attempts =
                                                retry_attempts.entry(path.clone()).or_default();
                                            *attempts += 1;
                                            *attempts <= MAX_FILE_RETRIES
                                        };
                                        if should_retry {
                                            changed_paths.insert(path);
                                        } else {
                                            retry_attempts.remove(&path);
                                            eprintln!(
                                                "Stopped retrying unreadable library file after {MAX_FILE_RETRIES} attempts: {}",
                                                path.display()
                                            );
                                        }
                                    }
                                    refresh_deadline =
                                        (!changed_paths.is_empty()).then_some(now + RETRY_DELAY);
                                }
                                Err(error) => {
                                    eprintln!("Automatic library refresh failed: {error}");
                                    needs_full_rescan = true;
                                    refresh_deadline = Some(now + RETRY_DELAY);
                                }
                            }
                        }
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
