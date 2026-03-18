use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use crate::agent_process::{AgentEvent, AgentProcess, ProcessState};
use crate::agent_widgets;

struct PanelState {
    process: AgentProcess,
    current_text_label: Option<gtk::Label>,
    current_text: String,
    current_tool_name: Option<String>,
    current_tool_input: String,
    current_tool_use_id: Option<String>,
    pending_tools: HashMap<String, gtk::Box>, // tool_use_id → content_box
}

pub fn create_agent_panel() -> (gtk::Box, gtk::TextView) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_width_request(420);

    // Header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.set_margin_start(8);
    header.set_margin_end(8);
    header.set_margin_top(6);
    header.set_margin_bottom(6);

    let title = gtk::Label::new(Some("Agent"));
    title.add_css_class("heading");
    title.set_hexpand(true);
    title.set_xalign(0.0);

    header.append(&title);

    // Chat history
    let message_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    message_list.set_valign(gtk::Align::End);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&message_list)
        .build();

    // Controls
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.set_margin_start(8);
    controls.set_margin_end(8);
    controls.set_margin_top(4);
    controls.set_margin_bottom(2);

    let pause_btn = gtk::Button::with_label("Pause");
    pause_btn.set_sensitive(false);
    let stop_btn = gtk::Button::with_label("Stop");
    stop_btn.set_sensitive(false);
    stop_btn.add_css_class("destructive-action");
    let clear_btn = gtk::Button::with_label("Clear");

    controls.append(&pause_btn);
    controls.append(&stop_btn);
    controls.append(&clear_btn);

    // Input area
    let input_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    input_box.set_margin_start(8);
    input_box.set_margin_end(8);
    input_box.set_margin_top(2);
    input_box.set_margin_bottom(8);

    let input_frame = gtk::Frame::new(None);
    input_frame.set_hexpand(true);

    let input_view = gtk::TextView::new();
    input_view.set_wrap_mode(gtk::WrapMode::WordChar);
    input_view.set_left_margin(6);
    input_view.set_right_margin(6);
    input_view.set_top_margin(4);
    input_view.set_bottom_margin(4);
    input_view.set_height_request(50);

    let input_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .max_content_height(120)
        .propagate_natural_height(true)
        .child(&input_view)
        .build();

    input_frame.set_child(Some(&input_scroll));

    let send_btn = gtk::Button::with_label("Send");
    send_btn.set_valign(gtk::Align::End);
    send_btn.add_css_class("suggested-action");

    input_box.append(&input_frame);
    input_box.append(&send_btn);

    // Assemble
    panel.append(&header);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&scrolled);
    panel.append(&controls);
    panel.append(&input_box);

    // --- State ---
    let state = Rc::new(RefCell::new(PanelState {
        process: AgentProcess::new(),
        current_text_label: None,
        current_text: String::new(),
        current_tool_name: None,
        current_tool_input: String::new(),
        current_tool_use_id: None,
        pending_tools: HashMap::new(),
    }));

    // --- Send handler ---
    let do_send = {
        let state = Rc::clone(&state);
        let message_list = message_list.clone();
        let scrolled = scrolled.clone();
        let input_view = input_view.clone();
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn = stop_btn.clone();

        move || {
            let buffer = input_view.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            buffer.set_text("");

            // Add user bubble
            let user_widget = agent_widgets::create_user_message(&text);
            message_list.append(&user_widget);
            scroll_to_bottom(&scrolled);

            let mut s = state.borrow_mut();

            if !s.process.is_alive() {
                // Spawn new process
                let (sender, receiver) = mpsc::channel::<AgentEvent>();
                let receiver = Rc::new(RefCell::new(Some(receiver)));

                if !s.process.spawn(sender) {
                    let err = agent_widgets::create_system_message("Failed to start claude CLI");
                    message_list.append(&err);
                    return;
                }

                // Poll receiver on GTK main loop (~60fps)
                let state_rx = Rc::clone(&state);
                let ml = message_list.clone();
                let sc = scrolled.clone();
                let send_btn_rx = send_btn.clone();
                let pause_btn_rx = pause_btn.clone();
                let stop_btn_rx = stop_btn.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                    let rx_ref = receiver.borrow();
                    let Some(ref rx) = *rx_ref else {
                        return glib::ControlFlow::Break;
                    };
                    while let Ok(event) = rx.try_recv() {
                        handle_event(
                            &state_rx,
                            &ml,
                            &sc,
                            &send_btn_rx,
                            &pause_btn_rx,
                            &stop_btn_rx,
                            event,
                        );
                    }
                    glib::ControlFlow::Continue
                });
            }

            s.process.send_message(&text);
            drop(s);

            send_btn.set_sensitive(false);
            pause_btn.set_sensitive(true);
            stop_btn.set_sensitive(true);
        }
    };

    // Send button click
    let do_send_click = do_send.clone();
    send_btn.connect_clicked(move |_| do_send_click());

    // Ctrl+Enter to send
    let key_ctrl = gtk::EventControllerKey::new();
    let do_send_key = do_send.clone();
    key_ctrl.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk::gdk::Key::Return
            && modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
        {
            do_send_key();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    input_view.add_controller(key_ctrl);

    // Pause button
    {
        let state = Rc::clone(&state);
        pause_btn.connect_clicked(move |btn| {
            let mut s = state.borrow_mut();
            match s.process.state {
                ProcessState::Running => {
                    s.process.pause();
                    btn.set_label("Resume");
                }
                ProcessState::Paused => {
                    s.process.resume();
                    btn.set_label("Pause");
                }
                _ => {}
            }
        });
    }

    // Stop button
    {
        let state = Rc::clone(&state);
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn_clone = stop_btn.clone();
        let message_list = message_list.clone();
        stop_btn_clone.connect_clicked(move |btn| {
            state.borrow_mut().process.stop();
            let info = agent_widgets::create_system_message("⏹ Stopped");
            message_list.append(&info);
            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_label("Pause");
            btn.set_sensitive(false);
        });
    }

    // Clear button
    {
        let state = Rc::clone(&state);
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn = stop_btn.clone();
        clear_btn.connect_clicked(move |_| {
            state.borrow_mut().process.stop();
            while let Some(child) = message_list.first_child() {
                message_list.remove(&child);
            }
            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_label("Pause");
            stop_btn.set_sensitive(false);
        });
    }

    (panel, input_view)
}

