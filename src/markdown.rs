use pulldown_cmark::{Event, Options, Parser, html};

// ─── HTML markdown rendering ────────────────────────────────────────────────

/// Convert markdown to HTML with file-path linkification.
///
/// The `_is_dark` parameter is reserved for future use (syntax highlight theme
/// selection); it is accepted but currently ignored.
pub fn md_to_html(md: &str, _is_dark: bool) -> String {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(md, opts);

    // Transform text events to linkify file paths.
    let events = parser.map(|event| match event {
        Event::Text(text) => {
            let linkified = linkify_file_paths_html(&text);
            if linkified != text.as_ref() {
                Event::Html(linkified.into())
            } else {
                Event::Text(text)
            }
        }
        other => other,
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, events);
    html_output
}

/// Fast-path markdown-to-HTML for streaming updates.
///
/// Skips file-path linkification to keep per-keystroke cost low.
pub fn md_to_html_streaming(md: &str) -> String {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(md, opts);

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

// ─── HTML escaping ──────────────────────────────────────────────────────────

/// Standard HTML entity escaping.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── File-path detection & linkification ────────────────────────────────────

/// Check if a string looks like an absolute file path.
#[allow(dead_code)]
fn looks_like_file_path(s: &str) -> bool {
    s.starts_with('/')
        && s.len() > 3
        && !s.contains(' ')
        && !s.contains("//")
        && s.matches('/').count() >= 2
}

/// Scan plain text for absolute file paths and wrap them in HTML `<a>` tags.
///
/// Paths use the `flycrys://open-file?path=…` scheme so the application can
/// intercept them and open the file in an editor.
fn linkify_file_paths_html(text: &str) -> String {
    let mut result = String::new();
    let mut last_end = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/'
            && (i == 0
                || matches!(
                    bytes[i - 1],
                    b' ' | b'\n' | b'\t' | b'(' | b'[' | b'"' | b'\''
                ))
            && i + 1 < len
            && (bytes[i + 1].is_ascii_alphanumeric() || bytes[i + 1] == b'.')
        {
            let start = i;
            while i < len
                && (bytes[i].is_ascii_alphanumeric()
                    || matches!(bytes[i], b'/' | b'.' | b'_' | b'-'))
            {
                i += 1;
            }
            let path_end = i;

            // Optionally consume a trailing `:line` suffix for display.
            let mut display_end = path_end;
            if i < len && bytes[i] == b':' {
                let colon = i;
                i += 1;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > colon + 1 {
                    display_end = i;
                } else {
                    i = path_end;
                }
            }

            let path = &text[start..path_end];
            let display = &text[start..display_end];

            if path.matches('/').count() >= 2 && path.len() > 3 && !path.contains("//") {
                result.push_str(&escape_html(&text[last_end..start]));
                result.push_str(&format!(
                    "<a href=\"flycrys://open-file?path={}\">{}</a>",
                    escape_html(path),
                    escape_html(display),
                ));
                last_end = display_end;
                i = display_end;
                continue;
            }
        }
        i += 1;
    }

    if last_end == 0 {
        escape_html(text)
    } else {
        if last_end < text.len() {
            result.push_str(&escape_html(&text[last_end..]));
        }
        result
    }
}
