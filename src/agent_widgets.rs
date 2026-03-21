use gtk::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

use crate::config::constants::{
    DISPLAY_TRUNCATE_AT, DISPLAY_TRUNCATE_KEEP, IMAGE_THUMBNAIL_HEIGHT, IMAGE_THUMBNAIL_WIDTH,
    OUTPUT_COLLAPSE_THRESHOLD, OUTPUT_HEAD_TAIL_LINES,
};

/// User message bubble (right-aligned)
pub fn create_user_message(text: &str) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_halign(gtk::Align::End);
    container.set_margin_start(48);
    container.set_margin_end(8);
    container.set_margin_top(4);
    container.set_margin_bottom(4);

    let frame = gtk::Frame::new(None);
    frame.add_css_class("user-message");

    let label = gtk::Label::new(Some(text));
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.set_margin_start(10);
    label.set_margin_end(10);
    label.set_margin_top(6);
    label.set_margin_bottom(6);

    frame.set_child(Some(&label));
    container.append(&frame);
    container
}

/// Assistant text block (left-aligned, updated during streaming with Pango markup)
pub fn create_assistant_text() -> (gtk::Box, gtk::Label) {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_halign(gtk::Align::Start);
    container.set_margin_start(8);
    container.set_margin_end(48);
    container.set_margin_top(4);
    container.set_margin_bottom(4);

    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.set_use_markup(true);

    container.append(&label);
    (container, label)
}

/// Update assistant label with markdown-rendered content
pub fn update_assistant_text(label: &gtk::Label, raw_md: &str, is_dark: bool) {
    let markup = crate::markdown::md_to_pango(raw_md, is_dark);
    // Validate markup first; if invalid, fall back to plain text so content is never lost
    if gtk::pango::parse_markup(&markup, '\0').is_ok() {
        label.set_markup(&markup);
    } else {
        label.set_text(raw_md);
    }
}

/// Tool call panel: header with spinner + tool info, expandable output area.
/// Returns (container, content_box, spinner, expander).
pub fn create_tool_call(
    tool_name: &str,
    tool_input_hint: &str,
    file_path: Option<&str>,
    on_open_file: Rc<dyn Fn(&str)>,
) -> (gtk::Box, gtk::Box, gtk::Spinner, gtk::Expander) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
    container.set_margin_start(8);
    container.set_margin_end(8);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.add_css_class("tool-call");

    // Header row: spinner + tool name(args)
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(16, 16);
    header.append(&spinner);

    if let Some(path) = file_path {
        let name_label = gtk::Label::new(Some(&format!("{tool_name}(")));
        header.append(&name_label);

        let short_path = if path.len() > DISPLAY_TRUNCATE_AT {
            let truncated: String = path.chars().take(DISPLAY_TRUNCATE_KEEP).collect();
            format!("{truncated}\u{2026}")
        } else {
            path.to_string()
        };

        let link_label = gtk::Label::new(None);
        link_label.set_use_markup(true);
        link_label.set_markup(&format!(
            "<span foreground=\"#4a90d9\" underline=\"single\">{}</span>",
            escape_markup(&short_path)
        ));

        let path_btn = gtk::Button::new();
        path_btn.set_child(Some(&link_label));
        path_btn.add_css_class("flat");
        path_btn.add_css_class("file-link");
        path_btn.set_tooltip_text(Some(path));

        let path_owned = path.to_string();
        path_btn.connect_clicked(move |_| {
            on_open_file(&path_owned);
        });
        header.append(&path_btn);

        let close_label = gtk::Label::new(Some(")"));
        header.append(&close_label);
    } else {
        let short = if tool_input_hint.len() > DISPLAY_TRUNCATE_AT {
            format!("{}\u{2026}", &tool_input_hint[..DISPLAY_TRUNCATE_KEEP])
        } else {
            tool_input_hint.to_string()
        };
        let label = gtk::Label::new(Some(&format!("{tool_name}({short})")));
        header.append(&label);
    }

    container.append(&header);

    // Expandable output area (hidden until result arrives)
    let expander = gtk::Expander::new(Some("Output"));
    expander.set_expanded(false);
    expander.set_visible(false);
    expander.set_margin_start(20);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    expander.set_child(Some(&content_box));
    container.append(&expander);

    (container, content_box, spinner, expander)
}

fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fill tool result into the tool call's content box
pub fn fill_tool_result(
    content_box: &gtk::Box,
    spinner: &gtk::Spinner,
    expander: &gtk::Expander,
    output: &str,
    is_error: bool,
    tool_name: &str,
    tool_input: &str,
) {
    spinner.set_spinning(false);
    spinner.set_visible(false);

    // No output — expander stays hidden (e.g. Read tool)
    if output.trim().is_empty() {
        return;
    }

    // For Edit tool, try to show a highlighted diff
    if tool_name == "Edit"
        && let Some(diff_markup) = create_edit_diff_markup(tool_input)
    {
        let label = gtk::Label::new(None);
        label.set_use_markup(true);
        label.set_markup(&diff_markup);
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_selectable(true);
        label.add_css_class("monospace");
        content_box.append(&label);
        expander.set_visible(true);
        return;
    }

    let text = if output.len() > OUTPUT_COLLAPSE_THRESHOLD {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() > OUTPUT_HEAD_TAIL_LINES * 2 {
            let head: Vec<&str> = lines[..OUTPUT_HEAD_TAIL_LINES].to_vec();
            let tail: Vec<&str> = lines[lines.len() - OUTPUT_HEAD_TAIL_LINES..].to_vec();
            format!(
                "{}\n  \u{2026} +{} lines \u{2026}\n{}",
                head.join("\n"),
                lines.len() - OUTPUT_HEAD_TAIL_LINES * 2,
                tail.join("\n"),
            )
        } else {
            output[..OUTPUT_COLLAPSE_THRESHOLD].to_string() + "\u{2026}"
        }
    } else {
        output.to_string()
    };

    let label = gtk::Label::new(Some(&text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.add_css_class("monospace");
    if is_error {
        label.add_css_class("error-text");
    }

    content_box.append(&label);
    expander.set_visible(true);
}

/// Try to build a Pango diff markup from Edit tool input JSON
fn create_edit_diff_markup(tool_input: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(tool_input).ok()?;
    let old_string = val.get("old_string")?.as_str()?;
    let new_string = val.get("new_string")?.as_str()?;
    let file_path = val.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

    Some(crate::highlight::diff_to_pango(
        old_string, new_string, file_path,
    ))
}

/// User message bubble with image thumbnails (right-aligned)
pub fn create_user_message_with_images(text: &str, textures: &[gtk::gdk::Texture]) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_halign(gtk::Align::End);
    container.set_margin_start(48);
    container.set_margin_end(8);
    container.set_margin_top(4);
    container.set_margin_bottom(4);

    let frame = gtk::Frame::new(None);
    frame.add_css_class("user-message");

    let inner = gtk::Box::new(gtk::Orientation::Vertical, 4);
    inner.set_margin_start(10);
    inner.set_margin_end(10);
    inner.set_margin_top(6);
    inner.set_margin_bottom(6);

    let img_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    img_row.set_halign(gtk::Align::End);
    for texture in textures {
        let picture = gtk::Picture::for_paintable(texture);
        picture.set_content_fit(gtk::ContentFit::Contain);
        let thumb = gtk::Frame::new(None);
        thumb.set_size_request(IMAGE_THUMBNAIL_WIDTH, IMAGE_THUMBNAIL_HEIGHT);
        thumb.set_overflow(gtk::Overflow::Hidden);
        thumb.set_child(Some(&picture));
        thumb.add_css_class("image-thumb");
        img_row.append(&thumb);
    }
    inner.append(&img_row);

    if !text.is_empty() {
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_xalign(0.0);
        label.set_selectable(true);
        inner.append(&label);
    }

    frame.set_child(Some(&inner));
    container.append(&frame);
    container
}

/// System/status message (subtle, centered)
pub fn create_system_message(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("system-info");
    label.set_margin_top(2);
    label.set_margin_bottom(2);
    label.set_halign(gtk::Align::Center);
    label
}

/// Thinking spinner shown at the bottom of the chat while the agent is processing
pub fn create_thinking_spinner() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    container.set_halign(gtk::Align::Start);
    container.set_margin_start(8);
    container.set_margin_top(4);
    container.set_margin_bottom(4);

    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_size_request(16, 16);

    let label = gtk::Label::new(Some("Thinking…"));
    label.add_css_class("system-info");

    container.append(&spinner);
    container.append(&label);
    container
}
