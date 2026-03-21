use serde::{Deserialize, Serialize};

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
