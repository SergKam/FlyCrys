mod common;
use common::md_to_pango_light;

// ──────────────────────────────────────────────────────────────────────
// Markdown → Pango markup: basic
// ──────────────────────────────────────────────────────────────────────

#[test]
fn md_plain_text() {
    let out = md_to_pango_light("hello world");
    assert_eq!(out, "hello world");
}

#[test]
fn md_escapes_special_chars() {
    let out = md_to_pango_light("a < b & c > d");
    assert!(out.contains("&lt;"), "should escape <");
    assert!(out.contains("&amp;"), "should escape &");
    assert!(out.contains("&gt;"), "should escape >");
}

#[test]
fn md_bold() {
    let out = md_to_pango_light("**bold**");
    assert!(out.contains("<b>bold</b>"), "got: {out}");
}

#[test]
fn md_italic() {
    let out = md_to_pango_light("*italic*");
    assert!(out.contains("<i>italic</i>"), "got: {out}");
}

#[test]
fn md_strikethrough() {
    let out = md_to_pango_light("~~struck~~");
    assert!(out.contains("<s>struck</s>"), "got: {out}");
}

#[test]
fn md_heading_h1() {
    let out = md_to_pango_light("# Title");
    assert!(out.contains("x-large"), "H1 should be x-large: {out}");
    assert!(out.contains("bold"), "H1 should be bold: {out}");
    assert!(out.contains("Title"));
}

#[test]
fn md_heading_h2() {
    let out = md_to_pango_light("## Subtitle");
    assert!(out.contains("large"), "H2 should be large: {out}");
    assert!(out.contains("Subtitle"));
}

#[test]
fn md_heading_h3() {
    let out = md_to_pango_light("### Section");
    assert!(out.contains("medium"), "H3 should be medium: {out}");
    assert!(out.contains("Section"));
}

#[test]
fn md_inline_code() {
    let out = md_to_pango_light("use `foo_bar` here");
    assert!(out.contains("font_family=\"monospace\""), "inline code uses monospace span: {out}");
    assert!(out.contains("foo_bar"));
}

#[test]
fn md_inline_code_with_special_chars() {
    let out = md_to_pango_light("run `x < y && z > w`");
    assert!(out.contains("&lt;"), "should escape < in code: {out}");
    assert!(out.contains("&amp;&amp;"), "should escape & in code: {out}");
}

#[test]
fn md_code_block() {
    let out = md_to_pango_light("```rust\nfn main() {}\n```");
    assert!(out.contains("monospace"), "code block should use monospace: {out}");
    assert!(out.contains("fn main()"));
}

#[test]
fn md_code_block_escapes_html() {
    let out = md_to_pango_light("```\n<div>&</div>\n```");
    assert!(out.contains("&lt;div&gt;"), "code block should escape HTML: {out}");
    assert!(out.contains("&amp;"));
}

#[test]
fn md_unordered_list() {
    let out = md_to_pango_light("- one\n- two\n- three");
    assert_eq!(out.matches('•').count(), 3, "should have 3 bullets: {out}");
}

#[test]
fn md_nested_list() {
    let out = md_to_pango_light("- outer\n  - inner");
    assert!(out.matches('•').count() >= 2, "nested list should have bullets: {out}");
}

#[test]
fn md_link() {
    let out = md_to_pango_light("[click](https://example.com)");
    assert!(out.contains("underline"), "link should be underlined: {out}");
    assert!(out.contains("click"), "link text present: {out}");
}

#[test]
fn md_blockquote() {
    let out = md_to_pango_light("> quoted text");
    assert!(out.contains("│"), "blockquote should have │ prefix: {out}");
    assert!(out.contains("quoted text"));
}

#[test]
fn md_horizontal_rule() {
    let out = md_to_pango_light("above\n\n---\n\nbelow");
    assert!(out.contains("─────"), "horizontal rule: {out}");
}

#[test]
fn md_bold_italic_combined() {
    let out = md_to_pango_light("***bold italic***");
    assert!(out.contains("<b>") || out.contains("<i>"), "combined formatting: {out}");
}

#[test]
fn md_file_path_in_inline_code() {
    let out = md_to_pango_light("see `/home/user/file.rs`");
    assert!(out.contains("file://"), "file path should be linked: {out}");
    assert!(out.contains("/home/user/file.rs"));
}

#[test]
fn md_file_path_with_line_number() {
    let out = md_to_pango_light("see `/home/user/file.rs:42`");
    assert!(out.contains("file://"), "file path should be linked: {out}");
    assert!(out.contains(":42"), "line number preserved: {out}");
}

#[test]
fn md_non_file_path_in_code() {
    let out = md_to_pango_light("use `HashMap`");
    assert!(!out.contains("file://"), "non-path code shouldn't link: {out}");
}

