use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::FileEntry)]
    pub struct FileEntry {
        #[property(get, set)]
        name: RefCell<String>,
        #[property(get, set)]
        path: RefCell<String>,
        #[property(get, set)]
        icon_name: RefCell<String>,
        #[property(get, set)]
        is_dir: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileEntry {
        const NAME: &'static str = "FlyCristalFileEntry";
        type Type = super::FileEntry;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for FileEntry {}
}

glib::wrapper! {
    pub struct FileEntry(ObjectSubclass<imp::FileEntry>);
}

impl FileEntry {
    pub fn new(name: &str, path: &str, icon_name: &str, is_dir: bool) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("path", path)
            .property("icon-name", icon_name)
            .property("is-dir", is_dir)
            .build()
    }
}
