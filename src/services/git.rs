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

/// Check if a file has uncommitted git changes.
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
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

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
