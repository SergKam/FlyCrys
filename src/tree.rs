use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::config::constants::TREE_SEARCH_MAX_RESULTS;
use crate::file_entry::FileEntry;

/// Shared map of directory path → ListStore, so the watcher can refresh stores.
pub type DirStoreMap = Rc<RefCell<HashMap<PathBuf, gio::ListStore>>>;

struct DirEntry {
    name: String,
    path: String,
    icon: &'static str,
    is_dir: bool,
}

/// All widgets produced by [`create_tree_panel`].
pub struct TreePanel {
    /// Vertical box containing toolbar, search entry, tree scroll, and search scroll.
    pub container: gtk::Box,
    pub tree_scroll: gtk::ScrolledWindow,
    pub list_view: gtk::ListView,
    pub selection: gtk::SingleSelection,
    pub dir_stores: DirStoreMap,
    pub search_btn: gtk::ToggleButton,
    pub search_entry: gtk::SearchEntry,
    pub search_store: gio::ListStore,
    pub search_selection: gtk::SingleSelection,
    pub search_list: gtk::ListView,
    pub search_scroll: gtk::ScrolledWindow,
}

/// Build the full left-pane tree panel: toolbar, search bar, file tree, search
/// results list.  Signal wiring (open-file, collapse, search-changed) stays in
/// the caller so it can capture workspace-level state.
pub fn create_tree_panel(root_dir: &Path) -> TreePanel {
    let (tree_scroll, list_view, selection, dir_stores) = create_file_tree(root_dir);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // ── Toolbar ──
    let tree_toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tree_toolbar.add_css_class("tree-toolbar");

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    tree_toolbar.append(&spacer);

    let collapse_btn = gtk::Button::from_icon_name("view-list-symbolic");
    collapse_btn.set_tooltip_text(Some("Collapse All"));
    collapse_btn.set_has_frame(false);
    tree_toolbar.append(&collapse_btn);

    let search_btn = gtk::ToggleButton::new();
    search_btn.set_icon_name("edit-find-symbolic");
    search_btn.set_tooltip_text(Some("Search Files"));
    search_btn.set_has_frame(false);
    tree_toolbar.append(&search_btn);

    container.append(&tree_toolbar);

    // ── Search entry (hidden by default) ──
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search files\u{2026}"));
    search_entry.set_visible(false);
    container.append(&search_entry);

    // ── Search results list (hidden by default) ──
    let search_store = gio::ListStore::new::<FileEntry>();
    let search_selection = gtk::SingleSelection::new(Some(search_store.clone()));
    let search_factory = build_search_factory();

    let search_list = gtk::ListView::new(Some(search_selection.clone()), Some(search_factory));
    search_list.add_css_class("file-tree");

    let search_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&search_list)
        .vexpand(true)
        .visible(false)
        .build();

    tree_scroll.set_vexpand(true);
    container.append(&tree_scroll);
    container.append(&search_scroll);

    // ── Collapse All (self-contained) ──
    {
        let sel = selection.clone();
        collapse_btn.connect_clicked(move |_| collapse_all(&sel));
    }

    // ── Search toggle ──
    {
        let se = search_entry.clone();
        let ts = tree_scroll.clone();
        let ss = search_scroll.clone();
        let store = search_store.clone();
        search_btn.connect_toggled(move |btn| {
            let active = btn.is_active();
            se.set_visible(active);
            if active {
                se.grab_focus();
            } else {
                se.set_text("");
                store.remove_all();
                ss.set_visible(false);
                ts.set_visible(true);
            }
        });
    }

    // ── Search Escape ──
    {
        let sb = search_btn.clone();
        search_entry.connect_stop_search(move |_| sb.set_active(false));
    }

    TreePanel {
        container,
        tree_scroll,
        list_view,
        selection,
        dir_stores,
        search_btn,
        search_entry,
        search_store,
        search_selection,
        search_list,
        search_scroll,
    }
}

