use std::fs;
use std::path::PathBuf;

use crate::models::{AgentConfig, AppConfig, Bookmark, ChatMessage, WorkspaceConfig};

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("flycrys")
}

fn sessions_dir() -> PathBuf {
    config_dir().join("sessions")
}

fn agents_dir() -> PathBuf {
    config_dir().join("agents")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn workspace_path(id: &str) -> PathBuf {
    sessions_dir().join(format!("{id}.json"))
}

pub fn load_app_config() -> AppConfig {
    let path = config_path();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    }
}

pub fn save_app_config(config: &AppConfig) {
    let dir = config_dir();
    let _ = fs::create_dir_all(&dir);
    let path = config_path();
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, data);
    }
}

pub fn load_workspace_config(id: &str) -> Option<WorkspaceConfig> {
    let path = workspace_path(id);
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_workspace_config(config: &WorkspaceConfig) {
    let dir = sessions_dir();
    let _ = fs::create_dir_all(&dir);
    let path = workspace_path(&config.id);
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, data);
    }
}

pub fn delete_workspace_config(id: &str) {
    let path = workspace_path(id);
    let _ = fs::remove_file(path);
}

// --- Terminal scrollback persistence ---

pub fn terminal_content_path(workspace_id: &str) -> PathBuf {
    sessions_dir().join(format!("{workspace_id}_terminal.txt"))
}

/// Path for a specific run-panel tab's scrollback content.
pub fn terminal_tab_content_path(workspace_id: &str, tab_id: &str) -> PathBuf {
    sessions_dir().join(format!("{workspace_id}_terminal_{tab_id}.txt"))
}

/// Remove the scrollback file for a closed run-panel tab.
pub fn delete_terminal_tab_content(workspace_id: &str, tab_id: &str) {
    let path = terminal_tab_content_path(workspace_id, tab_id);
    let _ = fs::remove_file(path);
}

// --- Chat history persistence ---

fn chat_history_path(workspace_id: &str) -> PathBuf {
    sessions_dir().join(format!("{workspace_id}_chat.json"))
}

/// Persist chat history synchronously. Use on shutdown and wherever the write
/// must be on disk before proceeding (the test suite relies on this).
pub fn save_chat_history(workspace_id: &str, messages: &[ChatMessage]) {
    write_chat_history(chat_history_path(workspace_id), messages);
}

/// Like [`save_chat_history`] but does the (potentially multi-MB) serialize and
/// disk write on a background thread, so periodic autosave never blocks the GTK
/// main loop. Not for shutdown — the process may exit before the thread runs.
pub fn save_chat_history_async(workspace_id: &str, messages: &[ChatMessage]) {
    // Cloning the history is a cheap memcpy of the String buffers; the expensive
    // serialize + write happen on the worker thread.
    let path = chat_history_path(workspace_id);
    let messages = messages.to_vec();
    std::thread::spawn(move || write_chat_history(path, &messages));
}

/// Atomic write: serialize to a temp file, then rename over the target, so a
/// crash mid-write can never leave a truncated / corrupt history file.
fn write_chat_history(path: PathBuf, messages: &[ChatMessage]) {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Disambiguates temp files if two writes overlap.
    static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

    let _ = fs::create_dir_all(sessions_dir());
    let Ok(data) = serde_json::to_string(messages) else {
        return;
    };
    let seq = WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.tmp.{seq}"));
    if fs::write(&tmp, data).is_ok() {
        let _ = fs::rename(&tmp, &path);
    } else {
        let _ = fs::remove_file(&tmp);
    }
}

