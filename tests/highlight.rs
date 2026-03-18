use flycrys::highlight;

// ──────────────────────────────────────────────────────────────────────
// Highlight: is_highlightable
// ──────────────────────────────────────────────────────────────────────

#[test]
fn highlightable_common_extensions() {
    let extensions = [
        "rs", "js", "ts", "py", "go", "java", "c", "cpp", "html", "css",
        "json", "yaml", "yml", "toml", "md", "sql", "sh", "bash", "rb",
    ];
    for ext in extensions {
        let path = format!("test.{ext}");
        assert!(highlight::is_highlightable(&path), "{ext} should be highlightable");
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
    assert!(!highlight::is_highlightable("Makefile"));
}

// ──────────────────────────────────────────────────────────────────────
// Highlight: diff_to_pango
// ──────────────────────────────────────────────────────────────────────

#[test]
fn diff_to_pango_basic() {
    let out = highlight::diff_to_pango("old line", "new line", "test.txt");
    assert!(out.contains("#ffeef0"), "removed lines have red background: {out}");
    assert!(out.contains("#e6ffed"), "added lines have green background: {out}");
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
    assert!(out.contains("&lt;") || out.contains("&lt;div&gt;"), "should escape HTML: {out}");
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
    assert!(out.contains("foreground="), "should have syntax colors: {out}");
}

// ──────────────────────────────────────────────────────────────────────
// AgentProcess: ProcessState
// ──────────────────────────────────────────────────────────────────────

#[test]
fn process_state_idle_by_default() {
    use flycrys::agent_process::ProcessState;
    let proc = flycrys::agent_process::AgentProcess::new();
    assert_eq!(proc.state, ProcessState::Idle);
    assert!(!proc.is_alive());
}

#[test]
fn process_state_pause_resume_without_child() {
    use flycrys::agent_process::ProcessState;
    let mut proc = flycrys::agent_process::AgentProcess::new();

    // Pause on idle process is a no-op (no pid)
    proc.pause();
    assert_eq!(proc.state, ProcessState::Idle);

    // Resume on idle process is a no-op
    proc.resume();
    assert_eq!(proc.state, ProcessState::Idle);

    // Stop on idle process cleans up gracefully
    proc.stop();
    assert_eq!(proc.state, ProcessState::Idle);
    assert!(!proc.is_alive());
}

#[test]
fn process_drop_is_safe_when_idle() {
    // Dropping an idle AgentProcess should not panic
    let proc = flycrys::agent_process::AgentProcess::new();
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

#[test]
fn highlightable_alias_extensions() {
    // mjs/cjs/jsx/tsx/yml/mdx should all be highlightable
    for ext in ["mjs", "cjs", "jsx", "tsx", "yml", "mdx"] {
        assert!(highlight::is_highlightable(&format!("file.{ext}")),
            "{ext} should be highlightable");
    }
}
