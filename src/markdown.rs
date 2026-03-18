use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind};

/// Convert markdown text to Pango markup for GTK labels.
pub fn md_to_pango(md: &str, is_dark: bool) -> String {
    let opts = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, opts);

    let (code_bg, code_fg) = if is_dark {
        ("#2a2a2a", "#e0e0e0")
    } else {
        ("#f5f5f5", "#333333")
    };
    let inline_bg = if is_dark { "#3a3a3a" } else { "#e8e8e8" };

    let mut out = String::with_capacity(md.len() * 2);
    let mut in_code_block = false;
    let mut list_depth: u32 = 0;
    // Track open Pango tags so we can auto-close them for incomplete streaming markdown
    let mut open_tags: Vec<&str> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let size = match level {
                        pulldown_cmark::HeadingLevel::H1 => "x-large",
                        pulldown_cmark::HeadingLevel::H2 => "large",
                        _ => "medium",
                    };
                    out.push_str(&format!("<span size=\"{size}\" weight=\"bold\">"));
                    open_tags.push("</span>");
                }
                Tag::Paragraph => {}
                Tag::Emphasis => {
                    out.push_str("<i>");
                    open_tags.push("</i>");
                }
                Tag::Strong => {
                    out.push_str("<b>");
                    open_tags.push("</b>");
                }
                Tag::Strikethrough => {
                    out.push_str("<s>");
                    open_tags.push("</s>");
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    let _lang = match &kind {
                        CodeBlockKind::Fenced(lang) => lang.as_ref(),
                        CodeBlockKind::Indented => "",
                    };
                    out.push_str(&format!(
                        "<span font_family=\"monospace\" background=\"{code_bg}\" foreground=\"{code_fg}\">"
                    ));
                    open_tags.push("</span>");
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    let indent = "  ".repeat(list_depth.saturating_sub(1) as usize);
                    out.push_str(&format!("{indent}• "));
                }
                Tag::Link { dest_url, .. } => {
                    out.push_str("<span foreground=\"#4a90d9\" underline=\"single\"><i>[");
                    // We'll close with the URL after the text
                    let _ = dest_url; // URL stored but not used in Pango (no clickable links)
                    open_tags.push("]</i></span>");
                }
                Tag::BlockQuote(_) => {
                    out.push_str("<span foreground=\"#888888\">│ ");
                    open_tags.push("</span>");
                }
                _ => {}
            },

            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    open_tags.pop();
                    out.push_str("</span>\n");
                }
                TagEnd::Paragraph => {
                    out.push('\n');
                }
                TagEnd::Emphasis => {
                    open_tags.pop();
                    out.push_str("</i>");
                }
                TagEnd::Strong => {
                    open_tags.pop();
                    out.push_str("</b>");
                }
                TagEnd::Strikethrough => {
                    open_tags.pop();
                    out.push_str("</s>");
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    open_tags.pop();
                    out.push_str("</span>\n");
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                }
                TagEnd::Item => {
                    out.push('\n');
                }
                TagEnd::Link => {
                    open_tags.pop();
                    out.push_str("]</i></span>");
                }
                TagEnd::BlockQuote(_) => {
                    open_tags.pop();
                    out.push_str("</span>\n");
                }
                _ => {}
            },

            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&escape_pango(&text));
                } else {
                    out.push_str(&linkify_file_paths(&text));
                }
            }

            Event::Code(code) => {
                if looks_like_file_path(&code) {
                    let clean = code.split(':').next().unwrap_or(&code);
                    out.push_str(&format!(
                        "<a href=\"file://{}\"><span font_family=\"monospace\" background=\"{inline_bg}\"> {} </span></a>",
                        escape_pango(clean),
                        escape_pango(&code),
                    ));
                } else {
                    out.push_str(&format!(
                        "<span font_family=\"monospace\" background=\"{inline_bg}\"> "
                    ));
                    out.push_str(&escape_pango(&code));
                    out.push_str(" </span>");
                }
            }

            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("\n─────────────────\n"),

            _ => {}
        }
    }

    // Auto-close any unclosed tags (handles incomplete markdown during streaming)
    while let Some(close_tag) = open_tags.pop() {
        out.push_str(close_tag);
    }

    // Trim trailing newlines
    while out.ends_with('\n') {
        out.pop();
    }

    out
}

fn escape_pango(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Check if a string looks like an absolute file path
fn looks_like_file_path(s: &str) -> bool {
    s.starts_with('/')
        && s.len() > 3
        && !s.contains(' ')
        && !s.contains("//")
        && s.matches('/').count() >= 2
}

/// Escape text for Pango and wrap absolute file paths in clickable <a> tags
fn linkify_file_paths(text: &str) -> String {
    let mut result = String::new();
    let mut last_end = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for start of absolute path: '/' preceded by whitespace/delimiters or at start
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
            // Scan forward for valid path characters
            while i < len
                && (bytes[i].is_ascii_alphanumeric()
                    || matches!(bytes[i], b'/' | b'.' | b'_' | b'-'))
            {
                i += 1;
            }
            let path_end = i;

            // Optional :line_number suffix
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

            // Valid path: at least 2 segments, no double slashes
            if path.matches('/').count() >= 2 && path.len() > 3 && !path.contains("//") {
                result.push_str(&escape_pango(&text[last_end..start]));
                result.push_str(&format!(
                    "<a href=\"file://{}\">{}</a>",
                    escape_pango(path),
                    escape_pango(display),
                ));
                last_end = display_end;
                i = display_end;
                continue;
            }
        }
        i += 1;
    }

    if last_end == 0 {
        escape_pango(text)
    } else {
        if last_end < text.len() {
            result.push_str(&escape_pango(&text[last_end..]));
        }
        result
    }
}
