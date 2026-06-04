use flycrys::highlight;

// ──────────────────────────────────────────────────────────────────────
// Highlight: is_highlightable
// ──────────────────────────────────────────────────────────────────────

#[test]
fn highlightable_common_extensions() {
    // Note: toml/dockerfile are intentionally absent — syntect's default set
    // ships no grammar for them, so they honestly render as plain text. (The
    // old allow-list listed them, but they were never actually colored.)
    let extensions = [
        "rs", "js", "ts", "py", "go", "java", "c", "cpp", "html", "css", "json", "yaml", "yml",
        "md", "sql", "sh", "bash", "rb",
    ];
    for ext in extensions {
        let path = format!("test.{ext}");
        assert!(
            highlight::is_highlightable(&path),
            "{ext} should be highlightable"
        );
    }
}

#[test]
fn highlightable_jsx_tsx() {
    assert!(highlight::is_highlightable("component.jsx"));
    assert!(highlight::is_highlightable("component.tsx"));
}

#[test]
fn not_highlightable_binary_formats() {
    assert!(!highlight::is_highlightable("image.png"));
    assert!(!highlight::is_highlightable("archive.zip"));
    assert!(!highlight::is_highlightable("binary.exe"));
    assert!(!highlight::is_highlightable("data.bin"));
    assert!(!highlight::is_highlightable("video.mp4"));
}

#[test]
fn not_highlightable_no_extension() {
    // Genuinely unknown extensionless files fall back to plain text.
    assert!(!highlight::is_highlightable("LICENSE"));
    assert!(!highlight::is_highlightable("CHANGELOG"));
    // A real `Makefile` IS highlightable now — syntect recognizes it by name,
    // which the old lowercase "makefile" allow-list entry silently missed.
    assert!(highlight::is_highlightable("Makefile"));
}

// ──────────────────────────────────────────────────────────────────────
// Highlight: diff_to_pango
// ──────────────────────────────────────────────────────────────────────

#[test]
fn diff_to_pango_basic() {
    let out = highlight::diff_to_pango("old line", "new line", "test.txt");
    assert!(
        out.contains("#ffeef0"),
        "removed lines have red background: {out}"
    );
    assert!(
        out.contains("#e6ffed"),
        "added lines have green background: {out}"
    );
    assert!(out.contains("- "), "removed prefix: {out}");
    assert!(out.contains("+ "), "added prefix: {out}");
}

#[test]
fn diff_to_pango_multiline() {
    let old = "line1\nline2";
    let new = "line3\nline4\nline5";
    let out = highlight::diff_to_pango(old, new, "test.rs");
    // 2 removed lines + 3 added lines
    assert_eq!(out.matches("#ffeef0").count(), 2, "2 removed lines");
    assert_eq!(out.matches("#e6ffed").count(), 3, "3 added lines");
}

#[test]
fn diff_to_pango_escapes_html() {
    let out = highlight::diff_to_pango("<div>", "&amp;", "test.html");
    assert!(
        out.contains("&lt;") || out.contains("&lt;div&gt;"),
        "should escape HTML: {out}"
    );
}

#[test]
fn diff_to_pango_empty_old() {
    let out = highlight::diff_to_pango("", "added", "test.txt");
    assert!(out.contains("#e6ffed"), "should show added line");
}

#[test]
fn diff_to_pango_empty_new() {
    let out = highlight::diff_to_pango("removed", "", "test.txt");
    assert!(out.contains("#ffeef0"), "should show removed line");
}

#[test]
fn diff_to_pango_with_rust_syntax() {
    let old = "fn old() {}";
    let new = "fn new() {}";
    let out = highlight::diff_to_pango(old, new, "test.rs");
    // Should have syntax-colored spans
    assert!(
        out.contains("foreground="),
        "should have syntax colors: {out}"
    );
}

// ──────────────────────────────────────────────────────────────────────
// Highlight: diff_to_html
// ──────────────────────────────────────────────────────────────────────

#[test]
fn diff_to_html_basic() {
    let out = highlight::diff_to_html("old line", "new line", "test.txt");
    assert!(out.contains("diff-del"), "removed lines class: {out}");
    assert!(out.contains("diff-add"), "added lines class: {out}");
    assert!(out.contains("- "), "removed prefix: {out}");
    assert!(out.contains("+ "), "added prefix: {out}");
}

#[test]
fn diff_to_html_escapes() {
    let out = highlight::diff_to_html("<div>", "&amp;", "test.html");
    assert!(
        out.contains("&lt;") || out.contains("&lt;div&gt;"),
        "should escape HTML: {out}"
    );
}

