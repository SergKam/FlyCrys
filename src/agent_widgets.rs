use gtk4 as gtk;
use gtk::prelude::*;

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
pub fn update_assistant_text(label: &gtk::Label, raw_md: &str) {
    let markup = crate::markdown::md_to_pango(raw_md);
    label.set_markup(&markup);
}

/// Tool call expandable panel
pub fn create_tool_call(tool_name: &str, tool_input_hint: &str) -> (gtk::Expander, gtk::Box) {
    let title = if tool_input_hint.is_empty() {
        format!("● {tool_name}(…)")
    } else {
        let short = if tool_input_hint.len() > 60 {
            format!("{}…", &tool_input_hint[..57])
        } else {
            tool_input_hint.to_string()
        };
        format!("● {tool_name}({short})")
    };

    let expander = gtk::Expander::new(Some(&title));
    expander.set_margin_start(8);
    expander.set_margin_end(8);
    expander.set_margin_top(2);
    expander.set_margin_bottom(2);
    expander.add_css_class("tool-call");

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content_box.set_margin_start(16);

    // Spinner shown while tool runs
    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    spinner.set_halign(gtk::Align::Start);
    content_box.append(&spinner);

    expander.set_child(Some(&content_box));

    (expander, content_box)
}

/// Fill tool result into the tool call's content box
pub fn fill_tool_result(content_box: &gtk::Box, output: &str, is_error: bool) {
    // Remove the spinner
    while let Some(child) = content_box.first_child() {
        content_box.remove(&child);
    }

    let text = if output.len() > 2000 {
        let lines: Vec<&str> = output.lines().collect();
        let shown = if lines.len() > 10 {
            let head: Vec<&str> = lines[..5].to_vec();
            let tail: Vec<&str> = lines[lines.len() - 5..].to_vec();
            format!(
                "{}\n  … +{} lines …\n{}",
                head.join("\n"),
                lines.len() - 10,
                tail.join("\n"),
            )
        } else {
            output[..2000].to_string() + "…"
        };
        shown
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
