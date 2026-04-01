use base64::{Engine as _, engine::general_purpose};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::{FromRawFd, OwnedFd};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;

use crate::config::types::AgentOutcome;

use super::{AgentBackend, AgentDomainEvent, AgentSpawnConfig, ImageAttachment};

// ---------------------------------------------------------------------------
// Private Claude wire types — never leak outside this module
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum ClaudeEvent {
    #[serde(rename = "system")]
    System {
        subtype: Option<String>,
        session_id: Option<String>,
        model: Option<String>,
        // Task notification fields (present when subtype = "task_notification")
        #[serde(default)]
        task_id: Option<String>,
        #[serde(default)]
        tool_use_id: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        output_file: Option<String>,
    },
    #[serde(rename = "stream_event")]
    StreamEvent { event: Box<StreamEventData> },
    #[serde(rename = "assistant")]
    Assistant { message: AssistantMessage },
    #[serde(rename = "user")]
    User {
        tool_use_result: Option<serde_json::Value>,
        #[serde(default)]
        message: serde_json::Value,
    },
    #[serde(rename = "result")]
    Result {
        result: Option<String>,
        #[serde(default)]
        total_cost_usd: f64,
        #[serde(default)]
        num_turns: u32,
        #[serde(default)]
        is_error: bool,
        #[serde(rename = "modelUsage")]
        model_usage: Option<serde_json::Value>,
    },
    #[serde(rename = "process_error")]
    ProcessError { message: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct StreamEventData {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<u32>,
    pub content_block: Option<ContentBlock>,
    pub delta: Option<Delta>,
    pub message: Option<serde_json::Value>,
    pub usage: Option<StreamUsage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct Delta {
    #[serde(rename = "type")]
    pub delta_type: Option<String>,
    pub text: Option<String>,
    pub partial_json: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct StreamUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct AssistantMessage {
    pub content: Option<Vec<ContentBlock>>,
}

// ---------------------------------------------------------------------------
// Reader-thread state for tracking content blocks across events
// ---------------------------------------------------------------------------

/// Tracks the type of content block currently being streamed so that
/// `content_block_stop` can emit the correct finishing event.
enum ActiveBlock {
    Text {
        accumulated: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    Thinking,
    None,
}

// ---------------------------------------------------------------------------
// Event translation — Claude wire format -> AgentDomainEvent
// ---------------------------------------------------------------------------

fn translate_event(
    claude_event: ClaudeEvent,
    active_block: &mut ActiveBlock,
    sender: &mpsc::Sender<AgentDomainEvent>,
) {
    match claude_event {
        ClaudeEvent::System {
            subtype,
            session_id,
            model,
            tool_use_id,
            status,
            output_file,
            ..
        } => match subtype.as_deref() {
            Some("task_notification") => {
                if let (Some(tool_id), Some(st)) = (tool_use_id, status) {
                    let _ = sender.send(AgentDomainEvent::TaskNotification {
                        tool_use_id: tool_id,
                        status: st,
                        output_file,
                    });
                }
            }
            _ => {
                let context_window = model.as_deref().and_then(parse_context_window);
                let _ = sender.send(AgentDomainEvent::Started {
                    session_id,
                    model: model.unwrap_or_default(),
                    context_window,
                });
            }
        },

        ClaudeEvent::StreamEvent { event: ev } => {
            translate_stream_event(&ev, active_block, sender);
        }

        ClaudeEvent::User {
            tool_use_result: Some(_),
            ref message,
            ..
        } => {
            let first = message
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first());

            let tool_id = first
                .and_then(|item| item.get("tool_use_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let output = first
                .and_then(|item| item.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let is_error = first
                .and_then(|item| item.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let _ = sender.send(AgentDomainEvent::ToolResult {
                id: tool_id,
                output,
                is_error,
            });
        }

        ClaudeEvent::Result {
            result,
            total_cost_usd,
            is_error,
            num_turns,
            model_usage,
        } => {
            let outcome = if is_error {
                AgentOutcome::Error
            } else {
                AgentOutcome::Success
            };

            // Extract context window from modelUsage
            let context_window = model_usage.as_ref().and_then(|mu| {
                let obj = mu.as_object()?;
                for (_model_name, info) in obj {
                    if let Some(ctx) = info.get("contextWindow").and_then(|v| v.as_u64())
                        && ctx > 0
                    {
                        return Some(ctx);
                    }
                }
                None
            });

            let _ = sender.send(AgentDomainEvent::Finished {
                outcome,
                message: result,
                total_cost_usd,
                num_turns,
                context_window,
            });
        }

        ClaudeEvent::ProcessError { message } => {
            let _ = sender.send(AgentDomainEvent::ProcessError(message));
        }

        // Assistant messages and Unknown are ignored (assistant content is
        // already delivered via stream events)
        _ => {}
    }
}

fn translate_stream_event(
    ev: &StreamEventData,
    active_block: &mut ActiveBlock,
    sender: &mpsc::Sender<AgentDomainEvent>,
) {
    match ev.event_type.as_str() {
        "message_start" => {
            // Extract total context usage from message.usage
            if let Some(ref msg) = ev.message
                && let Some(usage) = msg.get("usage")
            {
                let input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_create = usage
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let _ = sender.send(AgentDomainEvent::TokenUsage {
                    input_tokens: input,
                    output_tokens: 0,
                    cache_read_tokens: cache_read,
                    cache_write_tokens: cache_create,
                });
            }
        }

        "message_delta" => {
            if let Some(ref usage) = ev.usage {
                let _ = sender.send(AgentDomainEvent::TokenUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_read_tokens: usage.cache_read_input_tokens,
                    cache_write_tokens: usage.cache_creation_input_tokens,
                });
            }
        }

        "content_block_start" => {
            if let Some(ref cb) = ev.content_block {
                match cb.block_type.as_str() {
                    "text" => {
                        *active_block = ActiveBlock::Text {
                            accumulated: String::new(),
                        };
                        // No event needed — UI creates label on first TextDelta
                    }
                    "tool_use" => {
                        let id = cb.id.clone().unwrap_or_default();
                        let name = cb.name.clone().unwrap_or_else(|| "Tool".to_string());
                        let _ = sender.send(AgentDomainEvent::ToolStarted {
                            id: id.clone(),
                            name: name.clone(),
                        });
                        *active_block = ActiveBlock::ToolUse {
                            id,
                            name,
                            input: String::new(),
                        };
                    }
                    "thinking" => {
                        let _ = sender.send(AgentDomainEvent::ThinkingStarted);
                        *active_block = ActiveBlock::Thinking;
                    }
                    _ => {
                        *active_block = ActiveBlock::None;
                    }
                }
            }
        }

        "content_block_delta" => {
            if let Some(ref delta) = ev.delta {
                match delta.delta_type.as_deref() {
                    Some("text_delta") => {
                        if let Some(ref text) = delta.text {
                            if let ActiveBlock::Text { accumulated } = active_block {
                                accumulated.push_str(text);
                            }
                            let _ = sender.send(AgentDomainEvent::TextDelta(text.clone()));
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(ref json) = delta.partial_json {
                            if let ActiveBlock::ToolUse { input, .. } = active_block {
                                input.push_str(json);
                            }
                            let _ = sender.send(AgentDomainEvent::ToolInputDelta(json.clone()));
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(ref text) = delta.text {
                            let _ = sender.send(AgentDomainEvent::ThinkingDelta(text.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }

        "content_block_stop" => {
            let finished_block = std::mem::replace(active_block, ActiveBlock::None);
            match finished_block {
                ActiveBlock::Text { accumulated } => {
                    let _ = sender.send(AgentDomainEvent::TextBlockFinished {
                        full_text: accumulated,
                    });
                }
                ActiveBlock::ToolUse { id, name, input } => {
                    let _ = sender.send(AgentDomainEvent::ToolInputFinished {
                        id,
                        name,
                        input_json: input,
                    });
                }
                ActiveBlock::Thinking => {
                    let _ = sender.send(AgentDomainEvent::ThinkingFinished);
                }
                ActiveBlock::None => {}
            }
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// PTY creation
// ---------------------------------------------------------------------------

/// Create a PTY pair (master, slave) with raw mode on the slave.
/// A PTY forces Node.js to treat stdout as a TTY, which uses immediate
/// (unbuffered) writes instead of caching output in internal buffers.
fn create_pty_pair() -> Option<(OwnedFd, OwnedFd)> {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        if master < 0 {
            return None;
        }
        if libc::grantpt(master) != 0 || libc::unlockpt(master) != 0 {
            libc::close(master);
            return None;
        }
        let slave_name = libc::ptsname(master);
        if slave_name.is_null() {
            libc::close(master);
            return None;
        }
        let slave = libc::open(slave_name, libc::O_RDWR | libc::O_NOCTTY);
        if slave < 0 {
            libc::close(master);
            return None;
        }
        // Raw mode: disable terminal processing (CR/LF, echo, etc.)
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(slave, termios.as_mut_ptr()) == 0 {
            let mut t = termios.assume_init();
            libc::cfmakeraw(&mut t);
            libc::tcsetattr(slave, libc::TCSANOW, &t);
        }
        // Prevent master fd from leaking into child process
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        Some((OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)))
    }
}

// ---------------------------------------------------------------------------
// Reader threads
// ---------------------------------------------------------------------------

fn spawn_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    sender: mpsc::Sender<AgentDomainEvent>,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        let mut active_block = ActiveBlock::None;

        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => match serde_json::from_str::<ClaudeEvent>(&line) {
                    Ok(event) => {
                        translate_event(event, &mut active_block, &sender);
                    }
                    Err(e) => {
                        let preview: String = line.chars().take(200).collect();
                        eprintln!("flycrys: failed to parse agent JSON: {e} — {preview}");
                    }
                },
                Err(_) => break,
                _ => {}
            }
        }
    });
}

fn spawn_stderr_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    sender: mpsc::Sender<AgentDomainEvent>,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => {
                    if sender.send(AgentDomainEvent::ProcessError(line)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
                _ => {}
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Parse context window from model name
// ---------------------------------------------------------------------------

/// Parse context window size from model name (e.g., "claude-opus-4-6[1m]" -> 1_000_000)
fn parse_context_window(model: &str) -> Option<u64> {
    let start = model.find('[')?;
    let end = model.find(']')?;
    let spec = model[start + 1..end].to_lowercase();
    if let Some(num_str) = spec.strip_suffix('m') {
        num_str.parse::<u64>().ok().map(|n| n * 1_000_000)
    } else if let Some(num_str) = spec.strip_suffix('k') {
        num_str.parse::<u64>().ok().map(|n| n * 1_000)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// ClaudeBackend — the AgentBackend implementation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ProcessState {
    Idle,
    Running,
    Paused,
}

pub struct ClaudeBackend {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pid: Option<u32>,
    state: ProcessState,
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeBackend {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            pid: None,
            state: ProcessState::Idle,
        }
    }

    fn cleanup(&mut self) {
        self.stdin = None;
        if let Some(ref mut child) = self.child {
            let _ = child.wait();
        }
        self.child = None;
        self.pid = None;
        self.state = ProcessState::Idle;
    }
}

impl AgentBackend for ClaudeBackend {
    fn spawn(
        &mut self,
        sender: mpsc::Sender<AgentDomainEvent>,
        working_dir: &std::path::Path,
        config: &AgentSpawnConfig,
    ) -> Result<(), String> {
        let cwd = working_dir.to_path_buf();

        // Use a PTY for stdout so Node.js treats it as a TTY and flushes immediately
        let pty = create_pty_pair();
        let (pty_slave, pty_master) = match pty {
            Some((master, slave)) => (Some(slave), Some(master)),
            None => (None, None),
        };

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--dangerously-skip-permissions")
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped());

        // Agent profile options
        if let Some(ref prompt) = config.system_prompt
            && !prompt.is_empty()
        {
            cmd.arg("--system-prompt").arg(prompt);
        }
        for tool in &config.allowed_tools {
            cmd.arg("--allowedTools").arg(tool);
        }
        if let Some(ref model) = config.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(ref session_id) = config.resume_session_id {
            cmd.arg("--resume").arg(session_id);
        }

        if let Some(slave) = pty_slave {
            cmd.stdout(Stdio::from(slave));
        } else {
            cmd.stdout(Stdio::piped());
        }

        match cmd.spawn() {
            Ok(mut child) => {
                self.pid = Some(child.id());
                self.stdin = child.stdin.take();

                // Capture stderr for error reporting
                if let Some(stderr) = child.stderr.take() {
                    spawn_stderr_reader(stderr, sender.clone());
                }

                self.child = Some(child);
                self.state = ProcessState::Running;

                if let Some(master) = pty_master {
                    spawn_reader(std::fs::File::from(master), sender);
                } else {
                    let stdout = self.child.as_mut().unwrap().stdout.take().unwrap();
                    spawn_reader(stdout, sender);
                }
                Ok(())
            }
            Err(e) => Err(format!("{e}")),
        }
    }

    fn send_message(&mut self, text: &str, images: &[ImageAttachment]) -> Result<(), String> {
        let Some(ref mut stdin) = self.stdin else {
            return Err("Process stdin not available".to_string());
        };
        let content = if images.is_empty() {
            serde_json::json!(text)
        } else {
            let mut blocks: Vec<serde_json::Value> = Vec::new();
            for img in images {
                blocks.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": &img.media_type,
                        "data": general_purpose::STANDARD.encode(&img.bytes)
                    }
                }));
            }
            if !text.is_empty() {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": text
                }));
            }
            serde_json::json!(blocks)
        };
        let msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": content
            }
        });
        writeln!(stdin, "{}", msg).map_err(|e| format!("{e}"))?;
        stdin.flush().map_err(|e| format!("{e}"))?;
        Ok(())
    }

    fn pause(&mut self) {
        if let Some(pid) = self.pid
            && self.state == ProcessState::Running
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
            self.state = ProcessState::Paused;
        }
    }

    fn resume(&mut self) {
        if let Some(pid) = self.pid
            && self.state == ProcessState::Paused
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
            self.state = ProcessState::Running;
        }
    }

    fn stop(&mut self) {
        if let Some(pid) = self.pid {
            if self.state == ProcessState::Paused {
                unsafe {
                    libc::kill(pid as i32, libc::SIGCONT);
                }
            }
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
        self.cleanup();
    }

    fn is_alive(&self) -> bool {
        self.state != ProcessState::Idle && self.child.is_some()
    }

    fn is_paused(&self) -> bool {
        self.state == ProcessState::Paused
    }

    fn is_running(&self) -> bool {
        self.state == ProcessState::Running
    }
}

impl Drop for ClaudeBackend {
    fn drop(&mut self) {
        self.stop();
    }
}
