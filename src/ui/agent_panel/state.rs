use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::config::types::{NotificationLevel, Theme};
use crate::models::agent_config::AgentConfig;
use crate::models::chat::ChatMessage;
use crate::services::cli::claude::ClaudeBackend;

/// Metadata for a pending tool call (awaiting result).
pub(crate) struct ToolInfo {
    pub content_box: gtk::Box,
    pub spinner: gtk::Spinner,
    pub expander: gtk::Expander,
    pub tool_name: String,
    pub tool_input: String,
}

/// Process-related state.
pub(crate) struct AgentProcessState {
    pub process: ClaudeBackend,
    pub session_id: Option<String>,
    pub working_dir: std::path::PathBuf,
}

/// Token and cost tracking.
pub(crate) struct TokenState {
    pub context_tokens: u64,
    pub context_window_max: u64,
    pub total_cost_usd: f64,
    pub token_label: gtk::Label,
    pub cost_label: gtk::Label,
}

/// Chat rendering state.
pub(crate) struct ChatState {
    pub current_text_label: Option<gtk::Label>,
    pub current_text: String,
    pub pending_tools: HashMap<String, ToolInfo>,
    pub thinking_spinner: Option<gtk::Box>,
    pub chat_history: Rc<RefCell<Vec<ChatMessage>>>,
}

/// Panel configuration.
pub(crate) struct PanelConfig {
    pub agent_configs: Vec<AgentConfig>,
    pub selected_profile_idx: usize,
    pub theme: Rc<Cell<Theme>>,
    pub notification_level: Rc<Cell<NotificationLevel>>,
}

/// Top-level panel state — composes focused sub-structs.
pub(crate) struct PanelState {
    pub process: AgentProcessState,
    pub tokens: TokenState,
    pub chat: ChatState,
    pub config: PanelConfig,
    // UI widgets that don't belong in sub-structs
    pub tab_spinner: gtk::Spinner,
    // Callbacks
    pub on_open_file: Rc<dyn Fn(&str)>,
    pub on_session_id_change: Rc<dyn Fn(Option<String>)>,
    pub on_profile_change: Rc<dyn Fn(&str)>,
    pub on_tool_result: Option<Rc<dyn Fn()>>,
}
