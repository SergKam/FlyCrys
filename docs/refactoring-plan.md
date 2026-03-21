# Refactoring Plan: From Organic Feature Factory to Sustainable Architecture

**Created**: 2026-03-21
**Goal**: Clean layered architecture where UI, domain logic, CLI integration, and platform
services are separated. No magic numbers. No boolean-where-enum-belongs. Extensible patterns.
Linux-only but distro-aware.

**Non-goal**: Supporting macOS, Windows, or alternative CLI backends right now. The abstraction
is for code hygiene and testability, not for hypothetical portability.

---

## Current State (Honest Assessment)

```
src/
  main.rs .............. 665 lines  - app bootstrap + theme + tab management + settings UI
  workspace.rs ......... 946 lines  - workspace construction + file ops + editors + browsers
  agent_panel.rs ....... 1268 lines - THE GOD MODULE: UI + events + process + tokens + state
  agent_process.rs ..... 390 lines  - Claude CLI process + protocol + PTY
  agent_widgets.rs ..... 301 lines  - chat message widget factory
  session.rs ........... 319 lines  - data models + persistence + defaults (mixed)
  git_panel.rs ......... ~200 lines - git status UI + direct git CLI calls
  textview.rs .......... 323 lines  - file viewer + preview dispatch
  highlight.rs ......... ~225 lines - syntax highlighting + extension mapping
  markdown.rs .......... ~250 lines - markdown to pango
  terminal.rs .......... ~50 lines  - VTE terminal wrapper
  tree.rs .............. ~200 lines - file tree widget
  watcher.rs ........... 134 lines  - file system watcher
  agent_config_dialog.rs ~350 lines - agent profile CRUD dialog
  file_entry.rs ........ ~30 lines  - file metadata struct
  lib.rs ............... 14 lines   - flat module re-exports
```

**Key problems**:
- `agent_panel.rs` is a 1268-line god module with a 28-field god struct (`PanelState`)
  and a 657-line constructor function
- `create_agent_panel()` mixes UI construction, event handling, process management,
  token tracking, and theme logic in one closure-heavy function
- Claude CLI protocol details leak from `agent_process.rs` into `agent_panel.rs`
  (event type matching, token field names, context window parsing)
- `workspace.rs` directly spawns shell commands (xdg-open, editor list, browser list)
- Data models, defaults, persistence, and file paths are all in `session.rs`
- 19+ magic numbers scattered across files with no central registry
- 6 boolean fields that limit future extensibility
- Hardcoded lists (editors, browsers, quick commands, file extensions) that belong in config

---

## Target Architecture

```
src/
  main.rs                          - app bootstrap only, delegates everything
  lib.rs                           - module tree with layered re-exports

  config/                          - LAYER 0: constants, types, configuration
    mod.rs
    constants.rs                   - all magic numbers, thresholds, defaults
    types.rs                       - domain enums (Theme, ViewMode, DiffMode, etc.)
    theme.rs                       - CSS generation from theme definition

  models/                          - LAYER 1: pure data structures (no I/O, no GTK)
    mod.rs
    app_config.rs                  - AppConfig struct
    workspace_config.rs            - WorkspaceConfig struct
    agent_config.rs                - AgentConfig struct
    chat.rs                        - ChatMessage, conversation types
    agent_events.rs                - domain events (AgentStarted, TextChunk, ToolCall, etc.)

  services/                        - LAYER 2: business logic, I/O, no GTK
    mod.rs
    storage.rs                     - config/session persistence (file I/O)
    cli/
      mod.rs                       - trait AgentBackend + domain event translation
      claude.rs                    - Claude CLI: spawn, protocol, wire types, event mapping
    platform.rs                    - xdg-open, editor detection, shell, config paths
    git.rs                         - git CLI operations (status, diff, log)

  ui/                              - LAYER 3: GTK widgets, presentation only
    mod.rs
    workspace.rs                   - workspace container, panel layout
    agent_panel/
      mod.rs                       - panel construction + wiring
      state.rs                     - AgentPanelState (UI-only fields)
      event_handler.rs             - domain event -> UI update dispatch
      input.rs                     - text input, image attach, quick commands
      token_display.rs             - context/cost bar rendering
    agent_widgets.rs               - chat message widget factory (mostly unchanged)
    agent_config_dialog.rs         - agent profile CRUD dialog
    git_panel.rs                   - git status/diff UI (calls services/git.rs)
    textview.rs                    - file viewer
    file_tree.rs                   - file tree widget
    terminal.rs                    - VTE terminal wrapper
    highlight.rs                   - syntax highlighting
    markdown.rs                    - markdown rendering
    watcher.rs                     - file system watcher
```

