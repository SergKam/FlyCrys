use gtk::prelude::*;
use gtk4 as gtk;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::rc::Rc;

/// Shorthand for the optional file-open callback threaded through widget builders.
type OpenFileCb<'a> = Option<&'a Rc<dyn Fn(&str)>>;
/// Owned version of the above for `'static` closures (idle callbacks).
type OpenFileCbOwned = Option<Rc<dyn Fn(&str)>>;

// ─── Widget-per-block renderer (history, file preview, post-stream) ─────────

/// Threshold (in bytes) for splitting long markdown into deferred batches.
const DEFERRED_SPLIT_BYTES: usize = 3000;

/// Like [`md_to_widget_box`] but defers rendering of long documents.
///
/// The first ~3 KB of markdown is rendered immediately; the remainder is
/// built in an idle callback so the UI stays responsive.
pub fn md_to_widget_box_deferred(
    md: &str,
    is_dark: bool,
    on_open_file: OpenFileCb<'_>,
) -> gtk::Box {
    if md.len() <= DEFERRED_SPLIT_BYTES {
        return md_to_widget_box(md, is_dark, on_open_file);
    }

    // Split at a paragraph boundary (blank line) near the threshold.
    let split = md[..DEFERRED_SPLIT_BYTES]
        .rfind("\n\n")
        .map(|p| p + 2) // include the blank line in the first chunk
        .unwrap_or(DEFERRED_SPLIT_BYTES);
    let (first, rest) = md.split_at(split);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let first_box = md_to_widget_box(first, is_dark, on_open_file);
    root.append(&first_box);

    if !rest.trim().is_empty() {
        let rest_owned = rest.to_string();
        let root_clone = root.clone();
        let cb: OpenFileCbOwned = on_open_file.map(Rc::clone);
        gtk::glib::idle_add_local_once(move || {
            // Recursively defer if the remainder is still large.
            let rest_box = md_to_widget_box_deferred(&rest_owned, is_dark, cb.as_ref());
            root_clone.append(&rest_box);
        });
    }

    root
}