/// Wire the search-changed signal — needs the working directory so it lives
/// outside `create_tree_panel`.
pub fn wire_search(panel: &TreePanel, working_dir: &Path) {
    let store = panel.search_store.clone();
    let tree_scroll = panel.tree_scroll.clone();
    let search_scroll = panel.search_scroll.clone();
    let wd = working_dir.to_path_buf();
    panel.search_entry.connect_search_changed(move |entry| {
        let query = entry.text().to_string();
        store.remove_all();
        if query.is_empty() {
            search_scroll.set_visible(false);
            tree_scroll.set_visible(true);
            return;
        }
        tree_scroll.set_visible(false);
        search_scroll.set_visible(true);
        let results = search_files(&wd, &query, TREE_SEARCH_MAX_RESULTS);
        for (rel, full) in results {
            store.append(&FileEntry::new(&rel, &full, "text-x-generic", false));
        }
    });
}

/// Wire click-to-open on search results.
pub fn wire_search_activate(panel: &TreePanel, on_open_file: &Rc<dyn Fn(&str)>) {
    let oof = Rc::clone(on_open_file);
    let sb = panel.search_btn.clone();
    let sel = panel.search_selection.clone();
    panel.search_list.connect_activate(move |_list, pos| {
        if let Some(entry) = sel.item(pos).and_downcast::<FileEntry>() {
            oof(&entry.path());
            sb.set_active(false);
        }
    });
}

// ── File tree construction (internal) ────────────────────────────────────────

fn create_file_tree(
    root_dir: &Path,
) -> (
    gtk::ScrolledWindow,
    gtk::ListView,
    gtk::SingleSelection,
    DirStoreMap,
) {
    let dir_stores: DirStoreMap = Rc::new(RefCell::new(HashMap::new()));

    let root_store = create_directory_list(root_dir);
    dir_stores
        .borrow_mut()
        .insert(root_dir.to_path_buf(), root_store.clone());

    let stores = dir_stores.clone();
    let tree_model = gtk::TreeListModel::new(root_store, false, false, move |item| {
        let entry = item.downcast_ref::<FileEntry>()?;
        if !entry.is_dir() {
            return None;
        }
        let path = PathBuf::from(entry.path());
        let store = create_directory_list(&path);
        stores.borrow_mut().insert(path, store.clone());
        Some(store.upcast())
    });

    let selection = gtk::SingleSelection::new(Some(tree_model));

    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let expander = gtk::TreeExpander::new();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        hbox.set_margin_start(4);
        let icon = gtk::Image::new();
        icon.set_pixel_size(16);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        hbox.append(&icon);
        hbox.append(&label);
        expander.set_child(Some(&hbox));
        list_item.set_child(Some(&expander));
    });

    factory.connect_bind(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = list_item.item().and_downcast::<gtk::TreeListRow>().unwrap();
        let entry = row.item().and_downcast::<FileEntry>().unwrap();
        let expander = list_item
            .child()
            .and_downcast::<gtk::TreeExpander>()
            .unwrap();
        expander.set_list_row(Some(&row));
        let hbox = expander.child().and_downcast::<gtk::Box>().unwrap();
        let icon = hbox.first_child().and_downcast::<gtk::Image>().unwrap();
        icon.set_from_gicon(&content_type_icon(&entry));
        let label = icon.next_sibling().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&entry.name());
    });

    factory.connect_unbind(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        if let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() {
            expander.set_list_row(None::<&gtk::TreeListRow>);
        }
    });

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.add_css_class("file-tree");

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .build();

    (scrolled, list_view, selection, dir_stores)
}

fn build_search_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        hbox.set_margin_start(8);
        hbox.set_margin_top(2);
        hbox.set_margin_bottom(2);
        let icon = gtk::Image::new();
        icon.set_pixel_size(16);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        hbox.append(&icon);
        hbox.append(&label);
        list_item.set_child(Some(&hbox));
    });

    factory.connect_bind(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let entry = list_item.item().and_downcast::<FileEntry>().unwrap();
        let hbox = list_item.child().and_downcast::<gtk::Box>().unwrap();
        let icon = hbox.first_child().and_downcast::<gtk::Image>().unwrap();
        icon.set_from_gicon(&content_type_icon(&entry));
        let label = icon.next_sibling().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&entry.name());
        label.set_tooltip_text(Some(&entry.path()));
    });

    factory
}

// ── Public helpers ───────────────────────────────────────────────────────────

