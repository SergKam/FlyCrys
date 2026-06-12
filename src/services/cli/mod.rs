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
    /// Background task completed / failed / stopped.
    TaskNotification {
        tool_use_id: String,
        /// "completed", "failed", or "stopped"
        status: String,
        output_file: Option<String>,
    },
    /// The agent called `AskUserQuestion`. `input_json` is the tool input
    /// (`{"questions":[…]}`); answer via `AgentBackend::answer_question` using
    /// `request_id`.
    AskUserQuestion {
        request_id: String,
        input_json: String,
    },
    ProcessError(String),
}

/// Configuration for spawning an agent.
#[derive(Default)]
pub struct AgentSpawnConfig {
    pub system_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    /// Reasoning effort level (`--effort`): one of the model's
    /// `supported_effort_levels`. `None` lets the CLI pick its default.
    pub effort: Option<String>,
    pub resume_session_id: Option<String>,
    /// Fork `resume_session_id` into a new session instead of resuming it
    /// (adds `--fork-session`). Ignored when `resume_session_id` is `None`.
    pub fork_session: bool,
}

/// A model the account can select, as advertised by the CLI's `initialize`
/// control response. The list (and each model's valid effort levels) is fetched
/// at runtime via [`probe_models`] — nothing here is hardcoded.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ModelInfo {
    /// Identifier passed to `--model` (e.g. `default`, `sonnet`, `claude-fable-5[1m]`).
    pub value: String,
    #[serde(rename = "displayName", default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "supportsEffort", default)]
    pub supports_effort: bool,
    /// Effort levels valid for this model (e.g. Sonnet omits `xhigh`; Haiku has none).
    #[serde(rename = "supportedEffortLevels", default)]
    pub supported_effort_levels: Vec<String>,
}

/// Fetch the account's selectable models from the Claude CLI by performing the
/// stdio control-protocol `initialize` handshake and reading the model list out
/// of the response (`response.response.models`). Blocking — run on a worker
/// thread. Returns an empty vec on any failure (CLI missing, auth, parse error).
pub fn probe_models(working_dir: &Path) -> Vec<ModelInfo> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let mut child = match Command::new("claude")
        .args([
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--input-format",
            "stream-json",
            "--permission-prompt-tool",
            "stdio",
        ])
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    if let Some(stdin) = child.stdin.as_mut() {
        let init = r#"{"type":"control_request","request_id":"flycrys_probe","request":{"subtype":"initialize","hooks":null}}"#;
        let _ = writeln!(stdin, "{init}");
        let _ = stdin.flush();
    }

    let mut models = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        // The init control-response is the first line the CLI emits; bound the
        // scan so a missing response can't read forever.
        for line in BufReader::new(stdout)
            .lines()
            .take(50)
            .map_while(Result::ok)
        {
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if val.get("type").and_then(|v| v.as_str()) == Some("control_response") {
                if let Some(arr) = val
                    .pointer("/response/response/models")
                    .and_then(|m| m.as_array())
                {
                    models = arr
                        .iter()
                        .filter_map(|m| serde_json::from_value::<ModelInfo>(m.clone()).ok())
                        .collect();
                }
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    models
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

    /// Answer a pending `AskUserQuestion` control request. `updated_input` is the
    /// tool input echoed back with an added `answers` map (question text → label).
    fn answer_question(
        &mut self,
        request_id: &str,
        updated_input: serde_json::Value,
    ) -> Result<(), String>;

    /// Reject a pending `AskUserQuestion`: the user picked none of the offered
    /// options and will describe what they want instead. Denies the control
    /// request so the model stops asking and waits for the user's reply.
    fn reject_question(&mut self, request_id: &str) -> Result<(), String>;

    fn pause(&mut self);
    fn resume(&mut self);
    fn stop(&mut self);
    fn is_alive(&self) -> bool;
    fn is_paused(&self) -> bool;
    fn is_running(&self) -> bool;
}
