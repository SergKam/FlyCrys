mod common;
use common::md_to_html_light;

// ──────────────────────────────────────────────────────────────────────
// Markdown -> HTML: basic
// ──────────────────────────────────────────────────────────────────────

#[test]
fn md_plain_text() {
    let out = md_to_html_light("hello world");
    assert!(out.contains("hello world"), "got: {out}");
}

#[test]
fn md_escapes_special_chars() {
    let out = md_to_html_light("a < b & c > d");
    assert!(out.contains("&lt;"), "should escape <: {out}");
    assert!(out.contains("&amp;"), "should escape &: {out}");
    assert!(out.contains("&gt;"), "should escape >: {out}");
}

#[test]
fn md_bold() {
    let out = md_to_html_light("**bold**");
    assert!(out.contains("<strong>bold</strong>"), "got: {out}");
}

#[test]
fn md_italic() {
    let out = md_to_html_light("*italic*");
    assert!(out.contains("<em>italic</em>"), "got: {out}");
}

#[test]
fn md_strikethrough() {
    let out = md_to_html_light("~~struck~~");
    assert!(out.contains("<del>struck</del>"), "got: {out}");
}

#[test]
fn md_heading_h1() {
    let out = md_to_html_light("# Title");
    assert!(out.contains("<h1>"), "H1 tag: {out}");
    assert!(out.contains("Title"));
}

#[test]
fn md_heading_h2() {
    let out = md_to_html_light("## Subtitle");
    assert!(out.contains("<h2>"), "H2 tag: {out}");
    assert!(out.contains("Subtitle"));
}

#[test]
fn md_heading_h3() {
    let out = md_to_html_light("### Section");
    assert!(out.contains("<h3>"), "H3 tag: {out}");
    assert!(out.contains("Section"));
}

#[test]
fn md_inline_code() {
    let out = md_to_html_light("use `foo_bar` here");
    assert!(out.contains("<code>foo_bar</code>"), "got: {out}");
}

#[test]
fn md_inline_code_with_special_chars() {
    let out = md_to_html_light("run `x < y && z > w`");
    assert!(out.contains("&lt;"), "should escape < in code: {out}");
    assert!(out.contains("&amp;&amp;"), "should escape & in code: {out}");
}

#[test]
fn md_code_block() {
    let out = md_to_html_light("```rust\nfn main() {}\n```");
    assert!(out.contains("<code"), "code block present: {out}");
    assert!(out.contains("fn main()"));
}

#[test]
fn md_code_block_escapes_html() {
    let out = md_to_html_light("```\n<div>&</div>\n```");
    assert!(
        out.contains("&lt;div&gt;"),
        "code block should escape HTML: {out}"
    );
    assert!(out.contains("&amp;"));
}

#[test]
fn md_unordered_list() {
    let out = md_to_html_light("- one\n- two\n- three");
    assert!(out.contains("<ul>"), "should have ul: {out}");
    assert_eq!(out.matches("<li>").count(), 3, "should have 3 items: {out}");
}

#[test]
fn md_nested_list() {
    let out = md_to_html_light("- outer\n  - inner");
    assert!(
        out.matches("<li>").count() >= 2,
        "nested list should have items: {out}"
    );
}

#[test]
fn md_link() {
    let out = md_to_html_light("[click](https://example.com)");
    assert!(
        out.contains("<a href=\"https://example.com\">"),
        "link should use <a> tag: {out}"
    );
    assert!(out.contains("click"), "link text present: {out}");
    assert!(out.contains("</a>"), "link should close </a>: {out}");
}

#[test]
fn md_blockquote() {
    let out = md_to_html_light("> quoted text");
    assert!(out.contains("<blockquote>"), "blockquote tag: {out}");
    assert!(out.contains("quoted text"));
}

#[test]
fn md_horizontal_rule() {
    let out = md_to_html_light("above\n\n---\n\nbelow");
    assert!(out.contains("<hr"), "horizontal rule: {out}");
}

#[test]
fn md_bold_italic_combined() {
    let out = md_to_html_light("***bold italic***");
    assert!(
        out.contains("<strong>") || out.contains("<em>"),
        "combined formatting: {out}"
    );
}

