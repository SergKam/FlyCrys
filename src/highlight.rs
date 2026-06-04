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
                let ext = syntax_key(file_path);

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
                let ext = syntax_key(file_path);
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

/// Generate HTML for a diff between old and new strings,
/// with syntax highlighting based on file extension.
pub fn diff_to_html(old_string: &str, new_string: &str, file_path: &str) -> String {
    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    SYNTAX_SET.with(|default_ss| {
        CUSTOM_SYNTAX_SET.with(|custom_ss| {
            THEME_SET.with(|ts| {
                let theme = &ts.themes[LIGHT_THEME];
                let ext = syntax_key(file_path);
                let (ss, syntax) = resolve_syntax(default_ss, custom_ss, ext);

                let mut out = String::new();

                // Old lines (removed) — red background
                {
                    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
                    for line in old_string.lines() {
                        out.push_str("<div class=\"diff-del\"><span style=\"color:#b31d28;font-weight:bold\">- </span>");
                        let line_nl = format!("{}\n", line);
                        if let Ok(regions) = h.highlight_line(&line_nl, ss) {
                            for (style, text) in regions {
                                let text = text.trim_end_matches('\n');
                                if text.is_empty() {
                                    continue;
                                }
                                let fg = style.foreground;
                                out.push_str(&format!(
                                    "<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>",
                                    fg.r,
                                    fg.g,
                                    fg.b,
                                    escape_html(text)
                                ));
                            }
                        } else {
                            out.push_str(&escape_html(line));
                        }
                        out.push_str("</div>");
                    }
                }

                // New lines (added) — green background
                {
                    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
                    for line in new_string.lines() {
                        out.push_str("<div class=\"diff-add\"><span style=\"color:#22863a;font-weight:bold\">+ </span>");
                        let line_nl = format!("{}\n", line);
                        if let Ok(regions) = h.highlight_line(&line_nl, ss) {
                            for (style, text) in regions {
                                let text = text.trim_end_matches('\n');
                                if text.is_empty() {
                                    continue;
                                }
                                let fg = style.foreground;
                                out.push_str(&format!(
                                    "<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>",
                                    fg.r,
                                    fg.g,
                                    fg.b,
                                    escape_html(text)
                                ));
                            }
                        } else {
                            out.push_str(&escape_html(line));
                        }
                        out.push_str("</div>");
                    }
                }

                out
            })
        })
    })
}

/// The syntect lookup key for a path: the file extension, or — for
/// extensionless files like `Makefile`/`Dockerfile` — the bare file name.
///
/// The file name is isolated first so directory dots (e.g. `/home/u.x/Makefile`)
/// can't corrupt the result, and a leading-dot name (`.bashrc`) is treated as a
/// whole name rather than an extension.
fn syntax_key(file_path: &str) -> &str {
    let name = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext,
        _ => name,
    }
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

/// Whether the source viewer should run the syntax highlighter for this file,
/// as opposed to displaying it as plain text.
///
/// Rather than maintain a hand-curated extension allow-list (which inevitably
/// drifts out of sync with the grammar set), we ask syntect directly: a file is
/// "highlightable" iff a real grammar resolves for its extension. Anything with
/// no known grammar falls back to plain `set_text`, which is correct and fast.
///
/// Binary files and oversized files never reach this check — they are rejected
/// earlier in `textview::load_file` (non-UTF-8 read failure and the 10 MB cap).
pub fn is_highlightable(file_path: &str) -> bool {
    let ext = syntax_key(file_path);
    if ext.is_empty() {
        return false;
    }
    SYNTAX_SET.with(|default_ss| {
        CUSTOM_SYNTAX_SET.with(|custom_ss| {
            let (_, syntax) = resolve_syntax(default_ss, custom_ss, ext);
            // resolve_syntax falls back to the "Plain Text" grammar when nothing
            // matches; that is precisely the case we want to skip.
            syntax.name != "Plain Text"
        })
    })
}