#[test]
fn md_file_path_in_text() {
    let out = md_to_pango_light("edit /home/user/src/main.rs now");
    assert!(out.contains("file://"), "inline file paths should link: {out}");
}

#[test]
fn md_unclosed_bold_is_literal() {
    let out = md_to_pango_light("**bold start");
    assert!(out.contains("**bold start") || out.contains("bold start"),
        "unmatched ** treated as text: {out}");
}

#[test]
fn md_unclosed_code_block_auto_closes() {
    let out = md_to_pango_light("```\nsome code here");
    assert!(out.contains("monospace"), "code block opened: {out}");
    assert!(out.contains("</span>"), "code block auto-closed: {out}");
}

#[test]
fn md_empty_input() {
    let out = md_to_pango_light("");
    assert_eq!(out, "");
}

#[test]
fn md_multiple_paragraphs() {
    let out = md_to_pango_light("first\n\nsecond");
    assert!(out.contains("first"));
    assert!(out.contains("second"));
}

// ──────────────────────────────────────────────────────────────────────
// Markdown: stress & edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn md_deeply_nested_formatting() {
    let out = md_to_pango_light("**bold *bold-italic* bold**");
    assert!(out.contains("<b>"), "outer bold: {out}");
    assert!(out.contains("<i>"), "inner italic: {out}");
    assert!(out.contains("</i>"), "italic closed: {out}");
    assert!(out.contains("</b>"), "bold closed: {out}");
}

#[test]
fn md_multiple_code_blocks_with_different_langs() {
    let md = "```rust\nfn main() {}\n```\n\nText between.\n\n```python\nprint('hi')\n```";
    let out = md_to_pango_light(md);
    assert_eq!(out.matches("monospace").count(), 2, "two code blocks: {out}");
    assert!(out.contains("fn main()"));
    assert!(out.contains("print("));
    assert!(out.contains("Text between"));
}

#[test]
fn md_list_then_code_block() {
    let md = "- item one\n- item two\n\n```\ncode here\n```";
    let out = md_to_pango_light(md);
    assert_eq!(out.matches('•').count(), 2, "two bullets: {out}");
    assert!(out.contains("monospace"), "code block: {out}");
    assert!(out.contains("code here"));
}

#[test]
fn md_heading_then_list_then_paragraph() {
    let md = "# Title\n\n- a\n- b\n\nParagraph text.";
    let out = md_to_pango_light(md);
    assert!(out.contains("x-large"), "h1 size: {out}");
    assert_eq!(out.matches('•').count(), 2);
    assert!(out.contains("Paragraph text"));
}

#[test]
fn md_special_chars_everywhere() {
    let md = "# Heading & \"quoted\"\n\n`code <with> &`\n\n- item & stuff\n\n> blockquote & more";
    let out = md_to_pango_light(md);
    assert!(out.contains("&amp;"), "ampersand escaped: {out}");
    assert!(out.contains("&lt;with&gt;"), "angle brackets in code escaped: {out}");
    assert!(out.contains("Heading"), "heading text: {out}");
}

#[test]
fn md_many_file_paths_in_one_paragraph() {
    let md = "Check /home/user/src/a.rs and /home/user/src/b.rs and /opt/app/main.go for details.";
    let out = md_to_pango_light(md);
    assert_eq!(out.matches("file://").count(), 3, "three file links: {out}");
}

#[test]
fn md_streaming_incomplete_bold_in_code() {
    let out = md_to_pango_light("```\nsome code");
    assert!(out.contains("monospace"), "code block opened");
    let opens = out.matches("<span").count();
    let closes = out.matches("</span>").count();
    assert_eq!(opens, closes, "tags balanced: {out}");
}

#[test]
fn md_consecutive_blockquotes() {
    let out = md_to_pango_light("> line one\n> line two");
    assert!(out.contains("│"), "blockquote marker present");
    assert!(out.contains("line one"));
    assert!(out.contains("line two"));
}

#[test]
fn md_with_code_produces_valid_pango() {
    let md = r#"# Code Example

Here's some code:

```rust
fn main() {
    println!("Hello, world!");
}
```

And a file reference: `/home/user/src/main.rs:42`

**Bold** and *italic* work too.
"#;
    let out = md_to_pango_light(md);

    let opens = out.matches("<span").count() + out.matches("<b>").count()
        + out.matches("<i>").count() + out.matches("<s>").count();
    let closes = out.matches("</span>").count() + out.matches("</b>").count()
        + out.matches("</i>").count() + out.matches("</s>").count();
    assert_eq!(opens, closes, "all tags should be balanced:\n{out}");
}