**Dependency rule**: each layer may only depend on layers with lower numbers.
`ui/` -> `services/` -> `models/` -> `config/`. Never the reverse.

---

## Phase 1: Foundation (config/ + models/)

Extract the bedrock that everything else will stand on. No behavior changes.

### 1.1 Create `config/constants.rs`

Centralize every magic number currently scattered across the codebase.

```rust
// --- Timing ---
pub const AUTOSAVE_INTERVAL_SECS: u64 = 5;
pub const GIT_REFRESH_INTERVAL_SECS: u64 = 5;
pub const FILE_WATCHER_SYNC_MS: u64 = 200;

// --- UI Dimensions ---
pub const DEFAULT_WINDOW_WIDTH: i32 = 1200;
pub const DEFAULT_WINDOW_HEIGHT: i32 = 800;
pub const AGENT_PANEL_MIN_WIDTH: i32 = 420;
pub const TREE_PANE_DEFAULT_WIDTH: i32 = 300;
pub const INPUT_MAX_HEIGHT: i32 = 120;
pub const IMAGE_THUMBNAIL_WIDTH: i32 = 160;
pub const IMAGE_THUMBNAIL_HEIGHT: i32 = 120;
pub const AUTO_SCROLL_THRESHOLD: f64 = 20.0;

// --- Terminal ---
pub const TERMINAL_SCROLLBACK_LINES: i64 = 10_000;
pub const TERMINAL_FONT: &str = "Monospace 11";
pub const DEFAULT_SHELL: &str = "/bin/bash";

// --- Agent ---
pub const DEFAULT_CONTEXT_WINDOW: u64 = 200_000;

// --- Text/Display ---
pub const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
pub const DISPLAY_TRUNCATE_AT: usize = 60;
pub const DISPLAY_TRUNCATE_KEEP: usize = 57;
pub const OUTPUT_COLLAPSE_THRESHOLD: usize = 2000;
pub const OUTPUT_HEAD_TAIL_LINES: usize = 5;

// --- Chat History Pagination ---
pub const CHAT_TAIL_COUNT: usize = 20;
pub const CHAT_BATCH_SIZE: usize = 20;

// --- File Tree ---
pub const TREE_MAX_EXPAND_PASSES: usize = 30;

// --- Gutter ---
pub const GUTTER_CHAR_WIDTH_PX: i32 = 10;
pub const GUTTER_PADDING_PX: i32 = 12;
```

Each constant has a descriptive name. Changing a threshold is one line in one file.

### 1.2 Create `config/types.rs`

Replace booleans with enums. This is not pedantry — it's preventing the "oh we need a
third state" refactoring nightmare later.

```rust
/// Visual theme. Currently two variants, but structured for future extension
/// (e.g., HighContrast, Solarized, user-defined).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn is_dark(&self) -> bool { matches!(self, Theme::Dark) }
    pub fn toggle(&self) -> Self {
        match self { Theme::Light => Theme::Dark, Theme::Dark => Theme::Light }
    }
}

/// How to display the current file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Source,
    Preview,
}

/// Whether diff overlay is active in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiffMode {
    #[default]
    Hidden,
    Visible,
}

/// Notification preferences. Boolean today, but ready for levels like
/// ErrorsOnly, All, None, or per-agent granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotificationLevel {
    #[default]
    All,
    Disabled,
}

/// Agent completion result — not just error/success.
/// Could later include Cancelled, TimedOut, ContextExhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOutcome {
    Success,
    Error,
}

/// What kind of item in the file tree context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
    File,
    Directory,
}
```

### 1.3 Create `config/theme.rs`

Move CSS generation out of `main.rs` (lines 13-72). The theme module takes a `Theme`
enum value and returns the CSS string. Color values become named constants inside this
module. Later, this enables user-defined themes.

### 1.4 Extract `models/` from `session.rs`