/// Whether the source viewer should highlight a file of `byte_len` bytes.
///
/// Combines grammar availability ([`is_highlightable`]) with a size ceiling
/// ([`HIGHLIGHT_MAX_BYTES`]): above the ceiling, per-line tagging causes UI
/// jank, so the file is shown as plain text even when a grammar exists.
pub fn should_highlight(file_path: &str, byte_len: usize) -> bool {
    use crate::config::constants::HIGHLIGHT_MAX_BYTES;
    byte_len <= HIGHLIGHT_MAX_BYTES && is_highlightable(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// syntect's bundled default set ships a PHP syntax, so resolution must land
    /// on the real "PHP" grammar (not the plain-text fallback) for both extensions.
    #[test]
    fn php_resolves_to_php_syntax() {
        let default_ss = SyntaxSet::load_defaults_newlines();
        static BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/custom_syntaxes.packdump"));
        let custom_ss: SyntaxSet =
            syntect::dumps::from_uncompressed_data(BYTES).expect("bad custom syntax packdump");

        for ext in ["php", "phtml"] {
            let (_, syntax) = resolve_syntax(&default_ss, &custom_ss, ext);
            assert_eq!(syntax.name, "PHP", "extension {ext} should resolve to PHP");
        }
    }

    /// syntect-driven detection highlights every language with a real grammar —
    /// including PHP and the extension variants the old hand-curated list missed
    /// (C++ `.cc`, Python `.pyw`, `.markdown`, capital-M `Makefile`).
    #[test]
    fn highlightable_covers_known_grammars() {
        for path in [
            "index.php",
            "template.phtml",
            "main.rs",
            "app.cc",     // C++ variant missed by the old list
            "script.pyw", // Python variant missed by the old list
            "README.markdown",
            "Makefile",
            "Service.scala",
            "lib.hs",
        ] {
            assert!(is_highlightable(path), "{path} should be highlightable");
        }
    }

    /// Files with no grammar (plain text, unknown extensions, extensionless)
    /// take the fast `set_text` path instead of the highlighter.
    #[test]
    fn unknown_and_plain_files_are_not_highlightable() {
        assert!(!is_highlightable("notes.txt"));
        assert!(!is_highlightable("data.unknownext"));
        assert!(!is_highlightable("LICENSE"));
    }

    /// A grammar-backed file is highlighted up to the size ceiling, and shown as
    /// plain text above it — even though the grammar still resolves.
    #[test]
    fn should_highlight_respects_size_ceiling() {
        use crate::config::constants::HIGHLIGHT_MAX_BYTES;
        assert!(should_highlight("main.rs", 0));
        assert!(should_highlight("main.rs", HIGHLIGHT_MAX_BYTES));
        assert!(!should_highlight("main.rs", HIGHLIGHT_MAX_BYTES + 1));
        // No grammar: never highlighted, regardless of size.
        assert!(!should_highlight("notes.txt", 10));
    }

    /// The grammars we bundle (because syntect ships none) must resolve.
    #[test]
    fn bundled_toml_and_dockerfile_resolve() {
        let default_ss = SyntaxSet::load_defaults_newlines();
        static BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/custom_syntaxes.packdump"));
        let custom_ss: SyntaxSet =
            syntect::dumps::from_uncompressed_data(BYTES).expect("bad custom syntax packdump");
        assert_eq!(
            resolve_syntax(&default_ss, &custom_ss, "toml").1.name,
            "TOML"
        );
        assert_eq!(
            resolve_syntax(&default_ss, &custom_ss, "Dockerfile").1.name,
            "Dockerfile"
        );
    }

    /// `syntax_key` isolates the file name, so extensionless files resolve even
    /// at real paths containing directory dots.
    #[test]
    fn syntax_key_isolates_filename() {
        assert_eq!(syntax_key("/home/u/main.rs"), "rs");
        assert_eq!(syntax_key("archive.tar.gz"), "gz");
        assert_eq!(syntax_key("/home/u.x/Makefile"), "Makefile");
        assert_eq!(syntax_key("Dockerfile"), "Dockerfile");
        assert_eq!(syntax_key(".bashrc"), ".bashrc");
    }

    /// Real-world paths to grammar-backed files — including the bundled TOML and
    /// extensionless Dockerfile/Makefile at dotted directories — are highlightable.
    #[test]
    fn full_paths_are_highlightable() {
        assert!(is_highlightable("/home/sergii/work/flycrys/Cargo.toml"));
        assert!(is_highlightable("/home/sergii/work/flycrys/Dockerfile"));
        assert!(is_highlightable("/home/u.dotted/project/Makefile"));
    }
}
