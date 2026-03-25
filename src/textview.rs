use gtk::prelude::*;
use gtk4 as gtk;
use std::path::Path;

use crate::config::constants::{GUTTER_CHAR_WIDTH_PX, GUTTER_PADDING_PX, MAX_FILE_SIZE_BYTES};
use crate::highlight;
use crate::markdown;

pub struct TextViewPanel {
    pub container: gtk::Box,
    pub text_view: gtk::TextView,
    pub gutter: gtk::TextView,
    pub path_label: gtk::Label,
    pub open_btn: gtk::Button,
    pub edit_btn: gtk::Button,
    pub browser_btn: gtk::Button,
    pub terminal_btn: gtk::Button,
    pub copy_btn: gtk::Button,
    pub chat_btn: gtk::Button,
    pub diff_toggle: gtk::ToggleButton,
    pub view_toggle: gtk::ToggleButton,
    pub source_hbox: gtk::Box,
    pub preview_scroll: gtk::ScrolledWindow,
}

pub enum PreviewKind {
    Markdown,
    Image,
    None,
}

pub fn is_previewable(path: &str) -> PreviewKind {
    use crate::config::constants::{IMAGE_EXTENSIONS, MARKDOWN_EXTENSIONS};

    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
        PreviewKind::Markdown
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        PreviewKind::Image
    } else {
        PreviewKind::None
    }
}

pub fn create_text_view() -> TextViewPanel {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let path_label = gtk::Label::new(Some("Select a file to view"));
    path_label.set_xalign(0.0);
    path_label.set_margin_start(8);
    path_label.set_margin_end(8);
    path_label.set_margin_top(4);
    path_label.set_margin_bottom(4);
    path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);

    // Toolbar
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    toolbar.set_margin_start(4);
    toolbar.set_margin_end(4);
    toolbar.set_margin_top(2);
    toolbar.set_margin_bottom(2);

    let open_btn = gtk::Button::from_icon_name("document-open-symbolic");
    open_btn.set_tooltip_text(Some("Open (default app)"));
    open_btn.set_has_frame(false);
    open_btn.set_sensitive(false);

    let edit_btn = gtk::Button::from_icon_name("text-editor-symbolic");
    edit_btn.set_tooltip_text(Some("Edit in text editor"));
    edit_btn.set_has_frame(false);
    edit_btn.set_sensitive(false);

    let browser_btn = gtk::Button::from_icon_name("web-browser-symbolic");
    browser_btn.set_tooltip_text(Some("Open in browser"));
    browser_btn.set_has_frame(false);
    browser_btn.set_sensitive(false);

    let terminal_btn = gtk::Button::from_icon_name("utilities-terminal-symbolic");
    terminal_btn.set_tooltip_text(Some("Terminal here"));
    terminal_btn.set_has_frame(false);
    terminal_btn.set_sensitive(false);

    let copy_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy_btn.set_tooltip_text(Some("Copy file path"));
    copy_btn.set_has_frame(false);
    copy_btn.set_sensitive(false);

    let chat_btn = gtk::Button::from_icon_name("insert-text-symbolic");
    chat_btn.set_tooltip_text(Some("Add to chat"));
    chat_btn.set_has_frame(false);
    chat_btn.set_sensitive(false);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let diff_toggle = gtk::ToggleButton::new();
    diff_toggle.set_label("Diff");
    diff_toggle.set_tooltip_text(Some("Show git diff"));
    diff_toggle.set_has_frame(false);
    diff_toggle.set_sensitive(false);

    let view_toggle = gtk::ToggleButton::new();
    view_toggle.set_icon_name("view-reveal-symbolic");
    view_toggle.set_tooltip_text(Some("Preview"));
    view_toggle.set_has_frame(false);
    view_toggle.set_sensitive(false);

    toolbar.append(&open_btn);
    toolbar.append(&edit_btn);
    toolbar.append(&browser_btn);
    toolbar.append(&terminal_btn);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&copy_btn);
    toolbar.append(&chat_btn);
    toolbar.append(&spacer);
    toolbar.append(&diff_toggle);
    toolbar.append(&view_toggle);

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

    let gutter_scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vadjustment(&code_scrolled.vadjustment())
        .child(&gutter)
        .build();

    let source_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    source_hbox.set_vexpand(true);
    source_hbox.set_hexpand(true);
    source_hbox.append(&gutter_scrolled);
    source_hbox.append(&code_scrolled);

    // Preview area (hidden by default)
    let preview_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();
    preview_scroll.set_visible(false);

    vbox.append(&path_label);
    vbox.append(&toolbar);
    vbox.append(&source_hbox);
    vbox.append(&preview_scroll);

    TextViewPanel {
        container: vbox,
        text_view,
        gutter,
        path_label,
        open_btn,
        edit_btn,
        browser_btn,
        terminal_btn,
        copy_btn,
        chat_btn,
        diff_toggle,
        view_toggle,
        source_hbox,
        preview_scroll,
    }
}

