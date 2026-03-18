use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind};

/// Convert markdown text to Pango markup for GTK labels.
pub fn md_to_pango(md: &str) -> String {
    let opts = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(md, opts);

    let mut out = String::with_capacity(md.len() * 2);
    let mut in_code_block = false;
    let mut list_depth: u32 = 0;

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
                }
                Tag::Paragraph => {}
                Tag::Emphasis => out.push_str("<i>"),
                Tag::Strong => out.push_str("<b>"),
                Tag::Strikethrough => out.push_str("<s>"),
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    let _lang = match &kind {
                        CodeBlockKind::Fenced(lang) => lang.as_ref(),
                        CodeBlockKind::Indented => "",
                    };
                    out.push_str("<span font_family=\"monospace\" background=\"#2a2a2a\" foreground=\"#e0e0e0\">");
                }
                Tag::List(_) => {
                    list_depth += 1;
                }
                Tag::Item => {
                    let indent = "  ".repeat(list_depth.saturating_sub(1) as usize);
                    out.push_str(&format!("{indent}• "));
                }
                Tag::Link { dest_url, .. } => {
                    out.push_str(&format!(
                        "<span foreground=\"#4a90d9\" underline=\"single\"><i>[",
                    ));
                    // We'll close with the URL after the text
                    let _ = dest_url; // URL stored but not used in Pango (no clickable links)
                }
                Tag::BlockQuote(_) => {
                    out.push_str("<span foreground=\"#888888\">│ ");
                }
                _ => {}
            },

            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    out.push_str("</span>\n");
                }
                TagEnd::Paragraph => {
                    out.push('\n');
                }
                TagEnd::Emphasis => out.push_str("</i>"),
                TagEnd::Strong => out.push_str("</b>"),
                TagEnd::Strikethrough => out.push_str("</s>"),
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    out.push_str("</span>\n");
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                }
                TagEnd::Item => {
                    out.push('\n');
                }
                TagEnd::Link => {
                    out.push_str("]</i></span>");
                }
                TagEnd::BlockQuote(_) => {
                    out.push_str("</span>\n");
                }
                _ => {}
            },

            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&escape_pango(&text));
                } else {
                    out.push_str(&escape_pango(&text));
                }
            }

            Event::Code(code) => {
                out.push_str("<tt background=\"#e8e8e8\"> ");
                out.push_str(&escape_pango(&code));
                out.push_str(" </tt>");
            }

            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("\n─────────────────\n"),

            _ => {}
        }
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