/// Refresh a directory's ListStore to match the current filesystem state.
/// Removes deleted items, inserts new items at the correct sorted position.
/// Existing items are untouched so TreeListModel preserves their expand state.
pub fn refresh_directory(store: &gio::ListStore, dir: &Path, dir_stores: &DirStoreMap) {
    let Some(desired) = read_directory_entries(dir) else {
        // Directory no longer exists — clear store and unregister
        while store.n_items() > 0 {
            store.remove(0);
        }
        dir_stores.borrow_mut().remove(&dir.to_path_buf());
        return;
    };

    let desired_paths: HashSet<String> = desired.iter().map(|e| e.path.clone()).collect();

    // Remove items no longer on disk (scan backwards to keep indices valid)
    let mut i = store.n_items();
    while i > 0 {
        i -= 1;
        if let Some(entry) = store.item(i).and_downcast::<FileEntry>()
            && !desired_paths.contains(&entry.path())
        {
            if entry.is_dir() {
                dir_stores.borrow_mut().remove(&PathBuf::from(entry.path()));
            }
            store.remove(i);
        }
    }

    // Build set of paths still in the store after removals
    let existing_paths: HashSet<String> = (0..store.n_items())
        .filter_map(|i| store.item(i).and_downcast::<FileEntry>())
        .map(|e| e.path())
        .collect();

    // Walk the desired order, inserting missing items at the correct position.
    let mut store_idx = 0u32;
    for de in &desired {
        if existing_paths.contains(&de.path) {
            store_idx += 1;
        } else {
            store.insert(
                store_idx,
                &FileEntry::new(&de.name, &de.path, de.icon, de.is_dir),
            );
            store_idx += 1;
        }
    }
}

/// Collapse every expanded row in the tree.
pub fn collapse_all(selection: &gtk::SingleSelection) {
    let n = selection.n_items();
    for i in (0..n).rev() {
        if let Some(row) = selection.item(i).and_downcast::<gtk::TreeListRow>()
            && row.is_expanded()
        {
            row.set_expanded(false);
        }
    }
}

/// Recursively search for files whose name contains `query` (case-insensitive).
/// Returns up to `limit` matching paths, sorted alphabetically.
pub fn search_files(root: &Path, query: &str, limit: usize) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    let mut results: Vec<(String, String)> = Vec::new();
    search_files_recursive(root, root, &query_lower, limit, &mut results);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.truncate(limit);
    results
}

fn search_files_recursive(
    base: &Path,
    dir: &Path,
    query: &str,
    limit: usize,
    results: &mut Vec<(String, String)>,
) {
    if results.len() >= limit {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|a| a.file_name());
    for entry in entries {
        if results.len() >= limit {
            return;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" || name_str == "target" || name_str == "node_modules" {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if !is_dir && name_str.to_lowercase().contains(query) {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let full = path.to_string_lossy().to_string();
            results.push((rel, full));
        }
        if is_dir {
            search_files_recursive(base, &path, query, limit, results);
        }
    }
}

/// Return a system-themed icon for the given file entry based on its MIME type.
/// Uses `gio::content_type_guess` (filename-only, no disk I/O).
fn content_type_icon(entry: &FileEntry) -> gio::Icon {
    if entry.is_dir() {
        return gio::content_type_get_icon("inode/directory");
    }
    let (ct, _uncertain) = gio::content_type_guess(Some(&entry.name()), None::<&[u8]>);
    gio::content_type_get_icon(&ct)
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn read_directory_entries(dir: &Path) -> Option<Vec<DirEntry>> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut raw: Vec<_> = rd.filter_map(|e| e.ok()).collect();

    raw.sort_by(|a, b| {
        let a_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| {
            a.file_name()
                .to_ascii_lowercase()
                .cmp(&b.file_name().to_ascii_lowercase())
        })
    });

    Some(
        raw.into_iter()
            .filter_map(|entry| {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();
                if name_str == ".git" {
                    return None;
                }
                let path = entry.path();
                let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                let icon = if is_dir { "folder" } else { "text-x-generic" };
                Some(DirEntry {
                    name: name_str,
                    path: path.to_string_lossy().to_string(),
                    icon,
                    is_dir,
                })
            })
            .collect(),
    )
}

fn create_directory_list(dir: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<FileEntry>();
    if let Some(entries) = read_directory_entries(dir) {
        for e in entries {
            store.append(&FileEntry::new(&e.name, &e.path, e.icon, e.is_dir));
        }
    }
    store
}