/// Reset the text view to the initial "no file selected" state.
pub fn clear_view(text_view: &gtk::TextView, gutter: &gtk::TextView, path_label: &gtk::Label) {
    text_view.buffer().set_text("");
    gutter.buffer().set_text("");
    gutter.set_size_request(-1, -1);
    path_label.set_text("Select a file to view");
}

/// Load file source content. Does NOT reset view/diff toggle state.
/// Only forces source mode if the new file is not previewable and preview was active.
#[allow(clippy::too_many_arguments)]
pub fn load_file(
    text_view: &gtk::TextView,
    gutter: &gtk::TextView,
    path_label: &gtk::Label,
    file_path: &str,
    theme: &str,
    view_toggle: &gtk::ToggleButton,
    toolbar_btns: &[&gtk::Button],
) {
    let path = Path::new(file_path);
    path_label.set_text(file_path);

    // Enable toolbar buttons
    for btn in toolbar_btns {
        btn.set_sensitive(true);
    }

    // Set preview toggle sensitivity; force source mode if not previewable
    let previewable = matches!(
        is_previewable(file_path),
        PreviewKind::Markdown | PreviewKind::Image
    );
    view_toggle.set_sensitive(previewable);
    if !previewable && view_toggle.is_active() {
        view_toggle.set_active(false); // triggers handler → swaps to source
    }

    let buffer = text_view.buffer();

    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE_SIZE_BYTES => {
            buffer.set_text(&format!(
                "File too large to display ({:.1} MB, max {} MB)",
                meta.len() as f64 / (1024.0 * 1024.0),
                MAX_FILE_SIZE_BYTES / (1024 * 1024)
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

pub fn load_preview(preview_scroll: &gtk::ScrolledWindow, file_path: &str, is_dark: bool) {
    // Clear existing child
    preview_scroll.set_child(gtk::Widget::NONE);

    match is_previewable(file_path) {
        PreviewKind::Markdown => {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                let widget_box = markdown::md_to_widget_box_deferred(&content, is_dark, None);
                widget_box.set_valign(gtk::Align::Start);
                widget_box.set_margin_start(12);
                widget_box.set_margin_end(12);
                widget_box.set_margin_top(8);
                widget_box.set_margin_bottom(8);
                preview_scroll.set_child(Some(&widget_box));
            }
        }
        PreviewKind::Image => {
            let picture = gtk::Picture::for_filename(file_path);
            picture.set_content_fit(gtk::ContentFit::Contain);
            preview_scroll.set_child(Some(&picture));
        }
        PreviewKind::None => {}
    }
}

pub fn update_gutter(gutter: &gtk::TextView, line_count: usize) {
    let digits = format!("{}", line_count).len().max(3);
    let width = digits as i32 * GUTTER_CHAR_WIDTH_PX + GUTTER_PADDING_PX;
    gutter.set_size_request(width, -1);

    let text: String = (1..=line_count)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    gutter.buffer().set_text(&text);
}
