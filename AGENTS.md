# AGENTS.md - FlyCrys

Instructions for AI agents working on this codebase.

## Rules

- **NEVER use deprecated GTK4 APIs.** No TreeView, TreeStore, CellRenderer*, ListStore (deprecated one). Use modern replacements: ListView, ColumnView, TreeListModel, SignalListItemFactory, TreeExpander, gio::ListStore. No `#[allow(deprecated)]` annotations.

- **ALWAYS check latest GTK4 API docs before using any GTK API.** Do NOT rely on memory or training data — GTK4 APIs change between versions and many are deprecated. Use Context7 (`resolve-library-id` + `query-docs` for `gtk4-rs`) or fetch the official docs at https://docs.rs/gtk4 and https://docs.gtk.org/gtk4/ to verify that the API you're about to use is current and not deprecated. When in doubt, look it up.

- **Handle errors properly — show the reason to the user.** Never silently swallow errors. Follow these rules:

  **In UI code:** Show meaningful error messages to the user. Use `gtk::AlertDialog` (NOT the deprecated `MessageDialog`):
  ```rust
  // Correct pattern — AlertDialog with detail
  let dialog = gtk::AlertDialog::builder()
      .message("Operation failed")
      .detail(&format!("Could not open file: {err}"))
      .build();
  dialog.show(Some(&window));
  ```
  For errors inside the agent panel, use `agent_widgets::create_system_message()` to display the error inline with the reason.

  **In service code:** Return `Result<T, String>` (or a proper error type) with a descriptive message that includes the underlying cause. Never discard the `why`:
  ```rust
  // Bad — reason is lost
  fs::read_to_string(path).ok();
  command.spawn().ok();

  // Good — reason is propagated
  fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
  ```

  **Specifically:**
  - `eprintln!()` is acceptable for debug/logging but is NOT a substitute for user-visible feedback
  - `.unwrap()` is only acceptable on infallible operations (e.g., GTK widget downcasts inside factory callbacks where the type is guaranteed). Never on I/O, parsing, or network operations.
  - `.ok()` that discards a result is a code smell — justify it with a comment or handle the error
  - For async dialog callbacks (`FileDialog`, `AlertDialog`), check for `GtkDialogError::Dismissed` — that's the user choosing to cancel, not an error worth reporting

## Tech Stack

- **Language**: Rust (edition 2024)
- **UI toolkit**: GTK4 via `gtk4` crate (v0.10) with `v4_12` feature
- **Terminal**: VTE4 via `vte4` crate (v0.9)
- **Highlighting**: `syntect` for code, `pulldown-cmark` for markdown
- **Serialization**: `serde` + `serde_json` for config persistence and CLI protocol
- **Platform**: `dirs` crate for XDG-compliant config paths
- **System deps**: `libgtk-4-dev`, `libvte-2.91-gtk4-dev`
- **Target OS**: Linux only (Debian/Ubuntu, Fedora, Arch)

## Architecture

### Layered Architecture (MUST follow)

The codebase is organized into four layers. **Each layer may only depend on layers below it. Never the reverse.**

```
Layer 3 — UI        (src/ui/, src/workspace.rs, src/main.rs, src/agent_widgets.rs, ...)
    |                 GTK widgets, presentation, user interaction
    v
Layer 2 — Services   (src/services/)
    |                 Business logic, I/O, CLI backends, platform ops (NO GTK imports)
    v
Layer 1 — Models     (src/models/)
    |                 Pure data structures (no I/O, no GTK, no business logic)
    v
Layer 0 — Config     (src/config/)
                      Constants, domain enums, theme definitions (no imports from above)
```

**Dependency rules:**
- `config/` imports nothing from the project (only std and serde)
- `models/` imports only from `config/`
- `services/` imports from `models/` and `config/` (NEVER from `ui/` or GTK)
- `ui/` and top-level UI modules import from any layer below

### Module Map

