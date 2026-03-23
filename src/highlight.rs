use gtk::prelude::*;
use gtk4 as gtk;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

pub const LIGHT_THEME: &str = "InspiredGitHub";
pub const DARK_THEME: &str = "base16-eighties.dark";

thread_local! {
    static SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    /// Custom syntaxes (TypeScript, TSX, …) pre-compiled by build.rs.
    static CUSTOM_SYNTAX_SET: SyntaxSet = {
        static BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/custom_syntaxes.packdump"));
        syntect::dumps::from_uncompressed_data(BYTES).expect("bad custom syntax packdump")
    };
    static THEME_SET: ThemeSet = ThemeSet::load_defaults();
}

/// Apply syntax highlighting to a TextBuffer with a specific theme.
pub fn highlight_buffer_with_theme(
    buffer: &gtk::TextBuffer,
    content: &str,
    file_path: &str,
    theme_name: &str,
) {
    buffer.set_text(content);

    SYNTAX_SET.with(|default_ss| {
        CUSTOM_SYNTAX_SET.with(|custom_ss| {
            THEME_SET.with(|ts| {
                let theme = &ts.themes[theme_name];
                let ext = file_path.rsplit('.').next().unwrap_or("");

                let (ss, syntax) = resolve_syntax(default_ss, custom_ss, ext);
                let mut h = syntect::easy::HighlightLines::new(syntax, theme);
                let tag_table = buffer.tag_table();

                for (line_idx, line) in syntect::util::LinesWithEndings::from(content).enumerate() {
                    let regions = match h.highlight_line(line, ss) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    let Some(offset_iter) = buffer.iter_at_line(line_idx as i32) else {
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
            })
        })
    })
}

/// Generate Pango markup for a diff between old and new strings,
/// with syntax highlighting based on file extension.
pub fn diff_to_pango(old_string: &str, new_string: &str, file_path: &str) -> String {
    SYNTAX_SET.with(|default_ss| {
        CUSTOM_SYNTAX_SET.with(|custom_ss| {
            THEME_SET.with(|ts| {
                let theme = &ts.themes[LIGHT_THEME];
                let ext = file_path.rsplit('.').next().unwrap_or("");
                let (ss, syntax) = resolve_syntax(default_ss, custom_ss, ext);

                let mut out = String::new();

                // Old lines (removed) — red background
                {
                    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
                    for line in old_string.lines() {
                        out.push_str("<span background=\"#ffeef0\">");
                        out.push_str("<span foreground=\"#b31d28\" weight=\"bold\">- </span>");
                        let line_nl = format!("{}\n", line);
                        if let Ok(regions) = h.highlight_line(&line_nl, ss) {
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
                        if let Ok(regions) = h.highlight_line(&line_nl, ss) {
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
            })
        })
    })
}

/// Resolve an extension to a (SyntaxSet, SyntaxReference) pair.
///
/// Checks the default built-in set first, then the custom set (TypeScript, etc.),
/// then falls back through `SYNTAX_ALIASES`, and finally to plain text.
fn resolve_syntax<'a>(
    default_ss: &'a SyntaxSet,
    custom_ss: &'a SyntaxSet,
    ext: &str,
) -> (&'a SyntaxSet, &'a SyntaxReference) {
    use crate::config::constants::SYNTAX_ALIASES;

    // Direct extension match — default set first, then custom.
    if let Some(s) = default_ss.find_syntax_by_extension(ext) {
        return (default_ss, s);
    }
    if let Some(s) = custom_ss.find_syntax_by_extension(ext) {
        return (custom_ss, s);
    }

    // Alias lookup — same order.
    if let Some((_, canonical)) = SYNTAX_ALIASES.iter().find(|(e, _)| *e == ext) {
        if let Some(s) = default_ss.find_syntax_by_extension(canonical) {
            return (default_ss, s);
        }
        if let Some(s) = custom_ss.find_syntax_by_extension(canonical) {
            return (custom_ss, s);
        }
    }

    (default_ss, default_ss.find_syntax_plain_text())
}

fn escape_pango(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Check if a file extension is likely a text/code file worth highlighting.
pub fn is_highlightable(file_path: &str) -> bool {
    use crate::config::constants::HIGHLIGHTABLE_EXTENSIONS;

    let ext = file_path.rsplit('.').next().unwrap_or("");
    HIGHLIGHTABLE_EXTENSIONS.contains(&ext)
}
