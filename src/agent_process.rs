use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    #[serde(rename = "system")]
    System {
        subtype: Option<String>,
        session_id: Option<String>,
    },
    #[serde(rename = "stream_event")]
    StreamEvent {
        event: StreamEventData,
    },
    #[serde(rename = "assistant")]
    Assistant {
        message: AssistantMessage,
    },
    #[serde(rename = "user")]
    User {
        tool_use_result: Option<ToolUseResult>,
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
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StreamEventData {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub index: Option<u32>,
    pub content_block: Option<ContentBlock>,
    pub delta: Option<Delta>,
    pub message: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Delta {
    #[serde(rename = "type")]
    pub delta_type: Option<String>,
    pub text: Option<String>,
    pub partial_json: Option<String>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AssistantMessage {
    pub content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolUseResult {
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub is_error: bool,
    pub tool_use_id: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Idle,
    Running,
    Paused,
}

pub struct AgentProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pid: Option<u32>,
    pub state: ProcessState,
}

impl AgentProcess {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            pid: None,
            state: ProcessState::Idle,
        }
    }

    pub fn spawn(
        &mut self,
        sender: mpsc::Sender<AgentEvent>,
    ) -> bool {
        let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());

        let result = Command::new("claude")
            .arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--dangerously-skip-permissions")
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        match result {
            Ok(mut child) => {
                self.pid = Some(child.id());
                self.stdin = child.stdin.take();
                let stdout = child.stdout.take().unwrap();
                self.child = Some(child);
                self.state = ProcessState::Running;

                // Reader thread → glib channel
                std::thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        match line {
                            Ok(line) if !line.is_empty() => {
                                if let Ok(event) = serde_json::from_str::<AgentEvent>(&line) {
                                    if sender.send(event).is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(_) => break,
                            _ => {}
                        }
                    }
                });
                true
            }
            Err(_) => false,
        }
    }

    pub fn send_message(&mut self, text: &str) {
        if let Some(ref mut stdin) = self.stdin {
            let msg = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": text
                }
            });
            let _ = writeln!(stdin, "{}", msg);
            let _ = stdin.flush();
        }
    }

    pub fn pause(&mut self) {
        if let Some(pid) = self.pid {
            if self.state == ProcessState::Running {
                unsafe { libc::kill(pid as i32, libc::SIGSTOP); }
                self.state = ProcessState::Paused;
            }
        }
    }

    pub fn resume(&mut self) {
        if let Some(pid) = self.pid {
            if self.state == ProcessState::Paused {
                unsafe { libc::kill(pid as i32, libc::SIGCONT); }
                self.state = ProcessState::Running;
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(pid) = self.pid {
            // Resume first if paused, then interrupt
            if self.state == ProcessState::Paused {
                unsafe { libc::kill(pid as i32, libc::SIGCONT); }
            }
            unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        }
        self.cleanup();
    }

    pub fn is_alive(&self) -> bool {
        self.state != ProcessState::Idle && self.child.is_some()
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

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
