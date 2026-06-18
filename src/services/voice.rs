//! Voice dictation by mimicking the Claude Code CLI's speech-to-text path.
//!
//! The CLI streams microphone audio to a private Anthropic WebSocket endpoint
//! (`/api/ws/speech_to_text/voice_stream`, a Deepgram Nova-3 proxy) authenticated
//! with the Claude.ai OAuth token already stored in `~/.claude/.credentials.json`.
//! We reuse that exact protocol: capture 16 kHz mono PCM from a system recorder,
//! push it over the socket as binary frames, and surface the JSON transcripts.
//!
//! Caveats (intentional, documented): this is an undocumented endpoint reached
//! with subscription credentials, so it can break on any CLI release and is not a
//! supported API. We keep all the brittle assumptions in this one module.
//!
//! Threading follows the codebase convention (worker thread + `mpsc` + a GTK
//! `timeout_add_local` poll on the UI side): one thread owns the WebSocket and
//! emits [`VoiceEvent`]s; a helper thread drains the recorder's stdout so the
//! socket loop never blocks on the microphone.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tungstenite::Message;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;

/// Full STT WebSocket URL, including the Deepgram query params the CLI sends.
const VOICE_URL: &str = "wss://api.anthropic.com/api/ws/speech_to_text/voice_stream\
?encoding=linear16&sample_rate=16000&channels=1&endpointing_ms=300&utterance_end_ms=1000\
&language=en&use_conversation_engine=true&forward_interims=typed&stt_provider=deepgram-nova3";

/// User-Agent we present. Mirrors the CLI's `claude-cli/<ver> (external, cli)`
/// shape so the Cloudflare edge in front of the endpoint treats us like the CLI.
const USER_AGENT: &str = "claude-cli/2.1.175 (external, cli)";

/// KeepAlive cadence (the CLI uses an 8 s interval).
const KEEPALIVE: Duration = Duration::from_secs(8);

/// Hard safety cap on a single dictation, matching the CLI's 2-minute ceiling.
const MAX_RECORD: Duration = Duration::from_secs(120);

/// How long to keep reading for trailing finals after CloseStream.
const DRAIN_AFTER_CLOSE: Duration = Duration::from_secs(5);

/// Socket read timeout — small so the loop interleaves send/recv snappily.
const READ_TIMEOUT: Duration = Duration::from_millis(40);

/// Audio chunk size pulled from the recorder per read (~100 ms at 16 kHz·16-bit).
const AUDIO_CHUNK: usize = 3200;

/// Events emitted by the voice worker, consumed on the GTK main thread.
pub enum VoiceEvent {
    /// Socket connected and the server accepted our audio stream.
    Connected,
    /// Partial transcript for the in-progress utterance (replaces the previous
    /// interim). Suitable for a live, not-yet-committed preview.
    Interim(String),
    /// A finalized utterance segment — safe to commit into the input.
    Final(String),
    /// A fatal error (setup failure or stream error). The session is over.
    Error(String),
    /// The session has fully ended (socket closed, recorder stopped).
    Closed,
}

/// A live dictation session. Drop or call [`VoiceSession::stop`] to end it; the
/// worker then sends CloseStream, drains trailing finals, and emits `Closed`.
pub struct VoiceSession {
    stop: Arc<AtomicBool>,
}

