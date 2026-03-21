use serde::{Deserialize, Serialize};

use crate::config::constants::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};
use crate::config::types::{NotificationLevel, Theme};

/// Global app configuration — tracks which workspaces are open and window state
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub active_tab: usize,
    pub workspace_ids: Vec<String>,
    pub window_width: i32,
    pub window_height: i32,
    #[serde(default = "default_true")]
    pub window_maximized: bool,
    #[serde(default, alias = "is_dark")]
    pub theme: Theme,
    #[serde(default, alias = "notifications_enabled")]
    pub notification_level: NotificationLevel,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_tab: 0,
            workspace_ids: Vec::new(),
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            window_maximized: true,
            theme: Theme::default(),
            notification_level: NotificationLevel::default(),
        }
    }
}
