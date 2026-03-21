use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

use crate::agent_widgets;
use crate::models::chat::ChatMessage;
use crate::services::cli::AgentDomainEvent;

use super::state::{PanelState, ToolInfo};
use super::{format_token_count, scroll_to_bottom};

/// Handle a single domain event from the agent backend, updating UI and state.
pub(crate) fn handle_domain_event(
    state: &Rc<RefCell<PanelState>>,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
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
            remove_thinking_spinner(&mut s);
            // Create label on first delta if needed
            if s.chat.current_text_label.is_none() {
                let (container, label) = agent_widgets::create_assistant_text();
                let cb = s.on_open_file.clone();
                label.connect_activate_link(move |_label, uri| {
                    if let Some(path) = uri.strip_prefix("file://") {
                        cb(path);
                        gtk::glib::Propagation::Stop
                    } else {
                        gtk::glib::Propagation::Proceed
                    }
                });
                message_list.append(&container);
                s.chat.current_text_label = Some(label);
                s.chat.current_text.clear();
            }
            s.chat.current_text.push_str(&text);
            if let Some(ref label) = s.chat.current_text_label {
                agent_widgets::update_assistant_text(
                    label,
                    &s.chat.current_text,
                    s.config.theme.get().is_dark(),
                );
            }
            drop(s);
            scroll_to_bottom(scrolled);
        }

        AgentDomainEvent::TextBlockFinished { full_text } => {
            let mut s = state.borrow_mut();
            // Record in history (use full_text from backend which tracked the full block)
            if !full_text.is_empty() {
                s.chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::AssistantText { text: full_text });
            }
            s.chat.current_text_label = None;
            s.chat.current_text.clear();
            drop(s);
            scroll_to_bottom(scrolled);
        }

        AgentDomainEvent::ThinkingStarted => {
            // Thinking spinner is already shown when the message is sent;
            // nothing extra needed here.
        }

        AgentDomainEvent::ThinkingDelta(_text) => {
            // Currently not displayed in the UI — thinking content is hidden.
        }

        AgentDomainEvent::ThinkingFinished => {
            let mut s = state.borrow_mut();
            remove_thinking_spinner(&mut s);
        }

        AgentDomainEvent::ToolStarted { id: _, name: _ } => {
            // Tool widget is created when ToolInputFinished arrives
            // (after all input JSON has been accumulated).
            let mut s = state.borrow_mut();
            remove_thinking_spinner(&mut s);
        }

        AgentDomainEvent::ToolInputDelta(_json) => {
            // Input is being accumulated by the backend; nothing to do in UI.
        }

        AgentDomainEvent::ToolInputFinished {
            id,
            name,
            input_json,
        } => {
            let mut s = state.borrow_mut();
            let input_text = super::extract_tool_display(&name, &input_json);
            let file_path = super::extract_file_path(&input_json);
            let on_open = s.on_open_file.clone();
            let (container, content_box, spinner, expander) =
                agent_widgets::create_tool_call(&name, &input_text, file_path.as_deref(), on_open);
            message_list.append(&container);

            s.chat.pending_tools.insert(
                id,
                ToolInfo {
                    content_box,
                    spinner,
                    expander,
                    tool_name: name,
                    tool_input: input_json,
                },
            );
            // Clear text tracking since a tool block supersedes any text block
            s.chat.current_text_label = None;
            s.chat.current_text.clear();
            drop(s);
            scroll_to_bottom(scrolled);
        }

        AgentDomainEvent::ToolResult {
            id,
            output,
            is_error,
        } => {
            let mut s = state.borrow_mut();
            if let Some(info) = s.chat.pending_tools.remove(&id) {
                agent_widgets::fill_tool_result(
                    &info.content_box,
                    &info.spinner,
                    &info.expander,
                    &output,
                    is_error,
                    &info.tool_name,
                    &info.tool_input,
                );
                s.chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::ToolCall {
                        tool_name: info.tool_name.clone(),
                        tool_input: info.tool_input.clone(),
                        output,
                        is_error,
                    });
            }
            if let Some(ref cb) = s.on_tool_result {
                cb();
            }
            scroll_to_bottom(scrolled);
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
            // Update stored context_tokens with best available data
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

            // Only show a system message for errors
            if is_error {
                let msg = match message {
                    Some(ref detail) if !detail.is_empty() => {
                        format!("\u{2717} Error: {detail}")
                    }
                    _ => "\u{2717} Error (no details available)".to_string(),
                };
                let info = agent_widgets::create_system_message(&msg);
                message_list.append(&info);
            }

            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_icon_name("media-playback-pause-symbolic");
            pause_btn.set_tooltip_text(Some("Pause"));
            stop_btn.set_sensitive(false);

            let mut s = state.borrow_mut();
            remove_thinking_spinner(&mut s);
            s.tab_spinner.set_spinning(false);

            // Update cost display
            s.tokens.total_cost_usd = total_cost_usd;
            s.tokens
                .cost_label
                .set_text(&format!("${:.2}", total_cost_usd));

            // Update context window if a more accurate value arrived
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

            // Stop any tool spinners that never received a result
            let orphaned: Vec<ToolInfo> = s.chat.pending_tools.drain().map(|(_, v)| v).collect();
            for ti in &orphaned {
                ti.spinner.set_spinning(false);
                ti.spinner.set_visible(false);
            }
            {
                let mut hist = s.chat.chat_history.borrow_mut();
                for ti in orphaned {
                    hist.push(ChatMessage::ToolCall {
                        tool_name: ti.tool_name,
                        tool_input: ti.tool_input,
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
            s.chat.current_text_label = None;
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

            scroll_to_bottom(scrolled);
        }

        AgentDomainEvent::ProcessError(msg) => {
            let label = agent_widgets::create_system_message(&format!("\u{26a0} {msg}"));
            label.add_css_class("error-text");
            message_list.append(&label);
            scroll_to_bottom(scrolled);
        }
    }
}

/// Remove the thinking spinner from the message list if present.
pub(super) fn remove_thinking_spinner(state: &mut PanelState) {
    if let Some(spinner) = state.chat.thinking_spinner.take()
        && let Some(parent) = spinner.parent()
        && let Some(parent_box) = parent.downcast_ref::<gtk::Box>()
    {
        parent_box.remove(&spinner);
    }
}
