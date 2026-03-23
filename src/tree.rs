use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::file_entry::FileEntry;

/// Shared map of directory path → ListStore, so the watcher can refresh stores.
pub type DirStoreMap = Rc<RefCell<HashMap<PathBuf, gio::ListStore>>>;

struct DirEntry {
    name: String,
    path: String,
    icon: &'static str,
    is_dir: bool,
}

pub fn create_file_tree(
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
        icon.set_icon_name(Some(&entry.icon_name()));

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
    // Existing items are already in the same relative order (same sort).
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

// ── helpers ──────────────────────────────────────────────────────────────────

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