Split `session.rs` (currently 319 lines of mixed concerns) into pure data structs:

- `models/app_config.rs` — `AppConfig` (uses `Theme` instead of `is_dark: bool`,
   `NotificationLevel` instead of `notifications_enabled: bool`)
- `models/workspace_config.rs` — `WorkspaceConfig` (uses `ViewMode` instead of
   `preview_mode: bool`, `DiffMode` instead of `show_diff: bool`)
- `models/agent_config.rs` — `AgentConfig` (unchanged for now)
- `models/chat.rs` — `ChatMessage` enum (unchanged)

**Serde stays on the structs** — it's a derive macro, not a dependency. The models
remain pure data. What moves OUT is all the `fn load_*`, `fn save_*`, `fn config_dir()`
logic — that goes to `services/storage.rs` in Phase 2.

### 1.5 Backward-compatible serde for enum fields

For config file compatibility, use serde aliases so old `"is_dark": true` still loads:

```rust
// In AppConfig
#[serde(alias = "is_dark")]
pub theme: Theme,
```

With a custom deserializer that maps `true -> Dark`, `false -> Light` for the migration
period. Remove after one release cycle.

---

## Phase 2: Services Layer (business logic without GTK)

### 2.1 Create `services/storage.rs`

Move ALL file I/O out of `session.rs`:

```rust
pub fn config_dir() -> PathBuf { ... }       // uses `dirs` crate
pub fn sessions_dir() -> PathBuf { ... }
pub fn agents_dir() -> PathBuf { ... }

pub fn load_app_config() -> AppConfig { ... }
pub fn save_app_config(config: &AppConfig) { ... }
pub fn load_workspace(id: &str) -> Option<WorkspaceConfig> { ... }
pub fn save_workspace(config: &WorkspaceConfig) { ... }
pub fn delete_workspace(id: &str) { ... }
pub fn load_chat_history(workspace_id: &str) -> Vec<ChatMessage> { ... }
pub fn save_chat_history(workspace_id: &str, messages: &[ChatMessage]) { ... }
pub fn list_agent_configs() -> Vec<AgentConfig> { ... }
pub fn save_agent_config(config: &AgentConfig) { ... }
pub fn delete_agent_config(name: &str) { ... }
pub fn ensure_default_agents() { ... }
```

**Key change**: Use the `dirs` crate for config paths instead of hardcoded
`$HOME/.config/flycrys`. This handles XDG_CONFIG_HOME on all Linux distros,
and is one less thing to break if someone runs on NixOS or a custom setup.

```rust
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("flycrys")
}
```

### 2.2 Create `services/cli/` — the CLI abstraction

This is the main structural change. The goal is NOT to support multiple backends
simultaneously. It's to ensure that `agent_panel.rs` never sees a Claude-specific type.

#### `services/cli/mod.rs` — the trait and domain events

```rust
/// Domain events that the UI layer consumes. CLI-agnostic.
pub enum AgentDomainEvent {
    Started {
        session_id: Option<String>,
        model: String,
        context_window: Option<u64>,
    },
    TextDelta(String),
    TextBlockFinished(String),
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
    },
    ProcessError(String),
}

/// Configuration for spawning an agent. CLI-agnostic fields.
pub struct AgentSpawnConfig {
    pub working_dir: PathBuf,
    pub system_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub resume_session_id: Option<String>,
}

/// What the UI layer uses to control an agent process.
pub trait AgentBackend {
    fn spawn(
        &mut self,
        config: &AgentSpawnConfig,
        sender: mpsc::Sender<AgentDomainEvent>,
    ) -> Result<(), String>;

    fn send_message(&mut self, text: &str, images: &[ImageAttachment]) -> Result<(), String>;
    fn pause(&self);
    fn resume(&self);
    fn stop(&mut self);
    fn is_alive(&self) -> bool;
}
```

#### `services/cli/claude.rs` — Claude CLI implementation

Absorbs from current `agent_process.rs`:
- `Command::new("claude")` and all argument construction
- All wire types: current `AgentEvent`, `StreamEventData`, `ContentBlock`, `Delta`,
  `StreamUsage` — these become **private** to this module
- JSON stdin message format
- The `spawn_reader` / `spawn_stderr_reader` functions
- PTY creation (platform-specific but tied to process spawning)
- Translation logic: `AgentEvent` -> `AgentDomainEvent`

