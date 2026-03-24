use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::chat_entry::ChatEntry;
use crate::config::types::{NotificationLevel, Theme};
use crate::models::agent_config::AgentConfig;
use crate::models::chat::ChatMessage;
use crate::services::cli::claude::ClaudeBackend;

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

/// Chat rendering state — simple box with manual pagination.
pub(crate) struct ChatState {
    /// Vertical box holding [load_prev_btn, message widgets…].
    pub chat_box: gtk::Box,
    /// ScrolledWindow wrapping `chat_box`.
    pub scrolled: gtk::ScrolledWindow,
    /// "Load previous messages" button at the top of `chat_box`.
    pub load_prev_btn: gtk::Button,
    /// Index of the oldest history entry currently rendered.
    /// Entries `[oldest_rendered_idx .. history.len())` have widgets in the box.
    pub oldest_rendered_idx: usize,
    /// The entry currently being streamed (assistant text).
    pub current_streaming_entry: Option<ChatEntry>,
    /// Accumulated raw markdown for the current streaming block.
    pub current_text: String,
    /// Pending tool calls awaiting results. Key = tool call id.
    pub pending_tools: HashMap<String, ChatEntry>,
    /// Thinking spinner sentinel entry.
    pub thinking_entry: Option<ChatEntry>,
    /// Persistence-format chat history (written to disk on autosave).
    pub chat_history: Rc<RefCell<Vec<ChatMessage>>>,
}

/// Panel configuration.
pub(crate) struct PanelConfig {
    pub agent_configs: Vec<AgentConfig>,
    pub selected_profile_idx: usize,
    pub theme: Rc<std::cell::Cell<Theme>>,
    pub notification_level: Rc<std::cell::Cell<NotificationLevel>>,
}

/// Top-level panel state — composes focused sub-structs.
pub(crate) struct PanelState {
    pub process: AgentProcessState,
    pub tokens: TokenState,
    pub chat: ChatState,
    pub config: PanelConfig,
    pub tab_spinner: gtk::Spinner,
    pub on_open_file: Rc<dyn Fn(&str)>,
    pub on_session_id_change: Rc<dyn Fn(Option<String>)>,
    pub on_profile_change: Rc<dyn Fn(&str)>,
    pub on_tool_result: Option<Rc<dyn Fn()>>,
}
