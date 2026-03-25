use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::agent_widgets;
use crate::chat_entry::ChatEntry;
use crate::models::chat::ChatMessage;
use crate::services::cli::AgentDomainEvent;

use super::chat_factory;
use super::state::PanelState;
use super::{
    extract_file_path, extract_tool_display, format_token_count, scroll_to_bottom,
    trim_chat_if_needed,
};

/// Handle a single domain event from the agent backend.
pub(crate) fn handle_domain_event(
    state: &Rc<RefCell<PanelState>>,
    send_btn: &gtk::Button,
    pause_btn: &gtk::Button,
    stop_btn: &gtk::Button,
    event: AgentDomainEvent,
) {
    match event {
        AgentDomainEvent::Started {
            session_id,
            model: _,
            context_window,
        } => {
            let mut s = state.borrow_mut();
            if let Some(ref id) = session_id {
                s.process.session_id = Some(id.clone());
                (s.on_session_id_change)(Some(id.clone()));
            }
            if let Some(ctx) = context_window {
                s.tokens.context_window_max = ctx;
            }
        }

        AgentDomainEvent::TextDelta(text) => {
            let mut s = state.borrow_mut();
            remove_thinking_entry(&mut s);
            if s.chat.current_streaming_entry.is_none() {
                let entry = ChatEntry::new_assistant_streaming();
                let widget = chat_factory::build_and_cache_widget(
                    &entry,
                    &s.on_open_file,
                    s.config.theme.get().is_dark(),
                );
                s.chat.chat_box.append(&widget);
                s.chat.current_streaming_entry = Some(entry);
                s.chat.current_text.clear();
                trim_chat_if_needed(&mut s.chat);
            }
            s.chat.current_text.push_str(&text);
            if let Some(ref entry) = s.chat.current_streaming_entry {
                entry.set_text(s.chat.current_text.as_str());
                if let Some(ref label) = entry.text_label() {
                    agent_widgets::update_assistant_text(
                        label,
                        &s.chat.current_text,
                        s.config.theme.get().is_dark(),
                    );
                }
            }
            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }

        AgentDomainEvent::TextBlockFinished { full_text } => {
            let mut s = state.borrow_mut();
            if let Some(ref entry) = s.chat.current_streaming_entry {
                entry.set_is_streaming(false);
                // Swap the streaming label for widget-per-block markdown.
                if !full_text.is_empty()
                    && let Some(widget) = entry.cached_widget()
                    && let Some(container) = widget.downcast_ref::<gtk::Box>()
                {
                    agent_widgets::finalize_assistant_text(
                        container,
                        &full_text,
                        s.config.theme.get().is_dark(),
                        &s.on_open_file,
                    );
                }
                entry.set_text_label(None);
            }
            if !full_text.is_empty() {
                s.chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::AssistantText { text: full_text });
            }
            s.chat.current_streaming_entry = None;
            s.chat.current_text.clear();
            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }

        AgentDomainEvent::ThinkingStarted => {}

        AgentDomainEvent::ThinkingDelta(_text) => {}

        AgentDomainEvent::ThinkingFinished => {
            let mut s = state.borrow_mut();
            remove_thinking_entry(&mut s);
        }

        AgentDomainEvent::ToolStarted { id: _, name: _ } => {
            let mut s = state.borrow_mut();
            remove_thinking_entry(&mut s);
        }

        AgentDomainEvent::ToolInputDelta(_json) => {}

        AgentDomainEvent::ToolInputFinished {
            id,
            name,
            input_json,
        } => {
            let mut s = state.borrow_mut();
            let input_text = extract_tool_display(&name, &input_json);
            let file_path = extract_file_path(&input_json).unwrap_or_default();

            let entry = ChatEntry::new_tool_call(&name, &input_json, &input_text, &file_path);
            let widget = chat_factory::build_and_cache_widget(
                &entry,
                &s.on_open_file,
                s.config.theme.get().is_dark(),
            );
            s.chat.chat_box.append(&widget);
            s.chat.pending_tools.insert(id, entry);

            s.chat.current_streaming_entry = None;
            s.chat.current_text.clear();
            trim_chat_if_needed(&mut s.chat);
            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }

        AgentDomainEvent::ToolResult {
            id,
            output,
            is_error,
        } => {
            let mut s = state.borrow_mut();
            if let Some(entry) = s.chat.pending_tools.remove(&id) {
                entry.set_tool_output(output.as_str());
                entry.set_tool_is_error(is_error);
                entry.set_tool_complete(true);

                if let Some(ref spinner) = entry.tool_spinner_widget()
                    && let Some(ref triangle) = entry.tool_triangle_widget()
                {
                    agent_widgets::mark_tool_complete(spinner, triangle, is_error);
                }

                if let Some(ref content_box) = entry.tool_content_box_widget() {
                    let output_clone = output.clone();
                    let tool_name = entry.tool_name();
                    let tool_input = entry.tool_input();
                    let cb = content_box.clone();
                    let rendered = Rc::new(Cell::new(false));
                    content_box.connect_map(move |_| {
                        if !rendered.get() {
                            rendered.set(true);
                            agent_widgets::render_tool_output(
                                &cb,
                                &output_clone,
                                is_error,
                                &tool_name,
                                &tool_input,
                            );
                        }
                    });
                }

                let tool_name = entry.tool_name();
                let tool_input = entry.tool_input();
                s.chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::ToolCall {
                        tool_name,
                        tool_input,
                        output,
                        is_error,
                    });
            }
            if let Some(ref cb) = s.on_tool_result {
                cb();
            }
            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }

        AgentDomainEvent::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
        } => {
            let mut s = state.borrow_mut();
            let total_input = input_tokens + cache_write_tokens + cache_read_tokens;
            let total = total_input + output_tokens;
            if total_input > 0 {
                s.tokens.context_tokens = total_input;
            }
            let display_total = if total > 0 { total } else { total_input };
            let pct = (display_total as f64 / s.tokens.context_window_max as f64 * 100.0) as u32;
            s.tokens.token_label.set_text(&format!(
                "{} ({}%)",
                format_token_count(display_total),
                pct
            ));
        }

        AgentDomainEvent::Finished {
            outcome,
            message,
            total_cost_usd,
            num_turns: _,
            context_window,
        } => {
            let is_error = outcome == crate::config::types::AgentOutcome::Error;

            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_icon_name("media-playback-pause-symbolic");
            pause_btn.set_tooltip_text(Some("Pause"));
            stop_btn.set_sensitive(false);

            let mut s = state.borrow_mut();
            remove_thinking_entry(&mut s);

            if is_error {
                let msg = match message {
                    Some(ref detail) if !detail.is_empty() => {
                        format!("\u{2717} Error: {detail}")
                    }
                    _ => "\u{2717} Error (no details available)".to_string(),
                };
                let entry = ChatEntry::new_system(&msg);
                let widget = chat_factory::build_and_cache_widget(
                    &entry,
                    &s.on_open_file,
                    s.config.theme.get().is_dark(),
                );
                s.chat.chat_box.append(&widget);
                trim_chat_if_needed(&mut s.chat);
            }
            s.tab_spinner.set_spinning(false);

            s.tokens.total_cost_usd = total_cost_usd;
            s.tokens
                .cost_label
                .set_text(&format!("${:.2}", total_cost_usd));

            if let Some(ctx) = context_window
                && ctx > 0
            {
                s.tokens.context_window_max = ctx;
                let pct = (s.tokens.context_tokens as f64 / ctx as f64 * 100.0) as u32;
                s.tokens.token_label.set_text(&format!(
                    "{} ({}%)",
                    format_token_count(s.tokens.context_tokens),
                    pct
                ));
            }

            // Finalize orphaned tools
            let orphaned: Vec<ChatEntry> = s.chat.pending_tools.drain().map(|(_, v)| v).collect();
            for entry in &orphaned {
                entry.set_tool_complete(true);
                if let Some(ref spinner) = entry.tool_spinner_widget() {
                    spinner.set_spinning(false);
                    spinner.set_visible(false);
                }
            }
            {
                let mut hist = s.chat.chat_history.borrow_mut();
                for entry in orphaned {
                    hist.push(ChatMessage::ToolCall {
                        tool_name: entry.tool_name(),
                        tool_input: entry.tool_input(),
                        output: String::new(),
                        is_error: false,
                    });
                }
                if is_error {
                    let err_msg = match message {
                        Some(ref detail) if !detail.is_empty() => {
                            format!("\u{2717} Error: {detail}")
                        }
                        _ => "\u{2717} Error (no details available)".to_string(),
                    };
                    hist.push(ChatMessage::System { text: err_msg });
                }
            }
            s.chat.current_streaming_entry = None;
            s.chat.current_text.clear();

            // Desktop notification
            if s.config.notification_level.get().is_enabled()
                && let Some(app) = gio::Application::default()
            {
                let dir_name = s
                    .process
                    .working_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let notification = gio::Notification::new("FlyCrys");
                if is_error {
                    notification.set_body(Some(&format!("Agent error in {dir_name}")));
                } else {
                    notification.set_body(Some(&format!("Agent finished in {dir_name}")));
                }
                app.send_notification(None, &notification);
            }

            if let Some(ref cb) = s.on_tool_result {
                cb();
            }

            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }

        AgentDomainEvent::ProcessError(msg) => {
            let mut s = state.borrow_mut();
            let entry = ChatEntry::new_system(&format!("\u{26a0} {msg}"));
            let widget = chat_factory::build_and_cache_widget(
                &entry,
                &s.on_open_file,
                s.config.theme.get().is_dark(),
            );
            s.chat.chat_box.append(&widget);
            trim_chat_if_needed(&mut s.chat);
            let scrolled = s.chat.scrolled.clone();
            drop(s);
            scroll_to_bottom(&scrolled);
        }
    }
}

/// Remove the thinking spinner widget from the chat_box if present.
pub(super) fn remove_thinking_entry(state: &mut PanelState) {
    if let Some(entry) = state.chat.thinking_entry.take()
        && let Some(widget) = entry.cached_widget()
    {
        state.chat.chat_box.remove(&widget);
    }
}
