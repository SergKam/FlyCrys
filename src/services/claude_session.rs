//! Duplicating Claude Code session transcripts.
//!
//! Claude Code stores per-project session transcripts as JSONL under
//! `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`, where the directory
//! name is the working directory with `/` and `.` replaced by `-`. Every line
//! is a JSON object carrying a top-level `sessionId` field equal to the file's
//! id.
//!
//! To "clone" a session we copy that transcript under a fresh id (so the new
//! workspace gets an independent session rather than sharing the original).
//! Rather than reproduce Claude's path encoding — fragile for paths containing
//! dots or other punctuation — we locate the file by its globally-unique id and
//! write the copy alongside it, in the same project directory.

use std::path::PathBuf;

/// `~/.claude/projects`, where Claude Code keeps session transcripts.
fn claude_projects_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Find the transcript file for `session_id` by scanning the project
/// directories for `<session_id>.jsonl` (the id is a UUID, so it is unique).
fn find_session_file(session_id: &str) -> Option<PathBuf> {
    let projects = claude_projects_dir()?;
    let file_name = format!("{session_id}.jsonl");
    for entry in std::fs::read_dir(&projects).ok()?.flatten() {
        let dir = entry.path();
        if dir.is_dir() {
            let candidate = dir.join(&file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Duplicate the transcript for `old_id` into a new transcript for `new_id` in
/// the same project directory, rewriting only the top-level `sessionId` field on
/// each line (content/tool fields that merely mention the old id are preserved).
///
/// Returns `true` if a source transcript was found and the copy was written.
pub fn clone_session(old_id: &str, new_id: &str) -> bool {
    let Some(src) = find_session_file(old_id) else {
        return false;
    };
    let Some(dir) = src.parent() else {
        return false;
    };
    let dst = dir.join(format!("{new_id}.jsonl"));

    let Ok(content) = std::fs::read_to_string(&src) else {
        return false;
    };

    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Rewrite only the top-level `sessionId`; fall back to the raw line if a
        // line is not valid JSON (never expected, but keeps the copy lossless).
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(mut value) => {
                if let Some(obj) = value.as_object_mut()
                    && obj.contains_key("sessionId")
                {
                    obj.insert(
                        "sessionId".to_string(),
                        serde_json::Value::String(new_id.to_string()),
                    );
                }
                match serde_json::to_string(&value) {
                    Ok(s) => out.push_str(&s),
                    Err(_) => out.push_str(line),
                }
            }
            Err(_) => out.push_str(line),
        }
        out.push('\n');
    }

    std::fs::write(&dst, out).is_ok()
}