/// Build a vertical Box of properly styled widgets from markdown.
///
/// Each block (paragraph, heading, code, table, blockquote, list, rule)
/// becomes its own GTK widget with CSS spacing.  Inline formatting
/// (bold, italic, code, links) is rendered as Pango markup within
/// individual Labels.
pub fn md_to_widget_box(md: &str, is_dark: bool, on_open_file: OpenFileCb<'_>) -> gtk::Box {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(md, opts);

    let inline_bg = if is_dark { "#3a3a3a" } else { "#e8e8e8" };

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Container stack — push sub-boxes for blockquotes / list items.
    let mut containers: Vec<gtk::Box> = vec![root.clone()];
    // Inline Pango markup accumulated for the current paragraph / cell.
    let mut buf = String::new();
    // Code block accumulator.
    let mut code_buf = String::new();
    let mut code_lang = String::new();
    let mut in_code_block = false;
    // Heading level (set between Start/End Heading).
    let mut heading_level: Option<pulldown_cmark::HeadingLevel> = None;
    // List nesting.
    let mut list_stack: Vec<ListKind> = Vec::new();
    // Table state.
    let mut table: Option<TableBuilder> = None;

    for event in parser {
        match event {
            // ── Block-level open / close ────────────────────────────
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if table.is_some() {
                    // Inside a table cell paragraph — just add newline to buf
                    buf.push('\n');
                } else {
                    flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                }
            }

            Event::Start(Tag::Heading { level, .. }) => {
                heading_level = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_heading(
                    &mut buf,
                    heading_level.take(),
                    containers.last().unwrap(),
                    on_open_file,
                );
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                in_code_block = true;
                code_lang = match &kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                build_code_block(&code_buf, &code_lang, is_dark, containers.last().unwrap());
                code_buf.clear();
                code_lang.clear();
            }

            Event::Start(Tag::BlockQuote(_)) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                let bq = gtk::Box::new(gtk::Orientation::Vertical, 0);
                bq.add_css_class("md-blockquote");
                containers.last().unwrap().append(&bq);
                containers.push(bq);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                containers.pop();
            }

            // ── Lists ──────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                list_stack.push(match start {
                    Some(n) => ListKind::Ordered(n),
                    None => ListKind::Unordered,
                });
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                let bullet = match list_stack.last_mut() {
                    Some(ListKind::Ordered(n)) => {
                        let s = format!("{}.", n);
                        *n += 1;
                        s
                    }
                    _ => "•".to_string(),
                };
                let depth = list_stack.len().saturating_sub(1);
                let item_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                item_box.add_css_class("md-list-item");
                item_box.set_margin_start((depth as i32) * 16);

                let bullet_label = gtk::Label::new(Some(&bullet));
                bullet_label.add_css_class("md-list-bullet");
                bullet_label.set_valign(gtk::Align::Start);
                bullet_label.set_xalign(1.0);
                item_box.append(&bullet_label);

                let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                content.set_hexpand(true);
                item_box.append(&content);
                containers.last().unwrap().append(&item_box);
                containers.push(content);
            }
            Event::End(TagEnd::Item) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                containers.pop();
            }

            // ── Tables ─────────────────────────────────────────────
            Event::Start(Tag::Table(alignments)) => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                table = Some(TableBuilder::new(alignments));
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut t) = table {
                    t.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut t) = table {
                    t.in_head = false;
                    t.header = std::mem::take(&mut t.current_row);
                }
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut t) = table
                    && !t.in_head
                {
                    let row = std::mem::take(&mut t.current_row);
                    t.rows.push(row);
                }
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut t) = table {
                    let cell = std::mem::take(&mut buf);
                    t.current_row.push(cell);
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    build_table(&t, containers.last().unwrap(), on_open_file);
                }
            }

            // ── Inline formatting ──────────────────────────────────
            Event::Start(Tag::Strong) => buf.push_str("<b>"),
            Event::End(TagEnd::Strong) => buf.push_str("</b>"),
            Event::Start(Tag::Emphasis) => buf.push_str("<i>"),
            Event::End(TagEnd::Emphasis) => buf.push_str("</i>"),
            Event::Start(Tag::Strikethrough) => buf.push_str("<s>"),
            Event::End(TagEnd::Strikethrough) => buf.push_str("</s>"),

            Event::Start(Tag::Link { dest_url, .. }) => {
                buf.push_str("<span foreground=\"#4a90d9\" underline=\"single\"><i>[");
                let _ = dest_url;
            }
            Event::End(TagEnd::Link) => {
                buf.push_str("]</i></span>");
            }

            // ── Text content ───────────────────────────────────────
            Event::Text(text) => {
                if in_code_block {
                    code_buf.push_str(&text);
                } else {
                    buf.push_str(&linkify_file_paths(&text));
                }
            }

            Event::Code(code) => {
                if looks_like_file_path(&code) {
                    let clean = code.split(':').next().unwrap_or(&code);
                    buf.push_str(&format!(
                        "<a href=\"file://{}\"><span font_family=\"monospace\" background=\"{inline_bg}\"> {} </span></a>",
                        escape_pango(clean),
                        escape_pango(&code),
                    ));
                } else {
                    buf.push_str(&format!(
                        "<span font_family=\"monospace\" background=\"{inline_bg}\"> {} </span>",
                        escape_pango(&code),
                    ));
                }
            }

            Event::SoftBreak => buf.push(' '),
            Event::HardBreak => buf.push('\n'),

            Event::Rule => {
                flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                sep.set_margin_top(6);
                sep.set_margin_bottom(6);
                containers.last().unwrap().append(&sep);
            }

            _ => {}
        }
    }

    // Flush anything left over.
    flush_paragraph(&mut buf, containers.last().unwrap(), on_open_file);

    root
}

// ─── Widget builder helpers ─────────────────────────────────────────────────

