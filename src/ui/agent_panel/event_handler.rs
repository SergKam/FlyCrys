use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

use crate::agent_widgets;
use crate::markdown;
use crate::models::chat::ChatMessage;
use crate::services::cli::AgentDomainEvent;

use super::state::PanelState;
use super::{extract_file_path, extract_tool_display, format_token_count};

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
            remove_thinking(&mut s);
            if !s.chat.current_streaming {
                let id = s.chat.webview.begin_stream();
                s.chat.current_stream_id = Some(id);
                s.chat.current_streaming = true;
                s.chat.current_text.clear();
            }
            s.chat.current_text.push_str(&text);
            let html = markdown::md_to_html_streaming(&s.chat.current_text);
            if let Some(ref id) = s.chat.current_stream_id {
                s.chat.webview.update_stream(id, &html);
            }
            s.chat.webview.scroll_to_bottom();
        }

        AgentDomainEvent::TextBlockFinished { full_text } => {
            let mut s = state.borrow_mut();
            if let Some(ref id) = s.chat.current_stream_id
                && !full_text.is_empty()
            {
                let html = markdown::md_to_html(&full_text, s.config.theme.get().is_dark());
                s.chat.webview.finalize_stream(id, &html);
            }
            if !full_text.is_empty() {
                s.chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::AssistantText { text: full_text });
            }
            s.chat.current_streaming = false;
            s.chat.current_stream_id = None;
            s.chat.current_text.clear();
            s.chat.webview.scroll_to_bottom();
        }

        AgentDomainEvent::ThinkingStarted => {}

        AgentDomainEvent::ThinkingDelta(_text) => {}

        AgentDomainEvent::ThinkingFinished => {
            let mut s = state.borrow_mut();
            remove_thinking(&mut s);
        }

        AgentDomainEvent::ToolStarted { id: _, name: _ } => {
            let mut s = state.borrow_mut();
            remove_thinking(&mut s);
        }

        AgentDomainEvent::ToolInputDelta(_json) => {}

        AgentDomainEvent::ToolInputFinished {
            id,
            name,
            input_json,
        } => {
            let mut s = state.borrow_mut();
            let input_text = extract_tool_display(&name, &input_json);
            let file_path = extract_file_path(&input_json);
            let full_cmd = agent_widgets::extract_full_command(&name, &input_json);

            s.chat.webview.append_tool_call(
                &id,
                &name,
                &input_text,
                &full_cmd,
                file_path.as_deref(),
            );
            s.chat.pending_tools.insert(id, (name, input_json));

            s.chat.current_streaming = false;
            s.chat.current_stream_id = None;
            s.chat.current_text.clear();
            s.chat.webview.scroll_to_bottom();
        }

        AgentDomainEvent::ToolResult {
            id,
            output,
            is_error,
        } => {
            let mut s = state.borrow_mut();
            if let Some((tool_name, tool_input)) = s.chat.pending_tools.remove(&id) {
                s.chat.webview.tool_complete(&id, is_error);

                if !output.trim().is_empty() {
                    let output_html = agent_widgets::format_tool_output_html(
                        &output,
                        is_error,
                        &tool_name,
                        &tool_input,
                    );
                    s.chat.webview.tool_output(&id, &output_html);
                }

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
            s.chat.webview.scroll_to_bottom();
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
            remove_thinking(&mut s);

            if is_error {
                let msg = match message {
                    Some(ref detail) if !detail.is_empty() => {
                        format!("\u{2717} Error: {detail}")
                    }
                    _ => "\u{2717} Error (no details available)".to_string(),
                };
                s.chat.webview.append_system_message(&msg);
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
            let orphaned: Vec<(String, String, String)> = s
                .chat
                .pending_tools
                .drain()
                .map(|(id, (name, input))| (id, name, input))
                .collect();
            for (id, _, _) in &orphaned {
                s.chat.webview.tool_complete(id, false);
            }
            {
                let mut hist = s.chat.chat_history.borrow_mut();
                for (_id, name, input) in orphaned {
                    hist.push(ChatMessage::ToolCall {
                        tool_name: name,
                        tool_input: input,
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
            s.chat.current_streaming = false;
            s.chat.current_stream_id = None;
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

            s.chat.webview.scroll_to_bottom();
        }

        AgentDomainEvent::ProcessError(msg) => {
            let s = state.borrow_mut();
            s.chat
                .webview
                .append_system_message(&format!("\u{26a0} {msg}"));
            s.chat.webview.scroll_to_bottom();
        }
    }
}

/// Remove the thinking indicator from the WebView if present.
pub(super) fn remove_thinking(state: &mut PanelState) {
    if let Some(id) = state.chat.thinking_id.take() {
        state.chat.webview.remove_element(&id);
    }
}
