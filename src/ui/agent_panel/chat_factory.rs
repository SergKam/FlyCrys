use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

use crate::agent_widgets;
use crate::chat_entry::{
    ChatEntry, MSG_TYPE_ASSISTANT, MSG_TYPE_SYSTEM, MSG_TYPE_THINKING, MSG_TYPE_TOOL_CALL,
    MSG_TYPE_USER,
};

/// Build the widget tree for a ChatEntry and cache it on the entry.
/// Returns the top-level widget (ready to append to a Box).
pub(super) fn build_and_cache_widget(
    entry: &ChatEntry,
    on_open_file: &Rc<dyn Fn(&str)>,
    is_dark: bool,
) -> gtk::Widget {
    if let Some(w) = entry.cached_widget() {
        return w;
    }
    let w = build_widget(entry, on_open_file, is_dark);
    entry.set_cached_widget(Some(w.clone()));
    w
}

fn build_widget(entry: &ChatEntry, on_open_file: &Rc<dyn Fn(&str)>, is_dark: bool) -> gtk::Widget {
    match entry.msg_type() {
        MSG_TYPE_USER => build_user_widget(entry),
        MSG_TYPE_ASSISTANT => build_assistant_widget(entry, on_open_file, is_dark),
        MSG_TYPE_TOOL_CALL => build_tool_widget(entry, on_open_file),
        MSG_TYPE_SYSTEM => build_system_widget(entry),
        MSG_TYPE_THINKING => build_thinking_widget(),
        _ => gtk::Label::new(Some("???")).upcast(),
    }
}

fn build_user_widget(entry: &ChatEntry) -> gtk::Widget {
    let textures = entry.textures();
    if textures.is_empty() {
        agent_widgets::create_user_message(&entry.text()).upcast()
    } else {
        agent_widgets::create_user_message_with_images(&entry.text(), &textures).upcast()
    }
}

fn build_assistant_widget(
    entry: &ChatEntry,
    on_open_file: &Rc<dyn Fn(&str)>,
    is_dark: bool,
) -> gtk::Widget {
    let (container, label) = agent_widgets::create_assistant_text();

    let cb = on_open_file.clone();
    label.connect_activate_link(move |_label, uri| {
        if let Some(path) = uri.strip_prefix("file://") {
            cb(path);
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });

    let text = entry.text();
    if !text.is_empty() {
        agent_widgets::update_assistant_text(&label, &text, is_dark);
    }

    entry.set_text_label(Some(label));
    container.upcast()
}

fn build_tool_widget(entry: &ChatEntry, on_open_file: &Rc<dyn Fn(&str)>) -> gtk::Widget {
    let file_path_str = entry.file_path();
    let file_path = if file_path_str.is_empty() {
        None
    } else {
        Some(file_path_str.as_str())
    };

    let (container, triangle, content_box, spinner) = agent_widgets::create_tool_call(
        &entry.tool_name(),
        &entry.tool_display_hint(),
        file_path,
        on_open_file.clone(),
    );

    if entry.tool_complete() {
        agent_widgets::mark_tool_complete(&spinner, &triangle, entry.tool_is_error());

        let output = entry.tool_output();
        if !output.trim().is_empty() {
            let tool_name = entry.tool_name();
            let tool_input = entry.tool_input();
            let is_err = entry.tool_is_error();
            let rendered = Cell::new(false);
            content_box.connect_map(move |cb| {
                if !rendered.get() {
                    rendered.set(true);
                    agent_widgets::render_tool_output(cb, &output, is_err, &tool_name, &tool_input);
                }
            });
        }
    }

    entry.set_tool_spinner_widget(Some(spinner));
    entry.set_tool_triangle_widget(Some(triangle));
    entry.set_tool_content_box_widget(Some(content_box));

    container.upcast()
}

fn build_system_widget(entry: &ChatEntry) -> gtk::Widget {
    agent_widgets::create_system_message(&entry.text()).upcast()
}

fn build_thinking_widget() -> gtk::Widget {
    agent_widgets::create_thinking_spinner().upcast()
}