/// Create a standard markdown label with wrapping, selection, and link handling.
fn make_md_label(on_open_file: OpenFileCb<'_>) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_natural_wrap_mode(gtk::NaturalWrapMode::None);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.set_use_markup(true);
    label.set_hexpand(true);
    if let Some(cb) = on_open_file {
        let cb = cb.clone();
        label.connect_activate_link(move |_label, uri| {
            if let Some(path) = uri.strip_prefix("file://") {
                cb(path);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    label
}

/// Set Pango markup on a label, falling back to plain text if invalid.
fn set_markup_safe(label: &gtk::Label, markup: &str) {
    if gtk::pango::parse_markup(markup, '\0').is_ok() {
        label.set_markup(markup);
    } else {
        // Strip all tags and show as plain text — only this one block is affected.
        label.set_text(&strip_pango_tags(markup));
    }
}

/// Rough tag stripper for fallback display.
fn strip_pango_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' && in_tag {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Flush the inline buffer as a paragraph label.
fn flush_paragraph(buf: &mut String, container: &gtk::Box, on_open_file: OpenFileCb<'_>) {
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        buf.clear();
        return;
    }
    let label = make_md_label(on_open_file);
    label.add_css_class("md-paragraph");
    set_markup_safe(&label, trimmed);
    container.append(&label);
    buf.clear();
}

/// Flush the inline buffer as a heading label.
fn flush_heading(
    buf: &mut String,
    level: Option<pulldown_cmark::HeadingLevel>,
    container: &gtk::Box,
    on_open_file: OpenFileCb<'_>,
) {
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        buf.clear();
        return;
    }
    let label = make_md_label(on_open_file);

    let (size_attr, css_class) = match level {
        Some(pulldown_cmark::HeadingLevel::H1) => ("x-large", "md-heading-1"),
        Some(pulldown_cmark::HeadingLevel::H2) => ("large", "md-heading-2"),
        _ => ("medium", "md-heading-3"),
    };
    label.add_css_class(css_class);

    let markup = format!("<span size=\"{size_attr}\" weight=\"bold\">{trimmed}</span>");
    set_markup_safe(&label, &markup);
    container.append(&label);
    buf.clear();
}

/// Build a code block widget — monospace label inside a horizontally-scrollable box.
fn build_code_block(code: &str, _lang: &str, _is_dark: bool, container: &gtk::Box) {
    let code_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    code_box.add_css_class("md-code-block");

    let trimmed = code.trim_end_matches('\n');

    let label = gtk::Label::new(None);
    label.add_css_class("md-code-label");
    // Don't wrap code — let it scroll horizontally for long lines.
    label.set_wrap(false);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.set_use_markup(true);

    let markup = format!(
        "<span font_family=\"monospace\">{}</span>",
        escape_pango(trimmed)
    );
    set_markup_safe(&label, &markup);

    // Horizontal scroll for wide code; don't propagate width to parent.
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroll.set_propagate_natural_width(false);
    scroll.set_vexpand(false);
    scroll.set_child(Some(&label));

    code_box.append(&scroll);
    container.append(&code_box);
}

/// Build a table from accumulated header + body rows.
fn build_table(t: &TableBuilder, container: &gtk::Box, on_open_file: OpenFileCb<'_>) {
    let grid = gtk::Grid::new();
    grid.set_column_spacing(0);
    grid.set_row_spacing(0);
    grid.set_margin_top(6);
    grid.set_margin_bottom(6);
    // Clip horizontally so wide tables don't inflate parent min-width.
    grid.set_overflow(gtk::Overflow::Hidden);

    // Header row
    for (col, markup) in t.header.iter().enumerate() {
        let cell = make_table_cell(markup, true, t.alignments.get(col), on_open_file);
        grid.attach(&cell, col as i32, 0, 1, 1);
    }

    // Body rows
    for (row_idx, row) in t.rows.iter().enumerate() {
        for (col, markup) in row.iter().enumerate() {
            let cell = make_table_cell(markup, false, t.alignments.get(col), on_open_file);
            grid.attach(&cell, col as i32, (row_idx + 1) as i32, 1, 1);
        }
    }

    container.append(&grid);
}

/// Build a single table cell widget.
fn make_table_cell(
    markup: &str,
    is_header: bool,
    alignment: Option<&Alignment>,
    on_open_file: OpenFileCb<'_>,
) -> gtk::Box {
    let cell_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cell_box.add_css_class(if is_header {
        "md-table-header-cell"
    } else {
        "md-table-cell"
    });
    cell_box.set_hexpand(true);

    let label = make_md_label(on_open_file);
    if is_header {
        let bold_markup = format!("<b>{}</b>", markup.trim());
        set_markup_safe(&label, &bold_markup);
    } else {
        set_markup_safe(&label, markup.trim());
    }

    match alignment {
        Some(Alignment::Center) => label.set_xalign(0.5),
        Some(Alignment::Right) => label.set_xalign(1.0),
        _ => label.set_xalign(0.0),
    }

    cell_box.append(&label);
    cell_box
}

// ─── Table builder state ────────────────────────────────────────────────────

enum ListKind {
    Unordered,
    Ordered(u64),
}

struct TableBuilder {
    alignments: Vec<Alignment>,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    in_head: bool,
}

impl TableBuilder {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
            header: Vec::new(),
            rows: Vec::new(),
            current_row: Vec::new(),
            in_head: false,
        }
    }
}

