use gtk::glib;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::config::constants::{GIT_REFRESH_BACKSTOP_SECS, GIT_RESULT_POLL_MS};
use crate::git_panel::GitPanel;
use crate::services::git::{self as git_service, GitFileStatus, GitStatusEntry};
use crate::tree::{self, GitTreeStatusRef};

/// Coordinates git-status refresh for one workspace.
///
/// `git status` is a blocking subprocess, so running it on the GTK main loop (as
/// the old per-tab 5s timers did) froze the UI in proportion to repo size. This
/// controller runs it on a worker thread, coalesces overlapping requests, and
/// applies the result — feeding both the file-tree colorizer and the git
/// "Changes" panel from a single invocation.
///
/// Refresh is event-driven: call [`GitStatusController::trigger`] from the
/// worktree / `.git` inotify watchers and tool-result callbacks. A slow backstop
/// poll covers changes the watchers can't observe (collapsed, unwatched subtrees).
#[derive(Clone)]
pub struct GitStatusController {
    inner: Rc<Inner>,
}

struct Inner {
    working_dir: PathBuf,
    tx: mpsc::Sender<Vec<GitStatusEntry>>,
    /// A query is currently running on a worker thread.
    in_flight: Cell<bool>,
    /// A change arrived while a query was in flight — run once more on completion.
    pending: Cell<bool>,
}

impl GitStatusController {
    /// Request a refresh. Coalesces: at most one `git status` runs at a time, and
    /// any requests arriving mid-flight collapse into a single follow-up run.
    pub fn trigger(&self) {
        if self.inner.in_flight.get() {
            self.inner.pending.set(true);
            return;
        }
        self.spawn_query();
    }

    fn spawn_query(&self) {
        self.inner.in_flight.set(true);
        self.inner.pending.set(false);
        let wd = self.inner.working_dir.clone();
        let tx = self.inner.tx.clone();
        std::thread::spawn(move || {
            let entries = git_service::status(&wd).unwrap_or_default();
            let _ = tx.send(entries);
        });
    }
}

/// Wire up the off-thread git-status pipeline for a workspace and return a
/// controller whose [`GitStatusController::trigger`] drives event-driven refresh.
pub fn install(
    working_dir: &Path,
    list_view: gtk::ListView,
    git_status: GitTreeStatusRef,
    git_panel: Option<Rc<RefCell<GitPanel>>>,
) -> GitStatusController {
    let (tx, rx) = mpsc::channel::<Vec<GitStatusEntry>>();
    let controller = GitStatusController {
        inner: Rc::new(Inner {
            working_dir: working_dir.to_path_buf(),
            tx,
            in_flight: Cell::new(false),
            pending: Cell::new(false),
        }),
    };

    // Drain completed results on the main loop and apply them to the widgets.
    {
        let controller = controller.clone();
        let working_dir = working_dir.to_path_buf();
        glib::timeout_add_local(Duration::from_millis(GIT_RESULT_POLL_MS), move || {
            // Coalesce: if several results are queued, only the newest matters.
            let mut latest = None;
            while let Ok(entries) = rx.try_recv() {
                latest = Some(entries);
            }
            if let Some(entries) = latest {
                controller.inner.in_flight.set(false);
                apply(
                    &entries,
                    &list_view,
                    &git_status,
                    &working_dir,
                    git_panel.as_ref(),
                );
                // A change observed mid-flight queued a follow-up — run it now.
                if controller.inner.pending.get() {
                    controller.spawn_query();
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // Slow backstop poll for changes the inotify watchers don't observe.
    {
        let controller = controller.clone();
        glib::timeout_add_local(Duration::from_secs(GIT_REFRESH_BACKSTOP_SECS), move || {
            controller.trigger();
            glib::ControlFlow::Continue
        });
    }

    controller.trigger(); // initial paint
    controller
}

/// Apply a status snapshot to the tree colorizer and (optionally) the git panel.
fn apply(
    entries: &[GitStatusEntry],
    list_view: &gtk::ListView,
    git_status: &GitTreeStatusRef,
    working_dir: &Path,
    git_panel: Option<&Rc<RefCell<GitPanel>>>,
) {
    let file_map: HashMap<String, GitFileStatus> = entries
        .iter()
        .map(|e| (e.path.clone(), e.status.clone()))
        .collect();
    let dir_set = git_service::dirty_dirs(&file_map);
    {
        let mut gs = git_status.borrow_mut();
        gs.files = file_map;
        gs.dirs = dir_set;
    }
    tree::repaint_visible_labels(list_view, git_status, working_dir);
    if let Some(gp) = git_panel {
        gp.borrow().render(entries);
    }
}
