// Thin re-export module: all logic has moved to services::storage.
// Kept for backward compatibility during the transition period.

// Re-export model types so existing `use crate::session::X` still works.
pub use crate::models::AgentConfig;
pub use crate::models::AppConfig;
pub use crate::models::ChatMessage;
pub use crate::models::{RunTabConfig, RunTabType, WorkspaceConfig};

// Re-export all storage functions so existing `session::func()` calls still compile.
pub use crate::services::storage::dedup_labels;
pub use crate::services::storage::delete_agent_config;
pub use crate::services::storage::delete_chat_history;
pub use crate::services::storage::delete_terminal_tab_content;
pub use crate::services::storage::delete_workspace_config;
pub use crate::services::storage::ensure_default_agents;
pub use crate::services::storage::list_agent_configs;
pub use crate::services::storage::load_agent_config;
pub use crate::services::storage::load_app_config;
pub use crate::services::storage::load_chat_history;
pub use crate::services::storage::load_workspace_config;
pub use crate::services::storage::save_agent_config;
pub use crate::services::storage::save_app_config;
pub use crate::services::storage::save_chat_history;
pub use crate::services::storage::save_workspace_config;
pub use crate::services::storage::terminal_content_path;
pub use crate::services::storage::terminal_tab_content_path;

pub use crate::models::Bookmark;
pub use crate::services::storage::ensure_default_bookmarks;
pub use crate::services::storage::load_bookmarks;
pub use crate::services::storage::save_bookmarks;