// ─── Streaming Pango renderer (single label, fast updates) ──────────────────

/// Convert markdown text to Pango markup for a single GTK label (streaming).
pub fn md_to_pango(md: &str, is_dark: bool) -> String {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
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
    let mut open_tags: Vec<&str> = Vec::new();
    // Table state for streaming pango.
    let mut _in_table = false;
    let mut in_table_head = false;
    let mut table_cell_idx: usize = 0;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let size = match level {
                        pulldown_cmark::HeadingLevel::H1 => "x-large",
                        pulldown_cmark::HeadingLevel::H2 => "large",
                        _ => "medium",
                    };
                    out.push_str(&format!("\n<span size=\"{size}\" weight=\"bold\">"));
                    open_tags.push("</span>");
                }
                Tag::Paragraph => {
                    // Add blank line between paragraphs for spacing in single-label mode.
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
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
                        "\n<span font_family=\"monospace\" background=\"{code_bg}\" foreground=\"{code_fg}\">"
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
                    let _ = dest_url;
                    open_tags.push("]</i></span>");
                }
                Tag::BlockQuote(_) => {
                    out.push_str("<span foreground=\"#888888\">│ ");
                    open_tags.push("</span>");
                }
                Tag::Table(_) => {
                    _in_table = true;
                    out.push('\n');
                }
                Tag::TableHead => {
                    in_table_head = true;
                    table_cell_idx = 0;
                }
                Tag::TableRow => {
                    table_cell_idx = 0;
                }
                Tag::TableCell => {
                    if table_cell_idx > 0 {
                        out.push_str(" │ ");
                    }
                    if in_table_head {
                        out.push_str("<b>");
                    }
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
                TagEnd::Table => {
                    _in_table = false;
                    out.push('\n');
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                    out.push_str("\n───────\n");
                }
                TagEnd::TableRow => {
                    out.push('\n');
                }
                TagEnd::TableCell => {
                    if in_table_head {
                        out.push_str("</b>");
                    }
                    table_cell_idx += 1;
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
                        "<span font_family=\"monospace\" background=\"{inline_bg}\"> {} </span>",
                        escape_pango(&code),
                    ));
                }
            }

            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("\n─────────────────\n"),

            _ => {}
        }
    }

    // Auto-close unclosed tags (handles incomplete streaming markdown).
    while let Some(close_tag) = open_tags.pop() {
        out.push_str(close_tag);
    }

    // Trim trailing newlines.
    while out.ends_with('\n') {
        out.pop();
    }

    out
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

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
