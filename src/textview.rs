use gtk4 as gtk;
use gtk::prelude::*;
use std::path::Path;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

pub fn create_text_view() -> (gtk::Box, gtk::TextView, gtk::Label) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let path_label = gtk::Label::new(Some("Select a file to view"));
    path_label.set_xalign(0.0);
    path_label.set_margin_start(8);
    path_label.set_margin_end(8);
    path_label.set_margin_top(4);
    path_label.set_margin_bottom(4);
    path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);

    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::None);
    text_view.set_left_margin(8);
    text_view.set_top_margin(8);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .child(&text_view)
        .build();

    vbox.append(&path_label);
    vbox.append(&scrolled);

    (vbox, text_view, path_label)
}

pub fn load_file(text_view: &gtk::TextView, path_label: &gtk::Label, file_path: &str) {
    let path = Path::new(file_path);
    path_label.set_text(file_path);

    let buffer = text_view.buffer();

    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE_SIZE => {
            buffer.set_text(&format!(
                "File too large to display ({:.1} MB, max {} MB)",
                meta.len() as f64 / (1024.0 * 1024.0),
                MAX_FILE_SIZE / (1024 * 1024)
            ));
            return;
        }
        Err(e) => {
            buffer.set_text(&format!("Error reading file: {e}"));
            return;
        }
        _ => {}
    }

    match std::fs::read_to_string(path) {
        Ok(content) => buffer.set_text(&content),
        Err(e) => buffer.set_text(&format!("Cannot read file: {e}")),
    }
}