The key transformation: `handle_stream_event()` (currently 156 lines in `agent_panel.rs`
matching on `"content_block_start"`, `"content_block_delta"`, etc.) moves INTO `claude.rs`.
The claude module parses its own wire format and emits domain events.

**What stays private to `claude.rs`:**
- `"message_start"`, `"message_delta"`, `"content_block_start"`, `"content_block_delta"`,
  `"content_block_stop"` — all Claude API event type strings
- `cache_creation_input_tokens`, `cache_read_input_tokens` — Claude-specific token fields
- `parse_context_window("claude-opus-4-6[1m]")` — Claude model naming convention
- `--output-format stream-json`, `--dangerously-skip-permissions` — CLI flags
- `extract_tool_display()`, `extract_file_path()` — tool param extraction

**What the UI sees**: `AgentDomainEvent::TextDelta("hello")`, not
`StreamEventData { event_type: "content_block_delta", delta: Some(Delta { text: ... }) }`.

### 2.3 Create `services/platform.rs`

Consolidate all OS/distro interaction that's currently scattered in `workspace.rs`:

```rust
/// Open a file with the desktop's default handler.
/// Uses xdg-open on FreeDesktop-compliant systems.
pub fn open_with_default(path: &Path) -> Result<(), String> { ... }

/// Open a URL in the user's preferred browser.
/// Checks: $BROWSER -> xdg-settings -> gtk-launch -> known browsers.
pub fn open_in_browser(uri: &str) -> Result<(), String> { ... }

/// Open a file in a text editor.
/// Checks: $VISUAL -> $EDITOR -> known editors -> xdg-open fallback.
pub fn open_in_editor(path: &Path) -> Result<(), String> { ... }

/// Get the user's default shell.
pub fn default_shell() -> String { ... }
```

The lists of known editors and browsers become arrays in `config/constants.rs`:

```rust
pub const KNOWN_EDITORS: &[&str] = &[
    "gnome-text-editor", "gedit", "kate", "code",
    "xed", "pluma", "mousepad",
];

pub const KNOWN_BROWSERS: &[&str] = &[
    "sensible-browser", "x-www-browser",
    "google-chrome", "chromium", "chromium-browser", "firefox",
];
```

### 2.4 Create `services/git.rs`

Extract git CLI operations from `git_panel.rs`:

```rust
pub struct GitStatus {
    pub entries: Vec<GitStatusEntry>,
}

pub struct GitStatusEntry {
    pub path: String,
    pub status: GitFileStatus,  // enum, not string
}

pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unknown(String),
}

pub fn status(repo_path: &Path) -> Result<GitStatus, String> { ... }
pub fn diff_file(repo_path: &Path, file: &str) -> Result<String, String> { ... }
pub fn diff_staged(repo_path: &Path) -> Result<String, String> { ... }
pub fn is_file_modified(repo_path: &Path, file: &str) -> bool { ... }
```

`git_panel.rs` (UI) calls `services::git::status()` instead of spawning `git` directly.
The hardcoded status code matching (`"M"`, `"??"`, etc.) moves into `services/git.rs`
where it's parsed into the `GitFileStatus` enum — making it extensible and testable.

---

## Phase 3: Break the God Module (`agent_panel.rs`)

This is the hardest phase. The 1268-line `agent_panel.rs` with its 28-field `PanelState`
and 657-line constructor needs to be decomposed.

### 3.1 Split `PanelState` into focused structs

Current `PanelState` has 28 fields spanning 5 responsibilities. Split into:

