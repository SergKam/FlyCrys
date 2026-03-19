use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Agent profile configuration — stored in ~/.config/flycrys/agents/
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentConfig {
    pub name: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Global app configuration — tracks which workspaces are open and window state
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub active_tab: usize,
    pub workspace_ids: Vec<String>,
    pub window_width: i32,
    pub window_height: i32,
    pub is_dark: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_tab: 0,
            workspace_ids: Vec::new(),
            window_width: 1400,
            window_height: 800,
            is_dark: false,
        }
    }
}

/// Per-workspace configuration — everything needed to restore a single tab
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceConfig {
    pub id: String,
    pub working_directory: String,
    pub tree_pane_width: i32,
    pub editor_terminal_split: i32,
    pub agent_pane_width: i32,
    pub open_file: Option<String>,
    pub terminal_visible: bool,
    #[serde(default = "default_profile")]
    pub agent_1_profile: String,
    #[serde(default)]
    pub agent_1_session_id: Option<String>,
}

fn default_profile() -> String {
    "Default".to_string()
}

impl WorkspaceConfig {
    pub fn new(working_directory: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            working_directory: working_directory.to_string(),
            tree_pane_width: 300,
            editor_terminal_split: -1,
            agent_pane_width: 420,
            open_file: None,
            terminal_visible: false,
            agent_1_profile: default_profile(),
            agent_1_session_id: None,
        }
    }

    /// Short label for the tab: directory basename
    pub fn tab_label(&self) -> String {
        Path::new(&self.working_directory)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.working_directory.clone())
    }
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config").join("flycrys")
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

// --- Chat history persistence ---

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ChatMessage {
    #[serde(rename = "user")]
    User { text: String },
    #[serde(rename = "assistant")]
    AssistantText { text: String },
    #[serde(rename = "tool")]
    ToolCall {
        tool_name: String,
        tool_input: String,
        output: String,
        is_error: bool,
    },
    #[serde(rename = "system")]
    System { text: String },
}

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
