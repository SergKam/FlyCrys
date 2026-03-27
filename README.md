# FlyCrys

> *Fast like a fly, solid like a crystal.*

A Linux-native GUI for working with Claude Code agents. Built in Rust + GTK4. No Electron, no browser runtime, starts in under a second.

FlyCrys is not an IDE. It doesn't edit files. Agents do. You talk to agents, they write the code. FlyCrys gives you a minimal workspace to manage that workflow without getting in the way.

![FlyCrys workspace](docs/screenshot-workspace.png)

## Why this exists

Most AI coding tools are either browser-based (slow, resource-hungry) or terminal-only (limited UI for reviewing changes). FlyCrys sits in between: a native desktop app that wraps the Claude Code CLI with a proper file tree, text viewer, terminal, and chat panel. One binary, small footprint, runs on any Linux desktop.

## Install

### Debian package (recommended)

```bash
curl -fsSLo /tmp/flycrys.deb https://github.com/SergKam/FlyCrys/releases/latest/download/flycrys_amd64.deb
sudo apt install /tmp/flycrys.deb
```

To upgrade, run the same two commands. The URL always points to the latest release.

All releases: [github.com/SergKam/FlyCrys/releases](https://github.com/SergKam/FlyCrys/releases)

### Build from source

Install system dependencies:

```bash
# Ubuntu / Debian
sudo apt install libgtk-4-dev libvte-2.91-gtk4-dev libwebkitgtk-6.0-dev libjavascriptcoregtk-6.0-dev libsoup-3.0-dev

# Fedora
sudo dnf install gtk4-devel vte291-gtk4-devel webkitgtk6.0-devel

# Arch
sudo pacman -S gtk4 vte4 webkitgtk-6.0
```

Build and run:

```bash
git clone https://github.com/SergKam/FlyCrys.git
cd FlyCrys
cargo build --release
./target/release/flycrys
```

### Prerequisites

FlyCrys requires the [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code):

```bash
npm install -g @anthropic-ai/claude-code
```

## What it does

### Agent panel

- Streaming chat with markdown rendering (tables, code blocks, lists, blockquotes)
- Tool calls shown inline with spinners while running
- Pause, resume, stop agent processes
- Session resume across app restarts
- Agent profiles: Default, Security, Research, and custom (name, system prompt, allowed tools, model)
- Image attachments via clipboard (Ctrl+V) or file picker
- File and folder path insertion into prompts
- Bookmarks for reusable prompts with a CRUD dialog
- Clickable file paths in responses open directly in the viewer
- Token usage and session cost in the status bar
- Tab spinner shows which workspace's agent is active

### Slash commands and skills manager

Type `/` in the input field to see all available commands. The list filters as you type, and you can navigate with arrow keys, Tab, and Enter.

FlyCrys scans for commands from:
- Claude Code built-in commands (`/clear`, `/compact`, `/cost`, `/model`, etc.)
- Global user commands (`~/.claude/commands/`)
- Global user skills (`~/.claude/skills/`)
- Project commands and skills (`.claude/commands/`, `.claude/skills/`)
- Installed plugins (`~/.claude/plugins/`)

The skills manager dialog lets you create, edit, and delete user and project commands. Installed plugins are shown read-only. Use `/rescan-skills` to refresh the list without restarting.

![Skills manager dialog](docs/screenshot-skills.png)

### File tree

- Lazy-loading tree, subdirectories fetched only on expand
- System icons based on MIME types
- Toolbar with Collapse All and Search
- File search across the entire project
- Live refresh via filesystem watcher (preserves expand state)
- Right-click menu: Copy Path, Add to Chat, Open Terminal Here, Open in Default App, Edit in Text Editor, Open in Browser
- Drag and drop files onto agent input
- Git status panel with color-coded modified/added/deleted/untracked files
- `.git`, `target`, `node_modules` hidden automatically

### Text viewer

- Three-state mode switch: Source, Preview, Diff (segmented buttons)
- Syntax highlighting for 25+ languages via syntect
- Line numbers with auto-sizing gutter
- Markdown preview rendered in WebKitGTK
- Image preview with content-fit scaling
- Git diff view with syntax highlighting
- Toolbar: Open, Edit, Browser, Terminal, Copy Path, Add to Chat

### Terminal

- Embedded VTE4 terminal with full PTY support
- Uses `$SHELL` or falls back to `/bin/bash`
- "Terminal here" opens shell in the file's directory
- Scrollback saved and restored across sessions
- Colors adapt to light/dark mode

### Workspace

- Multi-tab workspaces, one per project directory
- Session persistence: window size, pane positions, open files, agent sessions, theme
- Lazy tab loading: only the active tab is built at startup
- Status bar: agent name, tokens, cost, git branch (updates via inotify), working directory
- Light/dark theme toggle with native GTK integration
- Desktop notifications toggle

## Project structure

```
src/
  config/                        Constants, enums, theme CSS
    constants.rs                 Magic numbers, file type maps, known editors/browsers
    types.rs                     Theme, PanelMode, NotificationLevel, AgentOutcome, TreeItemKind
    theme.rs                     Light/dark CSS

  models/                        Pure data structures (no I/O, no GTK)
    app_config.rs                AppConfig (window state, theme, notifications)
    workspace_config.rs          WorkspaceConfig (pane sizes, panel mode, agent sessions)
    agent_config.rs              AgentConfig (name, system prompt, tools, model)
    chat.rs                      ChatMessage enum (user, assistant, tool, system)

  services/                      Business logic and I/O (no GTK)
    storage.rs                   Config/session/agent/bookmark persistence
    platform.rs                  xdg-open, editor/browser detection, default shell
    git.rs                       Git CLI: status, diff, branch
    skills.rs                    Slash command/skill scanner and cache
    cli/
      mod.rs                     AgentBackend trait, AgentDomainEvent, AgentSpawnConfig
      claude.rs                  Claude CLI process spawn, stream-json parsing

  ui/                            GTK UI modules
    agent_panel/
      mod.rs                     Agent panel construction and wiring
      state.rs                   AgentProcessState, TokenState, ChatState, PanelConfig
      event_handler.rs           AgentDomainEvent -> UI updates
      chat_factory.rs            Chat history rendering

  main.rs                        App entry, window, tabs, settings menu
  workspace.rs                   Workspace layout, pane wiring, file-open pipeline
  textview.rs                    File viewer (source/preview/diff modes)
  tree.rs                        File tree panel (TreeListModel, search, collapse)
  chat_webview.rs                WebKitGTK chat rendering
  chat_entry.rs                  Agent input widget (multi-line, paste, send)
  slash_popover.rs               Slash command autocomplete popover
  skills_dialog.rs               Skills/commands CRUD dialog
  agent_widgets.rs               Chat message widget builders
  agent_config_dialog.rs         Agent profile CRUD dialog
  bookmark_dialog.rs             Bookmark CRUD dialog
  git_panel.rs                   Git status/diff panel
  highlight.rs                   Syntax highlighting via syntect
  markdown.rs                    Markdown -> HTML converter
  terminal.rs                    VTE4 terminal with scrollback persistence
  file_entry.rs                  GObject subclass for tree model items
  watcher.rs                     Filesystem change watcher
  session.rs                     Re-export layer (models + storage)
```

## Tech stack

| Crate | What for |
|-------|----------|
| `gtk4` 0.10 (v4_12) | UI toolkit |
| `webkit6` 0.5 | Chat rendering and markdown preview |
| `vte4` 0.9 | Embedded terminal |
| `syntect` 5 | Syntax highlighting |
| `pulldown-cmark` 0.12 | Markdown to HTML |
| `notify` 6 | Filesystem and git branch watcher (inotify) |
| `dirs` 6 | XDG config/data paths |
| `serde` + `serde_json` | Config persistence and Claude CLI protocol |
| `base64` 0.22 | Image encoding for attachments |
| `uuid` 1 | Workspace and session IDs |
| `libc` 0.2 | Process signal handling |

System dependencies: `libgtk-4-dev`, `libvte-2.91-gtk4-dev`, `libwebkitgtk-6.0-dev`, `libjavascriptcoregtk-6.0-dev`, `libsoup-3.0-dev`

Target: Linux (Debian/Ubuntu, Fedora, Arch)

## License

MIT License. See [LICENSE](LICENSE).
