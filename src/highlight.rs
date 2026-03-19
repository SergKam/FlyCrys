use gtk::prelude::*;
use gtk4 as gtk;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

pub const LIGHT_THEME: &str = "InspiredGitHub";
pub const DARK_THEME: &str = "base16-eighties.dark";

/// Apply syntax highlighting to a TextBuffer with a specific theme.
pub fn highlight_buffer_with_theme(
    buffer: &gtk::TextBuffer,
    content: &str,
    file_path: &str,
    theme_name: &str,
) {
    buffer.set_text(content);

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes[theme_name];

    let ext = file_path.rsplit('.').next().unwrap_or("");
    let syntax = find_syntax(&ss, ext);

    let mut h = syntect::easy::HighlightLines::new(syntax, theme);

    let tag_table = buffer.tag_table();

    for (line_idx, line) in syntect::util::LinesWithEndings::from(content).enumerate() {
        let regions = match h.highlight_line(line, &ss) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let line_start = buffer.iter_at_line(line_idx as i32);
        let Some(offset_iter) = line_start else {
            continue;
        };
        let line_byte_start = offset_iter.offset();

        let mut byte_pos = 0;
        for (style, text) in regions {
            let start_offset = line_byte_start + byte_pos as i32;
            let end_offset = start_offset + text.len() as i32;
            byte_pos += text.len();

            let fg = style.foreground;
            let bold = style.font_style.contains(FontStyle::BOLD);
            let italic = style.font_style.contains(FontStyle::ITALIC);

            let tag_name = format!(
                "hl_{:02x}{:02x}{:02x}_{}{}",
                fg.r, fg.g, fg.b, bold as u8, italic as u8,
            );

            if tag_table.lookup(&tag_name).is_none()
                && let Some(tag) = buffer.create_tag(
                    Some(&tag_name),
                    &[(
                        "foreground",
                        &format!("#{:02x}{:02x}{:02x}", fg.r, fg.g, fg.b),
                    )],
                )
            {
                if bold {
                    tag.set_weight(700);
                }
                if italic {
                    tag.set_style(gtk::pango::Style::Italic);
                }
            }

            let start = buffer.iter_at_offset(start_offset);
            let end = buffer.iter_at_offset(end_offset);
            buffer.apply_tag_by_name(&tag_name, &start, &end);
        }
    }
}

fn find_syntax<'a>(ss: &'a SyntaxSet, ext: &str) -> &'a syntect::parsing::SyntaxReference {
    ss.find_syntax_by_extension(ext)
        .or_else(|| match ext {
            "mjs" | "cjs" => ss.find_syntax_by_extension("js"),
            "jsx" => ss.find_syntax_by_extension("js"),
            "tsx" => ss.find_syntax_by_extension("ts"),
            "yml" => ss.find_syntax_by_extension("yaml"),
            "mdx" => ss.find_syntax_by_extension("md"),
            "toml" => ss.find_syntax_by_extension("toml"),
            _ => None,
        })
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

/// Generate Pango markup for a diff between old and new strings,
/// with syntax highlighting based on file extension.
pub fn diff_to_pango(old_string: &str, new_string: &str, file_path: &str) -> String {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes[LIGHT_THEME];

    let ext = file_path.rsplit('.').next().unwrap_or("");
    let syntax = find_syntax(&ss, ext);

    let mut out = String::new();

    // Old lines (removed) — red background
    {
        let mut h = syntect::easy::HighlightLines::new(syntax, theme);
        for line in old_string.lines() {
            out.push_str("<span background=\"#ffeef0\">");
            out.push_str("<span foreground=\"#b31d28\" weight=\"bold\">- </span>");
            let line_nl = format!("{}\n", line);
            if let Ok(regions) = h.highlight_line(&line_nl, &ss) {
                for (style, text) in regions {
                    let text = text.trim_end_matches('\n');
                    if text.is_empty() {
                        continue;
                    }
                    let fg = style.foreground;
                    out.push_str(&format!(
                        "<span foreground=\"#{:02x}{:02x}{:02x}\">{}</span>",
                        fg.r,
                        fg.g,
                        fg.b,
                        escape_pango(text)
                    ));
                }
            } else {
                out.push_str(&escape_pango(line));
            }
            out.push_str("</span>\n");
        }
    }

    // New lines (added) — green background
    {
        let mut h = syntect::easy::HighlightLines::new(syntax, theme);
        for line in new_string.lines() {
            out.push_str("<span background=\"#e6ffed\">");
            out.push_str("<span foreground=\"#22863a\" weight=\"bold\">+ </span>");
            let line_nl = format!("{}\n", line);
            if let Ok(regions) = h.highlight_line(&line_nl, &ss) {
                for (style, text) in regions {
                    let text = text.trim_end_matches('\n');
                    if text.is_empty() {
                        continue;
                    }
                    let fg = style.foreground;
                    out.push_str(&format!(
                        "<span foreground=\"#{:02x}{:02x}{:02x}\">{}</span>",
                        fg.r,
                        fg.g,
                        fg.b,
                        escape_pango(text)
                    ));
                }
            } else {
                out.push_str(&escape_pango(line));
            }
            out.push_str("</span>\n");
        }
    }

    if out.ends_with('\n') {
        out.pop();
    }

    out
}

fn escape_pango(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Check if a file extension is likely a text/code file worth highlighting.
pub fn is_highlightable(file_path: &str) -> bool {
    let ext = file_path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "mjs"
            | "cjs"
            | "jsx"
            | "ts"
            | "tsx"
            | "json"
            | "css"
            | "scss"
            | "less"
            | "html"
            | "htm"
            | "xml"
            | "svg"
            | "yaml"
            | "yml"
            | "toml"
            | "rs"
            | "py"
            | "rb"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "sh"
            | "bash"
            | "zsh"
            | "md"
            | "mdx"
            | "sql"
            | "dockerfile"
            | "makefile"
            | "lua"
    )
}
