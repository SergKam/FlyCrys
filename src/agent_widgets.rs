use crate::config::constants::{OUTPUT_COLLAPSE_THRESHOLD, OUTPUT_HEAD_TAIL_LINES};

/// Extract the full human-readable command / input from tool_input JSON.
pub fn extract_full_command(tool_name: &str, tool_input: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(tool_input) {
        // Bash: show full command
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return format!("$ {cmd}");
        }
        // Read/Write: show file_path
        if let Some(fp) = val.get("file_path").and_then(|v| v.as_str()) {
            return format!("{tool_name}: {fp}");
        }
        // Grep/search: show pattern
        if let Some(pat) = val.get("pattern").and_then(|v| v.as_str()) {
            return format!("{tool_name}: {pat}");
        }
    }
    String::new()
}

/// Escape text for safe embedding in Pango markup.
pub fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Try to build an HTML diff from Edit tool input JSON.
pub fn create_edit_diff_html(tool_input: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(tool_input).ok()?;
    let old_string = val.get("old_string")?.as_str()?;
    let new_string = val.get("new_string")?.as_str()?;
    let file_path = val.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

    Some(crate::highlight::diff_to_html(
        old_string, new_string, file_path,
    ))
}

/// Format tool output for display in HTML, with truncation for long output.
pub fn format_tool_output_html(
    output: &str,
    is_error: bool,
    tool_name: &str,
    tool_input: &str,
) -> String {
    let mut html = String::new();

    // Show the full command / input at the top
    let full_cmd = extract_full_command(tool_name, tool_input);
    if !full_cmd.is_empty() {
        html.push_str(&format!(
            "<div class=\"full-cmd\">{}</div>",
            crate::markdown::escape_html(&full_cmd)
        ));
    }

    // No output — nothing more to render
    if output.trim().is_empty() {
        return html;
    }

    // For Edit tool, try to show a highlighted diff
    if tool_name == "Edit"
        && let Some(diff_html) = create_edit_diff_html(tool_input)
    {
        html.push_str(&diff_html);
        return html;
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
            output
                .char_indices()
                .nth(OUTPUT_COLLAPSE_THRESHOLD)
                .map_or_else(
                    || output.to_string(),
                    |(i, _)| format!("{}\u{2026}", &output[..i]),
                )
        }
    } else {
        output.to_string()
    };

    let class = if is_error {
        " class=\"error-output\""
    } else {
        ""
    };
    html.push_str(&format!(
        "<pre{class}>{}</pre>",
        crate::markdown::escape_html(&text)
    ));

    html
}
