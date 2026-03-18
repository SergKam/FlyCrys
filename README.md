# FlyCrys

A native GTK4 file viewer and AI agent interface built in Rust. GUI frontend for Claude Code.

## Layout

```
+----------+-------------------+-----------------+
|          |                   |                 |
| File     |   Text View       |   Agent Panel   |
| Tree     |   (syntax-colored)|   (chat + tools)|
| (left)   |-------------------|                 |
|          |   Terminal         |   [Pause][Stop] |
|          |   (on demand)     |   [  input   ]  |
+----------+-------------------+-----------------+
```

## Features

- **File tree** with folder/file icons, lazy-loaded on expand (ListView + TreeListModel)
- **Single-click** to open files or toggle directories
- **Syntax highlighting** via syntect — JS/JSX, TS/TSX, JSON, CSS, HTML, YAML, Rust, Python, Go, and more
- **Text viewer** with dark theme, monospace font, read-only, 10 MB file size guard
- **Right-click context menu** on tree items:
  - Copy Path (to clipboard)
  - Add to Chat (appends file path to agent input)
  - Open Terminal Here (VTE terminal in bottom panel)
- **Drag and drop** files/folders from tree onto agent panel to add paths to input
- **Embedded terminal** using VTE4, spawns `$SHELL` in the selected directory
- **Agent panel** — chat interface to Claude Code CLI:
  - Streams responses in real-time via stream-json protocol
  - Markdown rendering in assistant responses (bold, italic, code, headings, lists)
  - Tool calls rendered as expandable panels with output
  - Pause/Resume (SIGSTOP/SIGCONT), Stop (SIGTERM), Clear
  - Multi-turn conversation via stdin pipe
- Directories sorted first, hidden files excluded

## Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System libraries (Ubuntu/Debian)
sudo apt install libgtk-4-dev libvte-2.91-gtk4-dev

# Claude Code CLI
npm install -g @anthropic-ai/claude-code
```

## Build & Run

```bash
cargo build
cargo run
```

## Project Structure

```
src/
  main.rs           - App bootstrap, window layout, signal wiring, CSS, drag-and-drop
  file_entry.rs     - GObject subclass for file tree items
  tree.rs           - File tree (ListView + TreeListModel), lazy-load, sorting
  textview.rs       - Read-only text viewer with syntax highlighting
  highlight.rs      - Syntax highlighting via syntect (TextBuffer tags)
  terminal.rs       - VTE4 terminal panel with close button
  agent_panel.rs    - Agent chat panel: UI, process lifecycle, event handling
  agent_process.rs  - Claude CLI subprocess: spawn, stdin/stdout, signals
  agent_widgets.rs  - Chat widget builders: user bubbles, tool panels, system info
  markdown.rs       - Markdown to Pango markup converter for assistant responses
```

## Dependencies

| Crate         | Version | Purpose                          |
|---------------|---------|----------------------------------|
| gtk4          | 0.10    | GTK4 Rust bindings (UI toolkit)  |
| vte4          | 0.9     | VTE terminal emulator widget     |
| serde         | 1       | JSON deserialization             |
| serde_json    | 1       | Claude CLI stream-json parsing   |
| libc          | 0.2     | Process signal sending           |
| pulldown-cmark| 0.12    | Markdown parsing for chat        |
| syntect       | 5       | Syntax highlighting for viewer   |

## Roadmap

- [x] File tree with lazy loading
- [x] Text viewer with syntax highlighting
- [x] Embedded terminal
- [x] Agent panel with Claude Code CLI integration
- [x] Markdown rendering in agent responses
- [x] Drag-and-drop files to agent chat
- [ ] File search / fuzzy finder
- [ ] Tab support for multiple open files
- [ ] Agent model selector (sonnet/opus/haiku)
- [ ] Session persistence (--continue/--resume)