```
src/
  config/                          LAYER 0: Constants, types, theme
    constants.rs                   All magic numbers, known editors/browsers, file type maps,
                                   quick commands — THE source of truth for tunable values
    types.rs                       Domain enums: Theme, ViewMode, DiffMode, NotificationLevel,
                                   AgentOutcome, TreeItemKind
    theme.rs                       CSS generation from Theme enum

  models/                          LAYER 1: Pure data structures
    app_config.rs                  AppConfig (window state, theme, notifications)
    workspace_config.rs            WorkspaceConfig (pane sizes, view mode, agent sessions)
    agent_config.rs                AgentConfig (name, system prompt, tools, model)
    chat.rs                        ChatMessage enum (user, assistant, tool, system)

  services/                        LAYER 2: Business logic and I/O
    storage.rs                     Config/session/agent persistence (load, save, delete, list)
    platform.rs                    OS interaction: xdg-open, editor/browser detection, default shell
    git.rs                         Git CLI operations: status, diff, GitFileStatus enum
    cli/
      mod.rs                       AgentBackend trait, AgentDomainEvent enum, AgentSpawnConfig,
                                   ImageAttachment — the CLI-agnostic abstraction
      claude.rs                    Claude CLI implementation: process spawning, wire types (private),
                                   stream-json parsing, event translation to domain events

  ui/                              LAYER 3: GTK UI (sub-modules)
    agent_panel/
      mod.rs                       create_agent_panel() — panel construction and wiring
      state.rs                     PanelState decomposed: AgentProcessState, TokenState,
                                   ChatState, PanelConfig
      event_handler.rs             handle_domain_event() — AgentDomainEvent -> UI updates

  session.rs                       Thin re-export layer (models + storage) for convenience
  main.rs                          App entry point, window setup, tab management, settings popover
  workspace.rs                     Workspace container: paned layout, file tree, editor, terminal, agent
  agent_widgets.rs                 Chat message widget builders (user, assistant, tool, system)
  agent_config_dialog.rs           Agent profile CRUD dialog
  git_panel.rs                     Git status/diff UI panel (calls services/git.rs)
  textview.rs                      File viewer with source/preview modes
  highlight.rs                     Syntax highlighting via syntect (data-driven extension map)
  markdown.rs                      Markdown to Pango markup converter
  terminal.rs                      VTE4 terminal wrapper
  tree.rs                          File tree (ListView + TreeListModel)
  file_entry.rs                    GObject subclass for tree model items
  watcher.rs                       File system change watcher
```

### CLI Abstraction

The UI layer communicates with agent CLIs through **domain events only**:

```
UI (agent_panel)  <---  AgentDomainEvent  <---  AgentBackend trait  <---  ClaudeBackend
                                                                          (services/cli/claude.rs)
```

- `AgentDomainEvent` is CLI-agnostic: `TextDelta`, `ToolStarted`, `TokenUsage`, `Finished`, etc.
- Claude wire types (`ClaudeEvent`, `StreamEventData`, `ContentBlock`, `Delta`) are **private** to `claude.rs`
- UI code MUST NOT match on Claude-specific strings (`"content_block_delta"`, `"message_start"`, etc.)
- The `AgentBackend` trait defines: `spawn`, `send_message`, `pause`, `resume`, `stop`, `is_alive`

### Key Patterns

- **GObject model**: `FileEntry` uses `mod imp` pattern with `ObjectSubclass`, `Properties` derive, `glib::wrapper!`
- **TreeListModel lazy loading**: `create_func` returns `Some(child_ListStore)` for directories, `None` for files
- **Agent subprocess I/O**: Reader thread + `std::sync::mpsc::channel` + `glib::timeout_add_local(16ms)` polling for GTK main loop integration. Backend translates wire events to domain events in the reader thread.
- **Streaming markdown**: Track current `gtk::Label`, accumulate text deltas, re-render full markdown->Pango on each delta
- **Signal closures**: `glib::clone!` with `#[weak]`/`#[strong]`. Shared mutable state via `Rc<RefCell<>>`
- **Drag-and-drop**: `DragSource` on ListView provides path as `glib::Value`. `DropTarget` on agent input accepts string drops.

## Coding Rules (MUST follow)

### No Magic Numbers

Every numeric literal, timeout, dimension, threshold, or buffer size MUST be a named constant in `config/constants.rs`. The only exceptions are:
- Trivial values: `0`, `1`, `-1` as initial/sentinel values
- GTK widget spacing (4, 6, 8, 10, 12) which are standard GTK defaults
- Loop bounds derived from data (`0..len`)

