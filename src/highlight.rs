use gtk4 as gtk;
use gtk::prelude::*;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Apply syntax highlighting to a TextBuffer based on file extension.
pub fn highlight_buffer(buffer: &gtk::TextBuffer, content: &str, file_path: &str) {
    buffer.set_text(content);

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-eighties.dark"];

    let ext = file_path.rsplit('.').next().unwrap_or("");
    let syntax = ss
        .find_syntax_by_extension(ext)
        .or_else(|| match ext {
            "mjs" | "cjs" => ss.find_syntax_by_extension("js"),
            "jsx" => ss.find_syntax_by_extension("js"),
            "tsx" => ss.find_syntax_by_extension("ts"),
            "yml" => ss.find_syntax_by_extension("yaml"),
            "mdx" => ss.find_syntax_by_extension("md"),
            "toml" => ss.find_syntax_by_extension("toml"),
            _ => None,
        })
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut h = syntect::easy::HighlightLines::new(syntax, theme);

    // Tag table for caching
    let tag_table = buffer.tag_table();

    for (line_idx, line) in syntect::util::LinesWithEndings::from(content).enumerate() {
        let regions = match h.highlight_line(line, &ss) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let line_start = buffer.iter_at_line(line_idx as i32);
        let Some(mut offset_iter) = line_start else {
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

            if tag_table.lookup(&tag_name).is_none() {
                if let Some(tag) = buffer.create_tag(
                    Some(&tag_name),
                    &[
                        ("foreground", &format!("#{:02x}{:02x}{:02x}", fg.r, fg.g, fg.b)),
                    ],
                ) {
                    if bold {
                        tag.set_weight(700);
                    }
                    if italic {
                        tag.set_style(gtk::pango::Style::Italic);
                    }
                }
            }

            let start = buffer.iter_at_offset(start_offset);
            let end = buffer.iter_at_offset(end_offset);
            buffer.apply_tag_by_name(&tag_name, &start, &end);
        }
    }
}

/// Check if a file extension is likely a text/code file worth highlighting.
pub fn is_highlightable(file_path: &str) -> bool {
    let ext = file_path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx"
            | "json"
            | "css" | "scss" | "less"
            | "html" | "htm" | "xml" | "svg"
            | "yaml" | "yml" | "toml"
            | "rs"
            | "py" | "rb" | "go" | "java" | "c" | "h" | "cpp" | "hpp"
            | "sh" | "bash" | "zsh"
            | "md" | "mdx"
            | "sql"
            | "dockerfile"
            | "makefile"
            | "lua"
    )
}
