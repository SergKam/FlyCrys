use base64::{Engine as _, engine::general_purpose};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::io::{FromRawFd, OwnedFd};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum AgentEvent {
    #[serde(rename = "system")]
    System {
        subtype: Option<String>,
        session_id: Option<String>,
        model: Option<String>,
    },
    #[serde(rename = "stream_event")]
    StreamEvent { event: Box<StreamEventData> },
    #[serde(rename = "assistant")]
    Assistant { message: AssistantMessage },
    #[serde(rename = "user")]
    User {
        /// Can be a JSON object, a plain string, or absent — kept as Value
        /// because the Claude CLI sends different shapes depending on the tool.
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

pub struct ImageAttachment {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// Configuration for spawning a Claude CLI process
#[derive(Default)]
pub struct AgentSpawnConfig {
    pub system_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub resume_session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessState {
    Idle,
    Running,
    Paused,
}

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

fn spawn_reader<R: std::io::Read + Send + 'static>(reader: R, sender: mpsc::Sender<AgentEvent>) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => match serde_json::from_str::<AgentEvent>(&line) {
                    Ok(event) => {
                        if sender.send(event).is_err() {
                            break;
                        }
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
    sender: mpsc::Sender<AgentEvent>,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.is_empty() => {
                    let event = AgentEvent::ProcessError { message: line };
                    if sender.send(event).is_err() {
                        break;
                    }
                }
                Err(_) => break,
                _ => {}
            }
        }
    });
}

pub struct AgentProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pid: Option<u32>,
    pub state: ProcessState,
}

impl Default for AgentProcess {
    fn default() -> Self {
        Self::new()
    }
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

    pub fn send_message(&mut self, text: &str, images: &[ImageAttachment]) {
        if let Some(ref mut stdin) = self.stdin {
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
            let _ = writeln!(stdin, "{}", msg);
            let _ = stdin.flush();
        }
    }

    pub fn pause(&mut self) {
        if let Some(pid) = self.pid
            && self.state == ProcessState::Running
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGSTOP);
            }
            self.state = ProcessState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if let Some(pid) = self.pid
            && self.state == ProcessState::Paused
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGCONT);
            }
            self.state = ProcessState::Running;
        }
    }

    pub fn stop(&mut self) {
        if let Some(pid) = self.pid {
            // Resume first if paused, then interrupt
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