impl VoiceSession {
    /// Request a graceful stop (flush remaining audio, collect final transcript).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for VoiceSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Start a dictation session. Spawns worker threads and returns immediately;
/// progress arrives as [`VoiceEvent`]s on `tx`. Returns the session handle whose
/// lifetime controls recording.
pub fn start(tx: Sender<VoiceEvent>) -> VoiceSession {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_worker = Arc::clone(&stop);
    std::thread::spawn(move || {
        if let Err(e) = run(&stop_worker, &tx) {
            let _ = tx.send(VoiceEvent::Error(e));
        }
        let _ = tx.send(VoiceEvent::Closed);
    });
    VoiceSession { stop }
}

/// Whether at least one supported recorder appears to be installed. Used to give
/// an upfront, actionable error instead of failing mid-connect.
pub fn recorder_available() -> bool {
    RECORDERS.iter().any(|(bin, _)| which(bin))
}

// --- internals -------------------------------------------------------------

/// Candidate recorders, in preference order, each producing raw signed 16-bit
/// little-endian mono PCM at 16 kHz on stdout.
const RECORDERS: &[(&str, &[&str])] = &[
    // PulseAudio / PipeWire-pulse.
    ("parec", &["--format=s16le", "--rate=16000", "--channels=1"]),
    // ALSA.
    (
        "arecord",
        &["-q", "-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "raw"],
    ),
    // SoX.
    (
        "rec",
        &[
            "-q",
            "-t",
            "raw",
            "-b",
            "16",
            "-e",
            "signed-integer",
            "-r",
            "16000",
            "-c",
            "1",
            "-",
        ],
    ),
    // ffmpeg (PulseAudio source).
    (
        "ffmpeg",
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "pulse",
            "-i",
            "default",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "s16le",
            "-",
        ],
    ),
];

fn which(bin: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {bin}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn spawn_recorder() -> Result<Child, String> {
    for (bin, args) in RECORDERS {
        if !which(bin) {
            continue;
        }
        match Command::new(bin)
            .args(*args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(_) => continue,
        }
    }
    Err(
        "No microphone recorder found. Install one of: pipewire-pulse/pulseaudio \
         (parec), alsa-utils (arecord), sox (rec), or ffmpeg."
            .to_string(),
    )
}

/// Read the Claude.ai OAuth access token from the CLI's credentials file.
fn load_token() -> Result<String, String> {
    let path = dirs::home_dir()
        .ok_or("Cannot locate home directory")?
        .join(".claude")
        .join(".credentials.json");
    let data = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("Cannot parse credentials: {e}"))?;
    let oauth = json.get("claudeAiOauth").ok_or(
        "No Claude.ai OAuth token found. Voice input needs a Claude.ai login \
         (run `claude` and sign in; API-key/Bedrock/Vertex auth is not supported).",
    )?;
    let token = oauth
        .get("accessToken")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or("Claude.ai OAuth token is missing an access token.")?;

    // Best-effort expiry check: warn loudly rather than fail on clock skew.
    if let Some(expires_at) = oauth.get("expiresAt").and_then(|v| v.as_i64())
        && let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH)
        && (now.as_millis() as i64) >= expires_at
    {
        return Err("Claude.ai login has expired. Run any `claude` command to \
                    refresh it, then try voice input again."
            .to_string());
    }

    Ok(token.to_string())
}

