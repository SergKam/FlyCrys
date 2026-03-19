use gtk::prelude::*;
use gtk4 as gtk;
use std::path::Path;

use crate::highlight;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

/// Returns (container, code_text_view, gutter_text_view, path_label).
pub fn create_text_view() -> (gtk::Box, gtk::TextView, gtk::TextView, gtk::Label) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let path_label = gtk::Label::new(Some("Select a file to view"));
    path_label.set_xalign(0.0);
    path_label.set_margin_start(8);
    path_label.set_margin_end(8);
    path_label.set_margin_top(4);
    path_label.set_margin_bottom(4);
    path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);

    // Line number gutter
    let gutter = gtk::TextView::new();
    gutter.set_editable(false);
    gutter.set_cursor_visible(false);
    gutter.set_monospace(true);
    gutter.set_right_margin(8);
    gutter.set_left_margin(4);
    gutter.set_top_margin(8);
    gutter.set_justification(gtk::Justification::Right);
    gutter.set_focusable(false);
    // Remove ALL built-in GestureClick controllers from the gutter.
    // GtkTextView ships an internal GestureClick (button 0 = any) that shows
    // a default Copy/Paste context menu on right-click. Since the gutter is a
    // read-only display widget, we strip those out and add our own in workspace.rs.
    {
        let model = gutter.observe_controllers();
        let mut to_remove: Vec<gtk::EventController> = Vec::new();
        for i in 0..model.n_items() {
            if let Some(obj) = model.item(i)
                && obj.is::<gtk::GestureClick>()
                && let Ok(ctrl) = obj.downcast::<gtk::EventController>()
            {
                to_remove.push(ctrl);
            }
        }
        for ctrl in to_remove {
            gutter.remove_controller(&ctrl);
        }
    }
    gutter.add_css_class("line-gutter");

    // Code view
    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::None);
    text_view.set_left_margin(8);
    text_view.set_top_margin(8);
    text_view.add_css_class("code-view");

    let code_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .child(&text_view)
        .build();

    // Gutter shares the vertical scroll adjustment with the code view.
    // PolicyType::External = "scrolling managed elsewhere, don't size to content".
    // PolicyType::Never would make the gutter request its full content height,
    // breaking the parent's scroll layout.
    let gutter_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vadjustment(&code_scrolled.vadjustment())
        .child(&gutter)
        .build();

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_vexpand(true);
    hbox.set_hexpand(true);
    hbox.append(&gutter_scrolled);
    hbox.append(&code_scrolled);

    vbox.append(&path_label);
    vbox.append(&hbox);

    (vbox, text_view, gutter, path_label)
}

/// Reset the text view to the initial "no file selected" state.
pub fn clear_view(text_view: &gtk::TextView, gutter: &gtk::TextView, path_label: &gtk::Label) {
    text_view.buffer().set_text("");
    gutter.buffer().set_text("");
    gutter.set_size_request(-1, -1);
    path_label.set_text("Select a file to view");
}

pub fn load_file(
    text_view: &gtk::TextView,
    gutter: &gtk::TextView,
    path_label: &gtk::Label,
    file_path: &str,
    theme: &str,
) {
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
            update_gutter(gutter, 1);
            return;
        }
        Err(e) => {
            buffer.set_text(&format!("Error reading file: {e}"));
            update_gutter(gutter, 1);
            return;
        }
        _ => {}
    }

    match std::fs::read_to_string(path) {
        Ok(content) => {
            let line_count = content.lines().count().max(1);
            if highlight::is_highlightable(file_path) {
                highlight::highlight_buffer_with_theme(&buffer, &content, file_path, theme);
            } else {
                buffer.set_text(&content);
            }
            update_gutter(gutter, line_count);
        }
        Err(e) => {
            buffer.set_text(&format!("Cannot read file: {e}"));
            update_gutter(gutter, 1);
        }
    }
}

fn update_gutter(gutter: &gtk::TextView, line_count: usize) {
    let digits = format!("{}", line_count).len().max(3);
    // ~10px per digit + left(4) + right(8) margins
    let width = digits as i32 * 10 + 12;
    gutter.set_size_request(width, -1);

    let text: String = (1..=line_count)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    gutter.buffer().set_text(&text);
}
