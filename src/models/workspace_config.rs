use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::constants::{
    AGENT_PANEL_MIN_WIDTH, EDITOR_TERMINAL_SPLIT_DEFAULT, TREE_PANE_DEFAULT_WIDTH,
};
use crate::config::types::{DiffMode, ViewMode};

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
    #[serde(default, alias = "preview_mode")]
    pub view_mode: ViewMode,
    #[serde(default, alias = "show_diff")]
    pub diff_mode: DiffMode,
}

fn default_profile() -> String {
    "Default".to_string()
}

impl WorkspaceConfig {
    pub fn new(working_directory: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            working_directory: working_directory.to_string(),
            tree_pane_width: TREE_PANE_DEFAULT_WIDTH,
            editor_terminal_split: EDITOR_TERMINAL_SPLIT_DEFAULT,
            agent_pane_width: AGENT_PANEL_MIN_WIDTH,
            open_file: None,
            terminal_visible: false,
            agent_1_profile: default_profile(),
            agent_1_session_id: None,
            view_mode: ViewMode::default(),
            diff_mode: DiffMode::Visible,
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
