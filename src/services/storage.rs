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

pub fn save_chat_history(workspace_id: &str, messages: &[ChatMessage]) {
    let dir = sessions_dir();
    let _ = fs::create_dir_all(&dir);
    let path = chat_history_path(workspace_id);
    if let Ok(data) = serde_json::to_string(messages) {
        let _ = fs::write(path, data);
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
