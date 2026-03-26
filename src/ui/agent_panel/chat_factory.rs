use crate::agent_widgets;
use crate::markdown;
use crate::models::chat::ChatMessage;

use super::{extract_file_path, extract_tool_display};

/// Convert a ChatMessage to an HTML string and inject it into the webview.
///
/// This replaces the old GTK widget factory; all rendering is now HTML-based.
pub(super) fn render_history_message(
    webview: &crate::chat_webview::ChatWebView,
    msg: &ChatMessage,
    is_dark: bool,
) {
    match msg {
        ChatMessage::User { text } => {
            webview.append_user_message(text, &[]);
        }
        ChatMessage::AssistantText { text } => {
            let html = markdown::md_to_html(text, is_dark);
            let id = webview.begin_stream();
            webview.finalize_stream(&id, &html);
        }
        ChatMessage::ToolCall {
            tool_name,
            tool_input,
            output,
            is_error,
        } => {
            let file_path = extract_file_path(tool_input);
            // When a file path is present it's shown as a clickable link,
            // so don't duplicate it in the hint text.
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

            // Mark as complete immediately (history entries are already done).
            webview.tool_complete(&tool_id, *is_error);

            if !output.trim().is_empty() {
                let output_html = agent_widgets::format_tool_output_html(
                    output, *is_error, tool_name, tool_input,
                );
                webview.tool_output(&tool_id, &output_html);
            }
        }
        ChatMessage::System { text } => {
            webview.append_system_message(text);
        }
    }
}