/// rustls 0.23 needs a process-wide crypto provider chosen explicitly. Install
/// `ring` once (idempotent across sessions).
fn ensure_crypto() {
    static CRYPTO: Once = Once::new();
    CRYPTO.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

fn run(stop: &Arc<AtomicBool>, tx: &Sender<VoiceEvent>) -> Result<(), String> {
    ensure_crypto();
    let token = load_token()?;

    // Build the upgrade request: standard WS handshake headers (from the URL)
    // plus the CLI's auth/identity headers.
    let mut request = VOICE_URL
        .into_client_request()
        .map_err(|e| format!("Bad voice URL: {e}"))?;
    {
        let headers = request.headers_mut();
        let bearer = format!("Bearer {token}")
            .parse()
            .map_err(|_| "Invalid auth header")?;
        headers.insert("authorization", bearer);
        headers.insert("user-agent", USER_AGENT.parse().unwrap());
        headers.insert("x-app", "cli".parse().unwrap());
        headers.insert("anthropic-client-platform", "cli".parse().unwrap());
    }

    let (mut socket, response) =
        tungstenite::connect(request).map_err(|e| format!("Voice connect failed: {e}"))?;
    let _ = response;

    // A short read timeout turns the blocking socket into a pollable one so the
    // single loop can interleave audio sends, keepalives, and transcript reads.
    set_read_timeout(&mut socket, Some(READ_TIMEOUT));

    // Initial KeepAlive (the CLI sends one immediately on open).
    socket
        .send(Message::text("{\"type\":\"KeepAlive\"}"))
        .map_err(|e| format!("Voice send failed: {e}"))?;
    let _ = tx.send(VoiceEvent::Connected);

    // Start capturing and hand the recorder's stdout to a drainer thread.
    let mut recorder = spawn_recorder()?;
    let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let mut stdout = recorder
        .stdout
        .take()
        .ok_or("Recorder produced no output stream")?;
    let stop_reader = Arc::clone(stop);
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; AUDIO_CHUNK];
        loop {
            if stop_reader.load(Ordering::SeqCst) {
                break;
            }
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if audio_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let outcome = pump(&mut socket, &audio_rx, stop, tx);

    // Tear down: stop the recorder and its drainer thread.
    let _ = recorder.kill();
    let _ = recorder.wait();
    drop(audio_rx);
    let _ = reader.join();
    let _ = socket.close(None);

    outcome
}

/// The main send/receive loop. Owns the socket; runs until stop is requested and
/// trailing finals are drained (or a fatal error / safety cap intervenes).
fn pump(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    audio_rx: &Receiver<Vec<u8>>,
    stop: &Arc<AtomicBool>,
    tx: &Sender<VoiceEvent>,
) -> Result<(), String> {
    let started = Instant::now();
    let mut last_keepalive = Instant::now();
    let mut closing = false;
    let mut close_deadline = Instant::now();

    loop {
        // Safety cap on total recording time.
        if !closing && started.elapsed() >= MAX_RECORD {
            stop.store(true, Ordering::SeqCst);
        }

        // Transition to closing: flush a CloseStream, then keep reading for the
        // server's trailing transcripts until it closes or we time out.
        if !closing && stop.load(Ordering::SeqCst) {
            // Drain any audio still queued before closing the stream.
            while let Ok(chunk) = audio_rx.try_recv() {
                let _ = socket.send(Message::binary(chunk));
            }
            socket
                .send(Message::text("{\"type\":\"CloseStream\"}"))
                .map_err(|e| format!("Voice close failed: {e}"))?;
            closing = true;
            close_deadline = Instant::now() + DRAIN_AFTER_CLOSE;
        }

        if closing && Instant::now() >= close_deadline {
            break;
        }

        // Forward queued audio (only while actively recording).
        if !closing {
            match audio_rx.try_recv() {
                Ok(chunk) => {
                    socket
                        .send(Message::binary(chunk))
                        .map_err(|e| format!("Voice send failed: {e}"))?;
                }
                Err(TryRecvError::Disconnected) => stop.store(true, Ordering::SeqCst),
                Err(TryRecvError::Empty) => {}
            }

            if last_keepalive.elapsed() >= KEEPALIVE {
                socket
                    .send(Message::text("{\"type\":\"KeepAlive\"}"))
                    .map_err(|e| format!("Voice keepalive failed: {e}"))?;
                last_keepalive = Instant::now();
            }
        }

        // Read one message (or time out). WouldBlock/TimedOut just means "nothing
        // yet" — keep looping.
        match socket.read() {
            Ok(Message::Text(s)) => {
                if handle_message(&s, closing, tx) == Some(true) {
                    break;
                }
            }
            Ok(Message::Binary(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Ok(Message::Frame(_)) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                break;
            }
            Err(e) => return Err(format!("Voice stream error: {e}")),
        }
    }

    Ok(())
}

/// Parse one server message. Returns `Some(true)` when the stream is finished
/// (final endpoint after CloseStream), `Some(false)`/`None` to keep going.
fn handle_message(raw: &str, closing: bool, tx: &Sender<VoiceEvent>) -> Option<bool> {
    let val: serde_json::Value = serde_json::from_str(raw).ok()?;
    match val.get("type").and_then(|v| v.as_str())? {
        "TranscriptInterim" | "TranscriptText" => {
            if let Some(text) = val.get("data").and_then(|v| v.as_str())
                && !text.is_empty()
            {
                let _ = tx.send(VoiceEvent::Interim(text.to_string()));
            }
            Some(false)
        }
        "TranscriptEndpoint" => {
            if let Some(text) = val.get("data").and_then(|v| v.as_str())
                && !text.is_empty()
            {
                let _ = tx.send(VoiceEvent::Final(text.to_string()));
            }
            // After CloseStream, the endpoint marks the end of the final segment.
            Some(closing)
        }
        "TranscriptError" => {
            let msg = val
                .get("description")
                .or_else(|| val.get("error_code"))
                .and_then(|v| v.as_str())
                .unwrap_or("transcription error");
            let _ = tx.send(VoiceEvent::Error(msg.to_string()));
            Some(true)
        }
        "error" => {
            let msg = val
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("voice stream error");
            let _ = tx.send(VoiceEvent::Error(msg.to_string()));
            Some(true)
        }
        _ => Some(false),
    }
}

/// Set the read timeout on the underlying TCP stream (works for plain or TLS).
fn set_read_timeout(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    timeout: Option<Duration>,
) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(timeout);
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.get_ref().set_read_timeout(timeout);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn interim_message_emits_interim_and_continues() {
        let (tx, rx) = mpsc::channel();
        let done = handle_message(
            r#"{"type":"TranscriptText","data":"hello world"}"#,
            false,
            &tx,
        );
        assert_eq!(done, Some(false));
        match rx.try_recv() {
            Ok(VoiceEvent::Interim(t)) => assert_eq!(t, "hello world"),
            other => panic!(
                "expected Interim, got something else: {}",
                event_name(&other)
            ),
        }
    }

    #[test]
    fn endpoint_finalizes_and_ends_only_after_close() {
        let (tx, rx) = mpsc::channel();
        // Before CloseStream: emits Final but keeps the stream open.
        assert_eq!(
            handle_message(
                r#"{"type":"TranscriptEndpoint","data":"a sentence"}"#,
                false,
                &tx
            ),
            Some(false)
        );
        assert!(matches!(rx.try_recv(), Ok(VoiceEvent::Final(t)) if t == "a sentence"));
        // After CloseStream: the endpoint ends the loop.
        assert_eq!(
            handle_message(r#"{"type":"TranscriptEndpoint","data":"tail"}"#, true, &tx),
            Some(true)
        );
        assert!(matches!(rx.try_recv(), Ok(VoiceEvent::Final(t)) if t == "tail"));
    }

    #[test]
    fn error_frames_emit_error_and_stop() {
        let (tx, rx) = mpsc::channel();
        assert_eq!(
            handle_message(
                r#"{"type":"TranscriptError","description":"boom"}"#,
                false,
                &tx
            ),
            Some(true)
        );
        assert!(matches!(rx.try_recv(), Ok(VoiceEvent::Error(e)) if e == "boom"));
    }

    #[test]
    fn unknown_and_malformed_messages_are_ignored() {
        let (tx, _rx) = mpsc::channel();
        assert_eq!(
            handle_message(r#"{"type":"Heartbeat"}"#, false, &tx),
            Some(false)
        );
        assert_eq!(handle_message("not json", false, &tx), None);
        assert_eq!(handle_message(r#"{"no":"type"}"#, false, &tx), None);
    }

    #[test]
    fn empty_data_does_not_emit() {
        let (tx, rx) = mpsc::channel();
        assert_eq!(
            handle_message(r#"{"type":"TranscriptText","data":""}"#, false, &tx),
            Some(false)
        );
        assert!(rx.try_recv().is_err());
    }

    fn event_name(e: &Result<VoiceEvent, mpsc::TryRecvError>) -> &'static str {
        match e {
            Ok(VoiceEvent::Connected) => "Connected",
            Ok(VoiceEvent::Interim(_)) => "Interim",
            Ok(VoiceEvent::Final(_)) => "Final",
            Ok(VoiceEvent::Error(_)) => "Error",
            Ok(VoiceEvent::Closed) => "Closed",
            Err(_) => "Empty",
        }
    }
}
