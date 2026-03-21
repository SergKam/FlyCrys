use gtk::glib;
use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::config::constants::FILE_WATCHER_SYNC_MS;
use crate::tree::{self, DirStoreMap};

/// Watches only the directories currently visible in the file tree (expanded
/// dirs tracked by `dir_stores`) plus the parent of the open file.
///
/// On each 200ms tick the watcher syncs its watch set with `dir_stores` keys,
/// adding/removing individual non-recursive watches as directories are
/// expanded or collapsed. This typically means 5–10 inotify watches instead of
/// tens of thousands for a project with node_modules/.
pub struct FileWatcher {
    _watcher: Rc<RefCell<Option<RecommendedWatcher>>>,
}

impl FileWatcher {
    pub fn new(
        root: &Path,
        dir_stores: DirStoreMap,
        current_file: Rc<RefCell<String>>,
        on_file_changed: Rc<dyn Fn()>,
        on_file_removed: Rc<dyn Fn()>,
    ) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Event>();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        )
        .ok()?;

        // Watch the root directory (always visible as the tree root)
        let _ = watcher.watch(root, RecursiveMode::NonRecursive);

        let watcher = Rc::new(RefCell::new(Some(watcher)));
        let watched_dirs: Rc<RefCell<HashSet<PathBuf>>> = Rc::new(RefCell::new(HashSet::new()));
        watched_dirs.borrow_mut().insert(root.to_path_buf());

        // Single timer: sync watched dirs with dir_stores, then drain events.
        let watcher_for_timer = Rc::clone(&watcher);
        glib::timeout_add_local(Duration::from_millis(FILE_WATCHER_SYNC_MS), move || {
            // --- Sync watches with visible directories ---
            if let Some(ref mut w) = *watcher_for_timer.borrow_mut() {
                let mut desired: HashSet<PathBuf> = dir_stores.borrow().keys().cloned().collect();

                // Also watch the parent dir of the currently open file
                let current = current_file.borrow().clone();
                if !current.is_empty()
                    && let Some(parent) = Path::new(&current).parent()
                {
                    desired.insert(parent.to_path_buf());
                }

                let mut watched = watched_dirs.borrow_mut();

                // Add watches for newly expanded dirs
                for dir in desired.difference(&watched).cloned().collect::<Vec<_>>() {
                    let _ = w.watch(&dir, RecursiveMode::NonRecursive);
                    watched.insert(dir);
                }

                // Remove watches for collapsed/gone dirs
                for dir in watched.difference(&desired).cloned().collect::<Vec<_>>() {
                    let _ = w.unwatch(&dir);
                    watched.remove(&dir);
                }
            }

            // --- Drain and process events ---
            let mut changed_dirs: HashSet<PathBuf> = HashSet::new();
            let mut current_modified = false;
            let mut current_removed = false;

            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        for path in &event.paths {
                            if let Some(parent) = path.parent() {
                                changed_dirs.insert(parent.to_path_buf());
                            }
                            if path.is_dir() {
                                changed_dirs.insert(path.clone());
                            }

                            let current = current_file.borrow().clone();
                            if !current.is_empty() && path.to_string_lossy() == current.as_str() {
                                if path.exists() {
                                    current_modified = true;
                                } else {
                                    current_removed = true;
                                }
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return glib::ControlFlow::Break;
                    }
                }
            }

            for dir in &changed_dirs {
                let store = dir_stores.borrow().get(dir).cloned();
                if let Some(store) = store {
                    tree::refresh_directory(&store, dir, &dir_stores);
                }
            }

            if current_removed {
                on_file_removed();
            } else if current_modified {
                on_file_changed();
            }

            glib::ControlFlow::Continue
        });

        Some(FileWatcher { _watcher: watcher })
    }
}
