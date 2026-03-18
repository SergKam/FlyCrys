use gtk4 as gtk;
use gtk::gio;
use gtk::prelude::*;
use std::path::Path;

use crate::file_entry::FileEntry;

pub fn create_file_tree() -> (gtk::ScrolledWindow, gtk::ListView, gtk::SingleSelection) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
    let root_store = create_directory_list(&cwd);

    let tree_model = gtk::TreeListModel::new(root_store, false, false, |item| {
        let entry = item.downcast_ref::<FileEntry>()?;
        if !entry.is_dir() {
            return None;
        }
        let store = create_directory_list(Path::new(&entry.path()));
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
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        hbox.append(&icon);
        hbox.append(&label);

        expander.set_child(Some(&hbox));
        list_item.set_child(Some(&expander));
    });

    factory.connect_bind(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = list_item
            .item()
            .and_downcast::<gtk::TreeListRow>()
            .unwrap();
        let entry = row.item().and_downcast::<FileEntry>().unwrap();

        let expander = list_item
            .child()
            .and_downcast::<gtk::TreeExpander>()
            .unwrap();
        expander.set_list_row(Some(&row));

        let hbox = expander.child().and_downcast::<gtk::Box>().unwrap();
        let icon = hbox.first_child().and_downcast::<gtk::Image>().unwrap();
        icon.set_icon_name(Some(&entry.icon_name()));

        let label = icon
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .unwrap();
        label.set_text(&entry.name());
    });

    factory.connect_unbind(|_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        if let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() {
            expander.set_list_row(None::<&gtk::TreeListRow>);
        }
    });

    let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list_view.set_single_click_activate(true);
    list_view.add_css_class("file-tree");

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&list_view)
        .build();

    (scrolled, list_view, selection)
}

fn create_directory_list(dir: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<FileEntry>();

    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return store,
    };

    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();

    entries.sort_by(|a, b| {
        let a_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| {
            a.file_name()
                .to_ascii_lowercase()
                .cmp(&b.file_name().to_ascii_lowercase())
        })
    });

    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let icon = if is_dir {
            "folder"
        } else {
            "text-x-generic"
        };

        store.append(&FileEntry::new(
            &name_str,
            &path.to_string_lossy(),
            icon,
            is_dir,
        ));
    }

    store
}
