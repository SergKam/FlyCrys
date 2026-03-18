# AGENTS.md - FlyCristal

Instructions for AI agents working on this codebase.

## Rules

- **NEVER use deprecated GTK4 APIs.** No TreeView, TreeStore, CellRenderer*, ListStore (deprecated one). Use modern replacements: ListView, ColumnView, TreeListModel, SignalListItemFactory, TreeExpander, gio::ListStore. No `#[allow(deprecated)]` annotations.

## Tech Stack

- **Language**: Rust (edition 2024)
- **UI toolkit**: GTK4 via `gtk4` crate (v0.10) with `v4_12` feature
- **Terminal**: VTE4 via `vte4` crate (v0.9)
- **Highlighting**: `syntect` for code, `pulldown-cmark` for markdown
- **Serialization**: `serde` + `serde_json` for Claude CLI stream-json protocol
- **System deps**: `libgtk-4-dev`, `libvte-2.91-gtk4-dev`

## Architecture

Single-window GTK4 application with three-column layout. Uses GObject subclass (`FileEntry`) for the file tree model. The agent panel interfaces with Claude Code CLI as a subprocess via stream-json protocol.

### Module Responsibilities

- **`main.rs`** — App entry point. Creates window with outer Paned (left: file tree+editor, right: agent panel), wires signals, registers actions (copy-path, add-to-chat, open-terminal-here), loads CSS, sets up drag-and-drop.
- **`file_entry.rs`** — GObject subclass with properties: `name`, `path`, `icon_name`, `is_dir`. Used in `gio::ListStore` for the tree model.
- **`tree.rs`** — File tree using `ListView` + `TreeListModel` + `SignalListItemFactory` + `TreeExpander`. Lazy-loads via `create_func`.
- **`textview.rs`** — Read-only monospace `TextView` with path label header. Delegates to `highlight.rs` for code files. 10 MB size guard.
- **`highlight.rs`** — Syntax highlighting via `syntect`. Applies `TextTag` per styled region (foreground, bold, italic) to `TextBuffer`. Uses `base16-eighties.dark` theme. Handles extension mapping (mjs/cjs/jsx→JS, tsx→TS, yml→YAML).
- **`terminal.rs`** — VTE4 terminal in a `Box` with close button. `spawn_shell()` starts `$SHELL`.
- **`agent_panel.rs`** — Agent chat panel: header, scrollable message list, Pause/Stop/Clear buttons, text input + Send. Returns `(gtk::Box, gtk::TextView)` — the panel and input widget. Manages `AgentProcess` lifecycle and wires stream events to widget updates.
- **`agent_process.rs`** — Subprocess management for Claude CLI. Spawns `claude -p --output-format stream-json --input-format stream-json`. Reader thread sends parsed `AgentEvent` via `mpsc::channel`. Supports pause (SIGSTOP), resume (SIGCONT), stop (SIGTERM).
- **`agent_widgets.rs`** — Widget builders for chat: user bubbles, streaming assistant text labels (with markdown via Pango markup), tool call Expanders with spinner/result, system info labels.
- **`markdown.rs`** — Converts markdown to Pango markup for GTK Labels. Handles bold, italic, code, code blocks, headings, lists, blockquotes, strikethrough.

### Layout Hierarchy

```
ApplicationWindow
  Paned (Horizontal, "outer", position=980)
    Paned (Horizontal, "inner", position=300)
      ScrolledWindow                   # file tree
        ListView + TreeListModel
          DragSource (provides file path as text)
      Paned (Vertical)                 # editor + terminal
        Box (textview + path label)
        Box (terminal, initially hidden)
    Box                                # agent panel (right)
      Label "Agent"
      Separator
      ScrolledWindow                   # chat history
        Box (vertical, message widgets)
      Box (Pause | Stop | Clear)
      Box (TextView input + Send btn)
        DropTarget (accepts text, appends path)
      DropTarget on panel (same behavior)
```

### Claude CLI Protocol

The agent spawns: `claude -p --output-format stream-json --verbose --include-partial-messages --input-format stream-json --dangerously-skip-permissions`

Key stream-json event types (NDJSON, one JSON per line):
- `system` — session init
- `stream_event` — incremental content: `content_block_start`, `content_block_delta` (text_delta / input_json_delta), `content_block_stop`, `message_start/delta/stop`
- `assistant` — complete assistant message
- `user` — tool results with `tool_use_result.stdout/stderr` (note: `tool_use_id` is in `message.content[0].tool_use_id`, NOT in `tool_use_result`)
- `result` — final summary with `total_cost_usd`

Multi-turn input via stdin: `{"type":"user","message":{"role":"user","content":"..."}}`

### Key Patterns

- **GObject model**: `FileEntry` uses `mod imp` pattern with `ObjectSubclass`, `Properties` derive, `glib::wrapper!`.
- **TreeListModel lazy loading**: `create_func` returns `Some(child_ListStore)` for directories, `None` for files.
- **ListView item access**: With `passthrough: false`, `list_item.item()` returns `TreeListRow`, then `.item()` returns `FileEntry`.
- **Agent subprocess I/O**: Reader thread + `std::sync::mpsc::channel` + `glib::timeout_add_local(16ms)` polling for GTK main loop integration.
- **Streaming markdown**: Track current `gtk::Label`, accumulate text deltas, re-render full markdown→Pango on each delta via `agent_widgets::update_assistant_text`.
- **Tool call widgets**: `gtk::Expander` with spinner during execution, replaced with tool output when result arrives. Tool panels tracked by `tool_use_id` in a `HashMap`.
- **Syntax highlighting**: `syntect` parses file content, creates `TextTag` per unique (color, bold, italic) combo, applies to `TextBuffer` regions. Tags cached in the tag table by name.
- **Drag-and-drop**: `DragSource` on ListView provides file path as `glib::Value` string. `DropTarget` on agent input and panel accepts string drops.
- **Signal closures**: `glib::clone!` with `#[weak]`/`#[strong]`. Shared mutable state via `Rc<RefCell<>>`.

## Build Commands

```bash
cargo build
cargo run
cargo build --release
```

## Conventions

- Use `gtk4 as gtk` alias throughout
- Import `gtk::prelude::*` for extension traits
- Use `glib::clone!` macro with `#[weak]`/`#[strong]` for signal closures
- GObject property names use kebab-case in builder (e.g., `"icon-name"`, `"is-dir"`)
- Minimize `unsafe` — only for `libc::kill` signal sending in agent_process.rs
- Keep modules focused: one widget concept per file
- Serde structs: use `#[serde(default)]` and `Option<>` liberally, `#[serde(other)]` catch-all for unknown variants

## When Making Changes

- Run `cargo build` after every change — GTK binding errors can be cryptic
- If adding new GTK4 features, check if they require a version feature flag (e.g., `v4_14`)
- VTE `spawn_async` requires a `child_setup: impl Fn()` parameter before `timeout`
- The terminal panel starts hidden; "Open Terminal Here" makes it visible
- When creating new list/tree widgets, use ListView/ColumnView + gio::ListStore + SignalListItemFactory
- Agent event deserialization must be lenient — unknown fields/types should be skipped gracefully
- Tool result `tool_use_id` is in `message.content[0].tool_use_id`, not in `tool_use_result`