**Bad:**
```rust
terminal.set_scrollback_lines(10000);
glib::timeout_add_local(Duration::from_secs(5), move || { ... });
container.set_width_request(420);
```

**Good:**
```rust
terminal.set_scrollback_lines(TERMINAL_SCROLLBACK_LINES);
glib::timeout_add_local(Duration::from_secs(AUTOSAVE_INTERVAL_SECS), move || { ... });
container.set_width_request(AGENT_PANEL_MIN_WIDTH);
```

### No Hardcoded Lists

Lists of known values (editors, browsers, file extensions, MIME types, quick commands) MUST live in `config/constants.rs` as static arrays. Code iterates over them — never inline the items.

### Enums Over Booleans

When a field represents a mode, preference, or status — use an enum, not a bool. Booleans are only for genuinely binary states (like "is the widget visible right now" in transient UI logic).

**Bad:** `is_dark: bool`, `preview_mode: bool`, `show_diff: bool`
**Good:** `theme: Theme`, `view_mode: ViewMode`, `diff_mode: DiffMode`

Domain enums live in `config/types.rs`. They must derive `Copy` (for use in `Cell<T>`), `Serialize`, `Deserialize`, and have helper methods (e.g., `Theme::is_dark()`, `Theme::toggle()`).

### Data-Driven Extensibility

When you add support for a new file type, editor, browser, quick command, or similar — add it to the corresponding constant array. Never add a new arm to a match statement that hardcodes specific values.

### Layer Discipline

- **New CLI-specific code** goes in `services/cli/claude.rs` (or a new backend module)
- **New OS/desktop interaction** goes in `services/platform.rs`
- **New git operations** go in `services/git.rs`
- **New persistence logic** goes in `services/storage.rs`
- **New data types** go in `models/`
- **New UI widgets** go in `ui/` or top-level UI modules
- Never import GTK types in `config/`, `models/`, or `services/`

### Struct Decomposition

Large state structs should be decomposed into focused sub-structs grouped by responsibility. See `ui/agent_panel/state.rs` for the pattern: `AgentProcessState`, `TokenState`, `ChatState`, `PanelConfig` composed into `PanelState`.

### Function Size

Keep functions under ~100 lines. If a function grows larger, extract helper functions. The event handler pattern in `ui/agent_panel/event_handler.rs` is the example: one top-level match, each arm delegates to focused logic.

## Build Commands

```bash
cargo build                  # debug build
cargo run                    # run debug
cargo build --release        # release build
cargo test                   # run all tests
cargo fmt                    # format code
cargo fmt -- --check         # check formatting without modifying
cargo clippy                 # lint
```

## Git Hooks

Pre-commit hook runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
The hook lives in `hooks/pre-commit` (tracked in the repo).

```bash
# Enable hooks (once per clone):
git config core.hooksPath hooks
```

## Conventions

- Use `gtk4 as gtk` alias throughout
- Import `gtk::prelude::*` for extension traits
- Use `glib::clone!` macro with `#[weak]`/`#[strong]` for signal closures
- GObject property names use kebab-case (e.g., `"icon-name"`, `"is-dir"`)
- Minimize `unsafe` — only for `libc::kill` in process management
- Use `pub(crate)` for internal items; only `pub` when needed across crate boundary
- Serde structs: use `#[serde(default)]` and `Option<>` liberally, `#[serde(other)]` catch-all for unknown variants

## When Making Changes

- Run `cargo check` after every structural change — GTK binding errors can be cryptic
- Run `cargo fmt` before committing
- If adding new GTK4 features, check if they require a version feature flag (e.g., `v4_14`)
- New tuneable values go in `config/constants.rs` with descriptive names
- New domain types/enums go in `config/types.rs`
- New file type support: add to `SYNTAX_ALIASES` and/or `HIGHLIGHTABLE_EXTENSIONS` in constants
- Agent event handling: ONLY match on `AgentDomainEvent` variants, never Claude wire types
- When creating new list/tree widgets, use ListView/ColumnView + gio::ListStore + SignalListItemFactory
- Agent event deserialization must be lenient — unknown fields/types should be skipped gracefully