fn handle_event(
    state: &Rc<RefCell<PanelState>>,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
    send_btn: &gtk::Button,
    pause_btn: &gtk::Button,
    stop_btn: &gtk::Button,
    event: AgentEvent,
) {
    match event {
        AgentEvent::System { .. } => {
            // Session started, nothing visible needed
        }

        AgentEvent::StreamEvent { event: ev } => {
            handle_stream_event(state, message_list, scrolled, &ev);
        }

        AgentEvent::User {
            tool_use_result: Some(ref result),
            ref message,
            ..
        } => {
            // tool_use_id is in message.content[0].tool_use_id, not in tool_use_result
            let tool_id = message
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("tool_use_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut s = state.borrow_mut();
            if let Some(content_box) = s.pending_tools.remove(&tool_id) {
                let output = if !result.stdout.is_empty() {
                    &result.stdout
                } else if let Some(ref c) = result.content {
                    c.as_str()
                } else {
                    &result.stderr
                };
                agent_widgets::fill_tool_result(&content_box, output, result.is_error);
            }
            scroll_to_bottom(scrolled);
        }

        AgentEvent::Result {
            total_cost_usd,
            is_error,
            ..
        } => {
            let msg = if is_error {
                "✗ Error".to_string()
            } else {
                format!("✓ Done (${:.4})", total_cost_usd)
            };
            let info = agent_widgets::create_system_message(&msg);
            message_list.append(&info);

            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_label("Pause");
            stop_btn.set_sensitive(false);

            let mut s = state.borrow_mut();
            s.current_text_label = None;
            s.current_text.clear();
            s.process.state = ProcessState::Idle;

            scroll_to_bottom(scrolled);
        }

        _ => {}
    }
}

fn handle_stream_event(
    state: &Rc<RefCell<PanelState>>,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
    ev: &crate::agent_process::StreamEventData,
) {
    match ev.event_type.as_str() {
        "content_block_start" => {
            let mut s = state.borrow_mut();
            if let Some(ref cb) = ev.content_block {
                match cb.block_type.as_str() {
                    "text" => {
                        let (container, label) = agent_widgets::create_assistant_text();
                        message_list.append(&container);
                        s.current_text_label = Some(label);
                        s.current_text.clear();
                    }
                    "tool_use" => {
                        let name = cb.name.as_deref().unwrap_or("Tool");
                        s.current_tool_name = Some(name.to_string());
                        s.current_tool_input.clear();
                        s.current_tool_use_id = cb.id.clone();
                    }
                    _ => {}
                }
            }
        }

        "content_block_delta" => {
            let mut s = state.borrow_mut();
            if let Some(ref delta) = ev.delta {
                match delta.delta_type.as_deref() {
                    Some("text_delta") => {
                        if let Some(ref text) = delta.text {
                            s.current_text.push_str(text);
                            if let Some(ref label) = s.current_text_label {
                                agent_widgets::update_assistant_text(label, &s.current_text);
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(ref json) = delta.partial_json {
                            s.current_tool_input.push_str(json);
                        }
                    }
                    _ => {}
                }
            }
            drop(s);
            scroll_to_bottom(scrolled);
        }

        "content_block_stop" => {
            let mut s = state.borrow_mut();
            // If we were building a tool call, finalize it
            if let Some(tool_name) = s.current_tool_name.take() {
                let input_text = extract_tool_display(&tool_name, &s.current_tool_input);
                let (expander, content_box) =
                    agent_widgets::create_tool_call(&tool_name, &input_text);
                message_list.append(&expander);

                if let Some(id) = s.current_tool_use_id.take() {
                    s.pending_tools.insert(id, content_box);
                }
                s.current_tool_input.clear();
            }
            // Clear text tracking (block is done)
            s.current_text_label = None;
            s.current_text.clear();
            drop(s);
            scroll_to_bottom(scrolled);
        }

        _ => {}
    }
}

/// Extract a short display string from tool input JSON
fn extract_tool_display(tool_name: &str, json_str: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        // Common patterns
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
        if let Some(q) = val.get("pattern").and_then(|v| v.as_str()) {
            let path = val
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return format!("{q} {path}").trim().to_string();
        }
        if let Some(p) = val.get("file_path").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        if let Some(p) = val.get("pattern").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        // Fallback: first string value
        for (_k, v) in val.as_object().into_iter().flatten() {
            if let Some(s) = v.as_str() {
                if s.len() <= 80 {
                    return s.to_string();
                }
            }
        }
    }
    tool_name.to_string()
}

fn scroll_to_bottom(scrolled: &gtk::ScrolledWindow) {
    let sc = scrolled.clone();
    glib::idle_add_local_once(move || {
        let adj = sc.vadjustment();
        adj.set_value(adj.upper() - adj.page_size());
    });
}
