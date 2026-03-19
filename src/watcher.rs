use gtk::glib;
use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::tree::{self, DirStoreMap};

/// Watches a directory tree for changes and refreshes the file-tree stores
/// and the currently open file view.
pub struct FileWatcher {
    // Dropping this stops the OS-level watcher and closes the sender,
    // which makes the GTK timer exit on the next tick.
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Start watching `root` recursively.
    ///
    /// * `dir_stores` – shared map of expanded-directory → ListStore
    /// * `current_file` – path of the file currently shown in the text view
    /// * `on_file_changed` – called when the current file is modified on disk
    /// * `on_file_removed` – called when the current file is deleted
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

        watcher.watch(root, RecursiveMode::Recursive).ok()?;

        // Single timer that drains the channel and applies changes.
        // Runs every 200 ms, providing natural debouncing for burst operations.
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let mut changed_dirs: HashSet<PathBuf> = HashSet::new();
            let mut current_modified = false;
            let mut current_removed = false;

            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        for path in &event.paths {
                            // Mark the parent directory as needing a refresh
                            if let Some(parent) = path.parent() {
                                changed_dirs.insert(parent.to_path_buf());
                            }
                            // Also mark the path itself if it's a directory
                            // (handles cases like directory creation/removal)
                            if path.is_dir() {
                                changed_dirs.insert(path.clone());
                            }

                            // Check whether the currently viewed file was affected
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
                        // Watcher was dropped — stop the timer
                        return glib::ControlFlow::Break;
                    }
                }
            }

            // Refresh affected directory stores
            for dir in &changed_dirs {
                // Clone the store Rc out of the borrow so we don't hold the
                // RefCell while refresh_directory also borrows dir_stores.
                let store = dir_stores.borrow().get(dir).cloned();
                if let Some(store) = store {
                    tree::refresh_directory(&store, dir, &dir_stores);
                }
            }

            // Handle current-file changes
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
