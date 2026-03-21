pub mod claude;

use std::path::Path;
use std::sync::mpsc;

use crate::config::types::AgentOutcome;

/// An image to attach to a message sent to the agent.
pub struct ImageAttachment {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// Domain events that the UI layer consumes. CLI-agnostic.
pub enum AgentDomainEvent {
    Started {
        session_id: Option<String>,
        model: String,
        context_window: Option<u64>,
    },
    TextDelta(String),
    TextBlockFinished {
        full_text: String,
    },
    ThinkingStarted,
    ThinkingDelta(String),
    ThinkingFinished,
    ToolStarted {
        id: String,
        name: String,
    },
    ToolInputDelta(String),
    ToolInputFinished {
        id: String,
        name: String,
        input_json: String,
    },
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    },
    Finished {
        outcome: AgentOutcome,
        message: Option<String>,
        total_cost_usd: f64,
        num_turns: u32,
        context_window: Option<u64>,
    },
    ProcessError(String),
}

/// Configuration for spawning an agent.
#[derive(Default)]
pub struct AgentSpawnConfig {
    pub system_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub resume_session_id: Option<String>,
}

/// Backend trait for agent CLI interaction.
pub trait AgentBackend {
    fn spawn(
        &mut self,
        sender: mpsc::Sender<AgentDomainEvent>,
        working_dir: &Path,
        config: &AgentSpawnConfig,
    ) -> Result<(), String>;

    fn send_message(&mut self, text: &str, images: &[ImageAttachment]) -> Result<(), String>;
    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    fn is_alive(&self) -> bool;
    fn is_paused(&self) -> bool;
    fn is_running(&self) -> bool;
}