```rust
/// In ui/agent_panel/state.rs

/// Process-related state
pub struct AgentProcessState {
    pub backend: Box<dyn AgentBackend>,    // was: process: AgentProcess
    pub session_id: Option<String>,
    pub working_dir: PathBuf,
}

/// Token and cost tracking
pub struct TokenState {
    pub context_tokens: u64,
    pub context_window_max: u64,
    pub total_cost_usd: f64,
    pub context_label: gtk::Label,
    pub cost_label: gtk::Label,
}

/// Chat rendering state
pub struct ChatState {
    pub current_text_label: Option<gtk::Label>,
    pub current_tool_name: Option<String>,
    pub current_tool_input: String,
    pub pending_tools: HashMap<String, gtk::Box>,
    pub thinking_spinner: Option<gtk::Box>,
    pub chat_history: Vec<ChatMessage>,
    pub history_loaded_count: usize,
}

/// Panel configuration
pub struct PanelConfig {
    pub agent_configs: Vec<AgentConfig>,
    pub selected_profile_idx: usize,
    pub theme: Rc<Cell<Theme>>,
    pub notification_level: Rc<Cell<NotificationLevel>>,
}

/// Top-level panel state composes the above
pub struct AgentPanelState {
    pub process: AgentProcessState,
    pub tokens: TokenState,
    pub chat: ChatState,
    pub config: PanelConfig,
    // callbacks
    pub on_open_file: Option<Box<dyn Fn(&str)>>,
    pub on_tool_result: Option<Box<dyn Fn()>>,
}
```

### 3.2 Extract `ui/agent_panel/event_handler.rs`

The 200-line `handle_event()` and 156-line `handle_stream_event()` closures move here.
But now they consume `AgentDomainEvent` (not `AgentEvent`), so the logic is simpler:

```rust
pub fn handle_domain_event(
    state: &mut AgentPanelState,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
    event: AgentDomainEvent,
) {
    match event {
        AgentDomainEvent::Started { session_id, model, context_window } => { ... }
        AgentDomainEvent::TextDelta(text) => { ... }
        AgentDomainEvent::ToolStarted { id, name } => { ... }
        AgentDomainEvent::ToolResult { id, output, is_error } => { ... }
        AgentDomainEvent::TokenUsage { .. } => { ... }
        AgentDomainEvent::Finished { outcome, .. } => { ... }
        // ... each arm is 10-20 lines, not 50-80
    }
}
```

No more matching on `"content_block_delta"` strings. No more
`cache_creation_input_tokens`. The UI speaks domain language.

### 3.3 Extract `ui/agent_panel/input.rs`

The input area construction, image attachment logic, quick command menu, and
send-message wiring. Currently ~150 lines buried inside `create_agent_panel()`.

### 3.4 Extract `ui/agent_panel/token_display.rs`

Context bar + cost display update logic. Small module (~50 lines) but clearly
its own concern.

---

## Phase 4: Cleanup Remaining UI Modules

### 4.1 Slim down `main.rs`

Current `build_ui()` is 372 lines. Extract:
- Theme setup -> calls `config/theme.rs`
- Settings popover construction -> `ui/settings_popover.rs`
- Tab management logic (new tab, restore tabs, close tab) -> stays but calls helpers
- Autosave timer uses `constants::AUTOSAVE_INTERVAL_SECS`

### 4.2 Slim down `workspace.rs`

After `services/platform.rs` and `services/git.rs` are extracted:
- `open_in_text_editor()` -> one-liner: `platform::open_in_editor(path)`
- `open_in_browser()` -> one-liner: `platform::open_in_browser(uri)`
- `xdg-open` calls -> `platform::open_with_default(path)`
- Direct git calls -> `services::git::*`

### 4.3 Make extension mappings data-driven

`highlight.rs` lines 189-225 (the giant match on file extensions) and
`textview.rs` lines 39-42 (preview kind mapping) should use static maps:

```rust
// In config/constants.rs or a dedicated file_types.rs
use std::collections::HashMap;
use std::sync::LazyLock;

pub static SYNTAX_MAP: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    HashMap::from([
        ("rs", "Rust"),
        ("js", "JavaScript"),
        ("ts", "TypeScript"),
        // ... all extensions
    ])
});

pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "mdx"];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];
pub const SUPPORTED_IMAGE_MIME: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];
```

### 4.4 Quick commands as data

The hardcoded quick menu in `agent_panel.rs` becomes a configurable list:

```rust
// In config/constants.rs
pub struct QuickCommand {
    pub label: &'static str,
    pub action_name: &'static str,
    pub prompt: &'static str,
}

pub const QUICK_COMMANDS: &[QuickCommand] = &[
    QuickCommand {
        label: "Commit changes",
        action_name: "panel.quick-commit",
        prompt: "/commit",
    },
    // ...
];
```

---

## Phase 5: Hardcoded Values Audit

After phases 1-4, do a final sweep. Every literal in the codebase should be either:

