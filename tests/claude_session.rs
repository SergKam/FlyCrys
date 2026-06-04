mod common;
use common::HOME_LOCK;
use std::fs;

// ──────────────────────────────────────────────────────────────────────
// services::claude_session — duplicating Claude Code transcripts
// ──────────────────────────────────────────────────────────────────────

#[test]
fn clone_session_copies_with_new_id() {
    let _guard = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation.
    unsafe { common::set_test_home(tmp.path()) };

    let old_id = "11111111-1111-1111-1111-111111111111";
    let new_id = "22222222-2222-2222-2222-222222222222";

    let proj = tmp.path().join(".claude/projects/-home-user-proj");
    fs::create_dir_all(&proj).unwrap();

    // A plain line, plus one where the old id also appears inside content
    // (mimicking a transcript that mentions its own file path).
    let line1 = format!(r#"{{"type":"user","sessionId":"{old_id}","content":"hi"}}"#);
    let line2 = format!(
        r#"{{"type":"assistant","sessionId":"{old_id}","message":{{"content":"see {old_id}.jsonl"}}}}"#
    );
    fs::write(
        proj.join(format!("{old_id}.jsonl")),
        format!("{line1}\n{line2}\n"),
    )
    .unwrap();

    assert!(flycrys::services::claude_session::clone_session(
        old_id, new_id
    ));

    let dst = proj.join(format!("{new_id}.jsonl"));
    assert!(dst.is_file(), "clone should create the new transcript file");

    let copied = fs::read_to_string(&dst).unwrap();
    let lines: Vec<&str> = copied.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2);
    for l in &lines {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert_eq!(v["sessionId"], new_id, "top-level sessionId is rewritten");
    }

    // A content reference to the old id must be preserved, not rewritten.
    assert!(
        copied.contains(&format!("{old_id}.jsonl")),
        "content references to the old id are preserved"
    );

    // The source transcript is left untouched.
    let orig = fs::read_to_string(proj.join(format!("{old_id}.jsonl"))).unwrap();
    assert!(orig.contains(&format!("\"sessionId\":\"{old_id}\"")));
}

#[test]
fn clone_session_missing_source_returns_false() {
    let _guard = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation.
    unsafe { common::set_test_home(tmp.path()) };

    assert!(!flycrys::services::claude_session::clone_session(
        "00000000-0000-0000-0000-000000000000",
        "99999999-9999-9999-9999-999999999999",
    ));
}
