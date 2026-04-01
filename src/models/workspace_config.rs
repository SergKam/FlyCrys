use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::constants::{
    AGENT_PANEL_MIN_WIDTH, EDITOR_TERMINAL_SPLIT_DEFAULT, TREE_PANE_DEFAULT_WIDTH,
};
use crate::config::types::PanelMode;

// ── Run Panel tab types ─────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RunTabType {
    Shell,
    BackgroundTask,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RunTabConfig {
    pub id: String,
    pub name: String,
    pub tab_type: RunTabType,
}

/// Per-workspace configuration — everything needed to restore a single tab
#[derive(Serialize, Clone, Debug)]
pub struct WorkspaceConfig {
    pub id: String,
    pub working_directory: String,
    pub tree_pane_width: i32,
    pub editor_terminal_split: i32,
    pub agent_pane_width: i32,
    pub open_file: Option<String>,
    pub terminal_visible: bool,
    pub agent_1_profile: String,
    pub agent_1_session_id: Option<String>,
    pub panel_mode: PanelMode,
    pub run_tabs: Vec<RunTabConfig>,
    pub active_run_tab: usize,
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
            panel_mode: PanelMode::default(),
            run_tabs: vec![RunTabConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: "bash(1)".to_string(),
                tab_type: RunTabType::Shell,
            }],
            active_run_tab: 0,
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

// ── Backward-compatible deserialization ──────────────────────────────────────
// Old configs have `view_mode` + `diff_mode`; new configs have `panel_mode`.

/// Raw shape used only for deserialization — supports both old and new formats.
#[derive(Deserialize)]
struct WorkspaceConfigRaw {
    id: String,
    working_directory: String,
    #[serde(default = "default_tree_pane")]
    tree_pane_width: i32,
    #[serde(default = "default_split")]
    editor_terminal_split: i32,
    #[serde(default = "default_agent_pane")]
    agent_pane_width: i32,
    #[serde(default)]
    open_file: Option<String>,
    #[serde(default)]
    terminal_visible: bool,
    #[serde(default = "default_profile")]
    agent_1_profile: String,
    #[serde(default)]
    agent_1_session_id: Option<String>,

    // New field
    #[serde(default)]
    panel_mode: Option<PanelMode>,

    // Run panel tabs (empty → default single bash tab)
    #[serde(default)]
    run_tabs: Vec<RunTabConfig>,
    #[serde(default)]
    active_run_tab: usize,

    // Legacy fields (consumed for migration, never serialized)
    #[serde(default, alias = "preview_mode")]
    view_mode: Option<crate::config::types::ViewMode>,
    #[serde(default, alias = "show_diff")]
    diff_mode: Option<crate::config::types::DiffMode>,
}

fn default_tree_pane() -> i32 {
    TREE_PANE_DEFAULT_WIDTH
}
fn default_split() -> i32 {
    EDITOR_TERMINAL_SPLIT_DEFAULT
}
fn default_agent_pane() -> i32 {
    AGENT_PANEL_MIN_WIDTH
}

impl<'de> Deserialize<'de> for WorkspaceConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = WorkspaceConfigRaw::deserialize(deserializer)?;
        let panel_mode = raw.panel_mode.unwrap_or_else(|| {
            // Migrate from old fields
            use crate::config::types::{DiffMode, ViewMode};
            if raw.diff_mode == Some(DiffMode::Visible) {
                PanelMode::Diff
            } else if raw.view_mode == Some(ViewMode::Preview) {
                PanelMode::Preview
            } else {
                PanelMode::Source
            }
        });
        // Old configs have no run_tabs — default to one bash(1) tab
        let run_tabs = if raw.run_tabs.is_empty() {
            vec![RunTabConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: "bash(1)".to_string(),
                tab_type: RunTabType::Shell,
            }]
        } else {
            raw.run_tabs
        };

        Ok(WorkspaceConfig {
            id: raw.id,
            working_directory: raw.working_directory,
            tree_pane_width: raw.tree_pane_width,
            editor_terminal_split: raw.editor_terminal_split,
            agent_pane_width: raw.agent_pane_width,
            open_file: raw.open_file,
            terminal_visible: raw.terminal_visible,
            agent_1_profile: raw.agent_1_profile,
            agent_1_session_id: raw.agent_1_session_id,
            panel_mode,
            run_tabs,
            active_run_tab: raw.active_run_tab,
        })
    }
}