pub fn load_chat_history(workspace_id: &str) -> Vec<ChatMessage> {
    let path = chat_history_path(workspace_id);
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn delete_chat_history(workspace_id: &str) {
    let path = chat_history_path(workspace_id);
    let _ = fs::remove_file(path);
}

// --- CLI session transcript backup ---
//
// `claude --resume <id>` reads the CLI's *own* transcript at
// ~/.claude/projects/<encoded-cwd>/<id>.jsonl — not our chat history. The CLI
// prunes those transcripts by age (`cleanupPeriodDays`, default 30 days), so a
// workspace left idle for weeks loses its backing file and can no longer be
// resumed ("No conversation found with session ID"). We keep a rolling copy
// under our own config dir so the transcript survives that cleanup. All
// filesystem work runs off the GTK main thread (see the `_async` variant).

fn transcripts_dir() -> PathBuf {
    sessions_dir().join("transcripts")
}

/// Path of our backup copy for a given CLI session id.
pub fn transcript_backup_path(session_id: &str) -> PathBuf {
    transcripts_dir().join(format!("{session_id}.jsonl"))
}

/// Map an absolute working directory to the CLI's project-folder name: the cwd
/// with every non-alphanumeric character replaced by '-' (e.g. `/home/u/p` ->
/// `-home-u-p`). Only a fast-path guess — [`locate_cli_transcript`] scans if it
/// misses.
fn encode_project_dir(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Locate the CLI's transcript for `session_id`. Tries the deterministic path
/// derived from `cwd`, then scans every project folder (session ids are UUIDs,
/// so any match is unambiguous). `None` means the CLI has no such transcript
/// (already pruned, or the session never wrote one yet).
fn locate_cli_transcript(session_id: &str, cwd: &str) -> Option<PathBuf> {
    let projects = dirs::home_dir()?.join(".claude").join("projects");
    let file = format!("{session_id}.jsonl");

    let guess = projects.join(encode_project_dir(cwd)).join(&file);
    if guess.is_file() {
        return Some(guess);
    }
    for entry in fs::read_dir(&projects).ok()?.flatten() {
        let candidate = entry.path().join(&file);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// True when `dst` already mirrors `src` (same byte length, not older) so the
/// copy can be skipped. Stat-only — never reads file contents.
fn backup_up_to_date(src: &std::path::Path, dst: &std::path::Path) -> bool {
    let (Ok(s), Ok(d)) = (fs::metadata(src), fs::metadata(dst)) else {
        return false;
    };
    s.len() == d.len() && matches!((s.modified(), d.modified()), (Ok(sm), Ok(dm)) if sm <= dm)
}

/// Atomic copy of `src` into `dir` as `<session_id>.jsonl`: copy to a temp file
/// then rename, so a reader never sees a half-written transcript.
fn copy_transcript_atomic(
    src: &std::path::Path,
    dir: &std::path::Path,
    session_id: &str,
) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    /// Disambiguates temp files if two copies for one session ever overlap.
    static SEQ: AtomicU64 = AtomicU64::new(0);

    fs::create_dir_all(dir)?;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{session_id}.jsonl.tmp.{seq}"));
    match fs::copy(src, &tmp) {
        Ok(_) => fs::rename(&tmp, dir.join(format!("{session_id}.jsonl"))),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Back up the CLI transcript for `session_id` (best effort, synchronous).
/// No-op if the transcript can't be located or our copy is already current.
pub fn backup_session_transcript(session_id: &str, cwd: &str) {
    let Some(src) = locate_cli_transcript(session_id, cwd) else {
        return;
    };
    let dst = transcript_backup_path(session_id);
    if backup_up_to_date(&src, &dst) {
        return;
    }
    let _ = copy_transcript_atomic(&src, &transcripts_dir(), session_id);
}

/// Like [`backup_session_transcript`] but runs the locate + copy on a worker
/// thread, so periodic autosave never touches the disk on the GTK main loop.
pub fn backup_session_transcript_async(session_id: String, cwd: String) {
    std::thread::spawn(move || backup_session_transcript(&session_id, &cwd));
}

// --- Agent config persistence ---

fn agent_config_path(name: &str) -> PathBuf {
    agents_dir().join(format!("{}.json", name.to_lowercase()))
}

pub fn load_agent_config(name: &str) -> Option<AgentConfig> {
    let path = agent_config_path(name);
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_agent_config(config: &AgentConfig) {
    let dir = agents_dir();
    let _ = fs::create_dir_all(&dir);
    let path = agent_config_path(&config.name);
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = fs::write(path, data);
    }
}

pub fn delete_agent_config(name: &str) {
    let path = agent_config_path(name);
    let _ = fs::remove_file(path);
}

pub fn list_agent_configs() -> Vec<AgentConfig> {
    let dir = agents_dir();
    let mut configs = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
                && let Ok(data) = fs::read_to_string(entry.path())
                && let Ok(config) = serde_json::from_str::<AgentConfig>(&data)
            {
                configs.push(config);
            }
        }
    }
    configs.sort_by(|a, b| a.name.cmp(&b.name));
    configs
}

// --- Bookmark persistence ---

fn bookmarks_path() -> PathBuf {
    config_dir().join("bookmarks.json")
}

pub fn load_bookmarks() -> Vec<Bookmark> {
    let path = bookmarks_path();
    if let Ok(data) = fs::read_to_string(path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn save_bookmarks(bookmarks: &[Bookmark]) {
    let _ = fs::create_dir_all(config_dir());
    if let Ok(data) = serde_json::to_string_pretty(bookmarks) {
        let _ = fs::write(bookmarks_path(), data);
    }
}

/// Seed default bookmarks if none exist yet.
pub fn ensure_default_bookmarks() {
    let path = bookmarks_path();
    if path.exists() {
        return;
    }
    let defaults = vec![
        Bookmark {
            name: "Commit changes".into(),
            prompt: "commit all changes with a meaningful message".into(),
        },
        Bookmark {
            name: "Create GitHub PR".into(),
            prompt: "create a pull request for current branch".into(),
        },
        Bookmark {
            name: "Update documentation".into(),
            prompt: "update documentation to reflect recent changes".into(),
        },
        Bookmark {
            name: "Run lint, build, tests".into(),
            prompt: "run lint, build, and tests, fix any errors".into(),
        },
    ];
    save_bookmarks(&defaults);
}

/// Create predefined agent profiles if they don't exist yet
pub fn ensure_default_agents() {
    let defaults = [
        AgentConfig {
            name: "Default".to_string(),
            system_prompt: String::new(),
            allowed_tools: Vec::new(),
            model: None,
        },
        AgentConfig {
            name: "Security".to_string(),
            system_prompt: "You are a security-focused code reviewer. Analyze code for \
                vulnerabilities, suggest fixes, and follow OWASP guidelines. Focus on \
                identifying injection attacks, authentication flaws, data exposure, and \
                insecure configurations."
                .to_string(),
            allowed_tools: Vec::new(),
            model: None,
        },
        AgentConfig {
            name: "Research".to_string(),
            system_prompt: "You are a code research assistant. Focus on understanding \
                codebases, explaining architecture, finding patterns, and answering \
                questions about code. Prefer reading and searching over modifying files."
                .to_string(),
            allowed_tools: vec![
                "Read".into(),
                "Grep".into(),
                "Glob".into(),
                "Bash".into(),
                "Agent".into(),
            ],
            model: None,
        },
    ];

    for config in defaults {
        let path = agent_config_path(&config.name);
        if !path.exists() {
            save_agent_config(&config);
        }
    }
}

/// Deduplicate tab labels by appending (2), (3), etc. when multiple workspaces
/// share the same directory basename.
pub fn dedup_labels(configs: &[WorkspaceConfig]) -> Vec<String> {
    let base_labels: Vec<String> = configs.iter().map(|c| c.tab_label()).collect();
    let mut result = base_labels.clone();

    for i in 0..result.len() {
        let mut count = 0;
        let mut my_index = 0;
        for (j, label) in base_labels.iter().enumerate() {
            if *label == base_labels[i] {
                count += 1;
                if j == i {
                    my_index = count;
                }
            }
        }
        if count > 1 {
            result[i] = format!("{} ({})", base_labels[i], my_index);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn encode_project_dir_matches_cli_scheme() {
        assert_eq!(encode_project_dir("/home/u/work/p"), "-home-u-work-p");
        // Dots and existing dashes both collapse to '-'.
        assert_eq!(encode_project_dir("/a.b/c-d"), "-a-b-c-d");
        assert_eq!(encode_project_dir("/srv/2solar"), "-srv-2solar");
    }

    #[test]
    fn up_to_date_detects_changes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.jsonl");
        let dst = dir.path().join("dst.jsonl");

        // Missing destination -> not up to date.
        fs::write(&src, b"hello").unwrap();
        assert!(!backup_up_to_date(&src, &dst));

        // Identical copy -> up to date.
        copy_transcript_atomic(&src, dir.path(), "dst").unwrap();
        assert!(backup_up_to_date(&src, &dst));

        // Source grows -> stale again (length mismatch).
        let mut f = fs::OpenOptions::new().append(true).open(&src).unwrap();
        f.write_all(b" world").unwrap();
        f.sync_all().unwrap();
        assert!(!backup_up_to_date(&src, &dst));
    }

    #[test]
    fn copy_transcript_atomic_writes_named_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.jsonl");
        fs::write(&src, b"{\"x\":1}\n").unwrap();
        let out = dir.path().join("backups");

        copy_transcript_atomic(&src, &out, "abc-123").unwrap();

        assert_eq!(fs::read(out.join("abc-123.jsonl")).unwrap(), b"{\"x\":1}\n");
        let temps = fs::read_dir(&out)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(temps, 0, "temp files must be renamed away");
    }
}
