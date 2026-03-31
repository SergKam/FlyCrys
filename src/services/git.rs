use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Classification of a git file status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unknown(String),
}

/// A single entry from `git status --porcelain`.
#[derive(Debug, Clone)]
pub struct GitStatusEntry {
    pub path: String,
    pub status: GitFileStatus,
    /// The original status code string (e.g. "M", "??", "AM") for display.
    pub raw_status: String,
}

/// Return the current git branch name (e.g. "main"), or `None` if not a repo / detached.
pub fn current_branch(working_dir: &Path) -> Option<String> {
    Command::new("git")
        .args([
            "-C",
            &working_dir.to_string_lossy(),
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Check if a given path is inside a git repository.
pub fn is_git_repo(working_dir: &Path) -> bool {
    Command::new("git")
        .args([
            "-C",
            &working_dir.to_string_lossy(),
            "rev-parse",
            "--git-dir",
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Get the list of changed files from `git status --porcelain`.
pub fn status(repo_path: &Path) -> Result<Vec<GitStatusEntry>, String> {
    let output = Command::new("git")
        .args(["-C", &repo_path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .map_err(|e| format!("git status failed: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let entries = text
        .lines()
        .filter(|l| l.len() >= 3)
        .map(|l| {
            let raw_status = l[..2].trim().to_string();
            let path = l[3..].to_string();
            let status = parse_status_code(&raw_status);
            GitStatusEntry {
                path,
                status,
                raw_status,
            }
        })
        .collect();

    Ok(entries)
}

/// Get the git diff for a specific file. Returns None if no changes.
pub fn diff_file(repo_path: &Path, file: &str) -> Option<String> {
    let rel = Path::new(file)
        .strip_prefix(repo_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string());
    let wd = repo_path.to_string_lossy();

    // Try HEAD (all uncommitted changes)
    if let Some(diff) = run_git_diff(&wd, &["diff", "HEAD", "--", &rel]) {
        return Some(diff);
    }
    // Try unstaged only
    if let Some(diff) = run_git_diff(&wd, &["diff", "--", &rel]) {
        return Some(diff);
    }
    // Try staged only
    run_git_diff(&wd, &["diff", "--cached", "--", &rel])
}

/// Check if a file has uncommitted git changes that produce a diff.
/// Excludes untracked files (`??`) — they appear in `git status` but have no
/// diff to display.
pub fn is_file_modified(file_path: &str, working_dir: &Path) -> bool {
    let rel = Path::new(file_path)
        .strip_prefix(working_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    let output = Command::new("git")
        .args([
            "-C",
            &working_dir.to_string_lossy(),
            "status",
            "--porcelain",
            "--",
            &rel,
        ])
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            // Only count tracked changes (M, A, D, R, C) — not untracked (??)
            text.lines().any(|l| l.len() >= 2 && !l.starts_with("??"))
        }
        Err(_) => false,
    }
}

/// Build a map of relative-path → git file status for the whole repo.
/// Used by the file tree to color-code entries.
pub fn status_map(working_dir: &Path) -> HashMap<String, GitFileStatus> {
    let entries = status(working_dir).unwrap_or_default();
    entries.into_iter().map(|e| (e.path, e.status)).collect()
}

/// Given a file status map, compute the set of relative directory paths that
/// contain at least one changed file (recursively up to the repo root).
pub fn dirty_dirs(file_map: &HashMap<String, GitFileStatus>) -> HashSet<String> {
    let mut dirs = HashSet::new();
    for path in file_map.keys() {
        let p = Path::new(path);
        let mut parent = p.parent();
        while let Some(dir) = parent {
            let dir_str = dir.to_string_lossy().to_string();
            if dir_str.is_empty() || !dirs.insert(dir_str) {
                break; // already inserted this dir and all its ancestors
            }
            parent = dir.parent();
        }
    }
    dirs
}

/// Return the CSS class name for a given git status.
pub fn status_css_class(status: &GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Modified | GitFileStatus::Renamed | GitFileStatus::Copied => "git-modified",
        GitFileStatus::Added => "git-added",
        GitFileStatus::Deleted => "git-deleted",
        GitFileStatus::Untracked => "git-untracked",
        GitFileStatus::Unknown(_) => "git-modified",
    }
}

/// All CSS class names used for git status coloring — for bulk removal.
pub const GIT_CSS_CLASSES: &[&str] = &["git-modified", "git-added", "git-deleted", "git-untracked"];

fn run_git_diff(wd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(wd)
        .args(args)
        .output()
        .ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn parse_status_code(code: &str) -> GitFileStatus {
    match code {
        "M" | "MM" => GitFileStatus::Modified,
        "A" | "AM" => GitFileStatus::Added,
        "D" => GitFileStatus::Deleted,
        "R" => GitFileStatus::Renamed,
        "C" => GitFileStatus::Copied,
        "??" => GitFileStatus::Untracked,
        other => GitFileStatus::Unknown(other.to_string()),
    }
}