1. **In `config/constants.rs`** — if it's a tunable value
2. **In a static map/array** — if it's a lookup table
3. **Truly local** — like a loop variable `0..len` or a format string that's used once

### Checklist of known values to centralize

| Current Location | Value | Target |
|---|---|---|
| `main.rs:561` | `Duration::from_secs(5)` | `AUTOSAVE_INTERVAL_SECS` |
| `workspace.rs:474` | `420` | `AGENT_PANEL_MIN_WIDTH` |
| `workspace.rs:131` | `20.0` | `AUTO_SCROLL_THRESHOLD` |
| `workspace.rs:912` | `30` iterations | `TREE_MAX_EXPAND_PASSES` |
| `agent_panel.rs:169` | `120` | `INPUT_MAX_HEIGHT` |
| `agent_panel.rs:130` | `20.0` | `AUTO_SCROLL_THRESHOLD` |
| `agent_widgets.rs:90` | `60`/`57` | `DISPLAY_TRUNCATE_AT/KEEP` |
| `agent_widgets.rs:184` | `2000` | `OUTPUT_COLLAPSE_THRESHOLD` |
| `agent_widgets.rs:187` | `5` head/tail | `OUTPUT_HEAD_TAIL_LINES` |
| `agent_widgets.rs:251` | `160x120` | `IMAGE_THUMBNAIL_*` |
| `terminal.rs:28` | `10000` | `TERMINAL_SCROLLBACK_LINES` |
| `terminal.rs:30` | `"Monospace 11"` | `TERMINAL_FONT` |
| `git_panel.rs:147` | `Duration::from_secs(5)` | `GIT_REFRESH_INTERVAL_SECS` |
| `watcher.rs:54` | `Duration::from_millis(200)` | `FILE_WATCHER_SYNC_MS` |
| `textview.rs:315` | `10`/`12` px | `GUTTER_CHAR_WIDTH_PX/PADDING_PX` |
| `session.rs:40-41` | `1200x800` | `DEFAULT_WINDOW_WIDTH/HEIGHT` |
| `session.rs:78-80` | `300`, `-1`, `420` | pane defaults |

---

## Execution Order & Risk Assessment

| Phase | Risk | Reason | Estimated Effort |
|-------|------|--------|------------------|
| **1.1** constants.rs | Minimal | Pure extraction, no behavior change | Small |
| **1.2** types.rs (enums) | Low | Serde compat needs care, but mechanical | Small |
| **1.3** theme.rs | Low | Move code, test CSS output | Small |
| **1.4** models/ extraction | Low | Struct moves, import updates | Small |
| **2.1** storage.rs | Low | Move functions, add `dirs` crate | Medium |
| **2.3** platform.rs | Low | Move functions, add `open` crate | Small |
| **2.4** git.rs service | Low | Extract + add GitFileStatus enum | Medium |
| **2.2** CLI abstraction | **Medium** | Core architectural change, needs careful testing | Large |
| **3.x** agent_panel split | **Medium-High** | Lots of closure rewiring, state threading | Large |
| **4.x** cleanup | Low | Mechanical after phases 1-3 | Medium |
| **5** audit | Minimal | Search & replace | Small |

**Recommended sequence**: 1.1 -> 1.2 -> 1.4 -> 2.1 -> 2.3 -> 2.4 -> 1.3 -> 4.3 -> 4.4 ->
2.2 -> 3.1 -> 3.2 -> 3.3 -> 3.4 -> 4.1 -> 4.2 -> 5

The CLI abstraction (2.2) and agent_panel split (3.x) are the two big moves. Everything
else is incremental extraction that can be done one commit at a time without breaking
anything.

---

## Principles to Follow During Refactoring

1. **One commit per move**. Never restructure + change behavior in the same commit.
2. **Compile after every step**. If it doesn't compile, the step was too big.
3. **No new features during refactoring**. Feature requests go to a parking lot.
4. **Tests follow structure**. When services/ is extracted, unit tests become possible
   for the first time (no GTK dependency needed to test git parsing or config loading).
5. **`pub(crate)` by default**. Only make things `pub` when needed across crate boundary.
   Tighten visibility as you go.
6. **Grep for the old import**. After every module move, grep for the old path to catch
   stragglers. The Rust compiler will catch most, but doc comments and string references won't.