#[test]
fn md_file_path_in_text() {
    let out = md_to_html_light("edit /home/user/src/main.rs now");
    assert!(
        out.contains("flycrys://open-file"),
        "inline file paths should link: {out}"
    );
}

#[test]
fn md_empty_input() {
    let out = md_to_html_light("");
    assert!(out.is_empty() || out.trim().is_empty(), "got: {out}");
}

#[test]
fn md_multiple_paragraphs() {
    let out = md_to_html_light("first\n\nsecond");
    assert!(out.contains("first"));
    assert!(out.contains("second"));
    assert!(out.contains("<p>"), "paragraphs: {out}");
}

// ──────────────────────────────────────────────────────────────────────
// Markdown: stress & edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn md_deeply_nested_formatting() {
    let out = md_to_html_light("**bold *bold-italic* bold**");
    assert!(out.contains("<strong>"), "outer bold: {out}");
    assert!(out.contains("<em>"), "inner italic: {out}");
}

#[test]
fn md_multiple_code_blocks_with_different_langs() {
    let md = "```rust\nfn main() {}\n```\n\nText between.\n\n```python\nprint('hi')\n```";
    let out = md_to_html_light(md);
    assert!(out.matches("<code").count() >= 2, "two code blocks: {out}");
    assert!(out.contains("fn main()"));
    assert!(out.contains("print("));
    assert!(out.contains("Text between"));
}

#[test]
fn md_list_then_code_block() {
    let md = "- item one\n- item two\n\n```\ncode here\n```";
    let out = md_to_html_light(md);
    assert_eq!(out.matches("<li>").count(), 2, "two items: {out}");
    assert!(out.contains("<code"), "code block: {out}");
    assert!(out.contains("code here"));
}

#[test]
fn md_heading_then_list_then_paragraph() {
    let md = "# Title\n\n- a\n- b\n\nParagraph text.";
    let out = md_to_html_light(md);
    assert!(out.contains("<h1>"), "h1 tag: {out}");
    assert_eq!(out.matches("<li>").count(), 2);
    assert!(out.contains("Paragraph text"));
}

#[test]
fn md_special_chars_everywhere() {
    let md = "# Heading & \"quoted\"\n\n`code <with> &`\n\n- item & stuff\n\n> blockquote & more";
    let out = md_to_html_light(md);
    assert!(out.contains("&amp;"), "ampersand escaped: {out}");
    assert!(
        out.contains("&lt;with&gt;"),
        "angle brackets in code escaped: {out}"
    );
    assert!(out.contains("Heading"), "heading text: {out}");
}

#[test]
fn md_many_file_paths_in_one_paragraph() {
    let md = "Check /home/user/src/a.rs and /home/user/src/b.rs and /opt/app/main.go for details.";
    let out = md_to_html_light(md);
    assert_eq!(
        out.matches("flycrys://open-file").count(),
        3,
        "three file links: {out}"
    );
}

#[test]
fn md_consecutive_blockquotes() {
    let out = md_to_html_light("> line one\n> line two");
    assert!(
        out.contains("<blockquote>"),
        "blockquote tag present: {out}"
    );
    assert!(out.contains("line one"));
    assert!(out.contains("line two"));
}

#[test]
fn md_table() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |";
    let out = md_to_html_light(md);
    assert!(out.contains("<table>"), "table present: {out}");
    assert!(out.contains("<th>"), "table header: {out}");
    assert!(out.contains("<td>"), "table data: {out}");
}

#[test]
fn md_streaming_produces_valid_html() {
    let out = flycrys::markdown::md_to_html_streaming("**bold** and *italic*");
    assert!(out.contains("<strong>bold</strong>"));
    assert!(out.contains("<em>italic</em>"));
}

#[test]
fn md_escape_html() {
    let out = flycrys::markdown::escape_html("<div>&\"test\"</div>");
    assert_eq!(out, "&lt;div&gt;&amp;&quot;test&quot;&lt;/div&gt;");
}
