use gtk4 as gtk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::chat_webview::ChatWebView;
use crate::config::types::{NotificationLevel, Theme};
use crate::models::agent_config::AgentConfig;
use crate::models::chat::ChatMessage;
use crate::services::cli::ModelInfo;
use crate::services::cli::claude::ClaudeBackend;

pub(crate) type BackgroundTaskResultCb = Option<Rc<dyn Fn(String, String, bool)>>;
pub(crate) type TaskCompletedCb = Option<Rc<dyn Fn(String, String, Option<String>)>>;

/// Process-related state.
pub(crate) struct AgentProcessState {
    pub process: ClaudeBackend,
    pub session_id: Option<String>,
    /// When true, the next spawn forks `session_id` (`--fork-session`) rather
    /// than resuming it. Set for cloned workspaces; cleared once the agent
    /// reports a (forked) session id.
    pub fork_session: bool,
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

/// Chat rendering state — uses ChatWebView.
pub(crate) struct ChatState {
    /// The WebView-based chat renderer.
    pub webview: ChatWebView,
    /// Index of the oldest history entry currently rendered.
    pub oldest_rendered_idx: usize,
    /// True while there is an active streaming block.
    pub current_streaming: bool,
    /// The element ID of the current streaming assistant message in the WebView.
    pub current_stream_id: Option<String>,
    /// Accumulated raw markdown for the current streaming block.
    pub current_text: String,
    /// Pending tool calls awaiting results. Key = tool call id, value = (name, input_json).
    pub pending_tools: HashMap<String, (String, String)>,
    /// The element ID of the current thinking indicator in the WebView.
    pub thinking_id: Option<String>,
    /// Persistence-format chat history (written to disk on autosave).
    pub chat_history: Rc<RefCell<Vec<ChatMessage>>>,
}

/// Panel configuration.
pub(crate) struct PanelConfig {
    pub agent_configs: Vec<AgentConfig>,
    pub selected_profile_idx: usize,
    pub theme: Rc<std::cell::Cell<Theme>>,
    pub notification_level: Rc<std::cell::Cell<NotificationLevel>>,
    /// Session model override (CLI model `value`). `None` → fall back to the
    /// agent profile's model, then the CLI default. Applied at the next spawn.
    pub model_override: Option<String>,
    /// Session reasoning effort (`--effort`). `None` → CLI default.
    pub effort: Option<String>,
    /// Selectable models fetched from the CLI; drives the switcher menu.
    pub models: Vec<ModelInfo>,
    /// The model the CLI actually resolved for the running session, reported by
    /// the `Started` event (e.g. `claude-opus-4-8[1m]`). Ground truth for the
    /// status bar; `None` until the first turn runs.
    pub active_model: Option<String>,
}

/// Top-level panel state — composes focused sub-structs.
pub(crate) struct PanelState {
    pub process: AgentProcessState,
    pub tokens: TokenState,
    pub chat: ChatState,
    pub config: PanelConfig,
    /// Status-bar label showing the effective model · effort. Updated from menu
    /// actions, the model probe, and the `Started` event.
    pub model_status_label: gtk::Label,
    pub tab_spinner: gtk::Spinner,
    /// Returns the workspace's current display name (custom tab label if set,
    /// else the directory basename). Used for desktop notification bodies, so it
    /// must reflect renames — hence a getter over the shared config, not a
    /// snapshot string.
    pub workspace_label: Rc<dyn Fn() -> String>,
    #[allow(dead_code)]
    pub on_open_file: Rc<dyn Fn(&str)>,
    pub on_session_id_change: Rc<dyn Fn(Option<String>)>,
    pub on_profile_change: Rc<dyn Fn(&str)>,
    /// Persists the session model override + effort to the workspace config.
    /// Args: (model_override, effort).
    pub on_model_effort_change: Rc<dyn Fn(Option<String>, Option<String>)>,
    pub on_tool_result: Option<Rc<dyn Fn()>>,
    /// Called when a background Bash task is detected (`run_in_background: true`).
    /// Args: (command, tool_use_id).
    pub on_background_task: Option<Rc<dyn Fn(String, String)>>,
    /// Called when a background task's ToolResult arrives (immediate boilerplate).
    /// Args: (tool_use_id, output, is_error).
    pub on_background_task_result: BackgroundTaskResultCb,
    /// Called when a task_notification event signals task completion.
    /// Args: (tool_use_id, status, output_file).
    pub on_task_completed: TaskCompletedCb,
    /// Tool IDs that are known background tasks (for result routing).
    pub pending_background_tasks: std::collections::HashSet<String>,
}