// ──────────────────────────────────────────────────────────────────────
// ClaudeBackend: process state
// ──────────────────────────────────────────────────────────────────────

#[test]
fn process_state_idle_by_default() {
    use flycrys::services::cli::AgentBackend;
    let proc = flycrys::services::cli::claude::ClaudeBackend::new();
    assert!(!proc.is_alive());
    assert!(!proc.is_running());
    assert!(!proc.is_paused());
}

#[test]
fn process_state_pause_resume_without_child() {
    use flycrys::services::cli::AgentBackend;
    let mut proc = flycrys::services::cli::claude::ClaudeBackend::new();

    // Pause on idle process is a no-op (no pid)
    proc.pause();
    assert!(!proc.is_alive());

    // Resume on idle process is a no-op
    proc.resume();
    assert!(!proc.is_alive());

    // Stop on idle process cleans up gracefully
    proc.stop();
    assert!(!proc.is_alive());
    assert!(!proc.is_running());
}

#[test]
fn process_drop_is_safe_when_idle() {
    // Dropping an idle ClaudeBackend should not panic
    let proc = flycrys::services::cli::claude::ClaudeBackend::new();
    drop(proc);
}

// ──────────────────────────────────────────────────────────────────────
// Integration: highlight + markdown pipeline
// ──────────────────────────────────────────────────────────────────────

#[test]
fn diff_then_markdown_no_double_escape() {
    // diff_to_pango output should already be escaped; verify no double-escaping
    let diff = highlight::diff_to_pango("a < b", "c > d", "test.txt");
    assert!(diff.contains("&lt;"), "< escaped: {diff}");
    assert!(diff.contains("&gt;"), "> escaped: {diff}");
    assert!(!diff.contains("&amp;lt;"), "no double escape: {diff}");
    assert!(!diff.contains("&amp;gt;"), "no double escape: {diff}");
}

#[test]
fn diff_identical_strings() {
    // When old == new, both still show as removed+added (simple diff, no unchanged detection)
    let out = highlight::diff_to_pango("same", "same", "test.txt");
    assert!(out.contains("#ffeef0"), "removed section present");
    assert!(out.contains("#e6ffed"), "added section present");
}

#[test]
fn diff_multiline_rust_preserves_syntax_colors() {
    let old = "fn foo() -> i32 {\n    42\n}";
    let new = "fn bar() -> String {\n    \"hello\".to_string()\n}";
    let out = highlight::diff_to_pango(old, new, "lib.rs");

    // Both sections should have syntax highlighting
    assert!(out.contains("foreground="), "syntax colors present: {out}");
    // Both red and green backgrounds
    assert!(out.matches("#ffeef0").count() == 3, "3 removed lines");
    assert!(out.matches("#e6ffed").count() == 3, "3 added lines");
}

/// Distinct foreground colors emitted by `diff_to_html`, excluding the fixed
/// red/green diff-marker colors so only content tokenization is counted.
fn content_colors(out: &str) -> std::collections::HashSet<String> {
    let markers = ["b31d28", "22863a"]; // del "- " / add "+ " marker colors
    out.match_indices("color:#")
        .filter_map(|(i, _)| out.get(i + 7..i + 13))
        .filter(|c| !markers.contains(c))
        .map(|c| c.to_string())
        .collect()
}

#[test]
fn toml_grammar_tokenizes() {
    // Bundled TOML grammar: comment + string + number must produce >1 color,
    // which also exercises the grammar's regexes at runtime (no panic).
    let snippet = "# a comment\nname = \"flycrys\"\nport = 8080";
    let out = highlight::diff_to_html(snippet, snippet, "Cargo.toml");
    let colors = content_colors(&out);
    assert!(colors.len() > 1, "TOML should tokenize: {colors:?}");
}

#[test]
fn dockerfile_grammar_tokenizes() {
    let snippet = "# comment\nFROM rust:1.96 AS build\nRUN cargo build";
    let out = highlight::diff_to_html(snippet, snippet, "Dockerfile");
    let colors = content_colors(&out);
    assert!(colors.len() > 1, "Dockerfile should tokenize: {colors:?}");
}

#[test]
fn highlightable_alias_extensions() {
    // mjs/cjs/jsx/tsx/yml/mdx should all be highlightable
    for ext in ["mjs", "cjs", "jsx", "tsx", "yml", "mdx"] {
        assert!(
            highlight::is_highlightable(&format!("file.{ext}")),
            "{ext} should be highlightable"
        );
    }
}
