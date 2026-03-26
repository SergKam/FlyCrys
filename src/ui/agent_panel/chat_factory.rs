use crate::agent_widgets;
use crate::markdown;
use crate::models::chat::ChatMessage;

use super::{extract_file_path, extract_tool_display};

/// Append a history message at the bottom of the chat (normal order).
pub(super) fn render_history_message(
    webview: &crate::chat_webview::ChatWebView,
    msg: &ChatMessage,
    is_dark: bool,
) {
    inject_message(webview, msg, is_dark, false);
}

/// Prepend a history message at the top of the chat (for "Load previous").
pub(super) fn render_history_message_prepend(
    webview: &crate::chat_webview::ChatWebView,
    msg: &ChatMessage,
    is_dark: bool,
) {
    inject_message(webview, msg, is_dark, true);
}

fn inject_message(
    webview: &crate::chat_webview::ChatWebView,
    msg: &ChatMessage,
    is_dark: bool,
    prepend: bool,
) {
    match msg {
        ChatMessage::User { text } => {
            if prepend {
                let escaped = markdown::escape_html(text);
                webview.prepend_html(&format!("<div class=\"msg user-msg\">{escaped}</div>"));
            } else {
                webview.append_user_message(text, &[]);
            }
        }
        ChatMessage::AssistantText { text } => {
            let html = markdown::md_to_html(text, is_dark);
            if prepend {
                webview.prepend_html(&format!("<div class=\"msg assistant-msg\">{html}</div>"));
            } else {
                let id = webview.begin_stream();
                webview.finalize_stream(&id, &html);
            }
        }
        ChatMessage::ToolCall {
            tool_name,
            tool_input,
            output,
            is_error,
        } => {
            let file_path = extract_file_path(tool_input);
            let display_hint = if file_path.is_some() {
                String::new()
            } else {
                extract_tool_display(tool_name, tool_input)
            };
            let full_cmd = agent_widgets::extract_full_command(tool_name, tool_input);

            let tool_id = format!("hist-{}", uuid::Uuid::new_v4());
            webview.append_tool_call(
                &tool_id,
                tool_name,
                &display_hint,
                &full_cmd,
                file_path.as_deref(),
            );

            webview.tool_complete(&tool_id, *is_error);

            if !output.trim().is_empty() {
                let output_html = agent_widgets::format_tool_output_html(
                    output, *is_error, tool_name, tool_input,
                );
                webview.tool_output(&tool_id, &output_html);
            }

            // For prepend, we used append_tool_call (which uses JS DOM append).
            // Move the element to the top of #chat.
            if prepend {
                webview.move_to_top(&tool_id);
            }
        }
        ChatMessage::System { text } => {
            if prepend {
                let escaped = markdown::escape_html(text);
                webview.prepend_html(&format!("<div class=\"msg system-msg\">{escaped}</div>"));
            } else {
                webview.append_system_message(text);
            }
        }
    }
}
