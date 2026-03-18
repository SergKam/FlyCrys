# FlyCrys

> *Fast like a fly, solid like a crystal!*

**Lightning-fast, Linux-native agentic UI on top of Claude Code CLI.**

FlyCrys is not an IDE. It doesn't edit files — agents do. You talk to agents, they write the code. FlyCrys gives you a minimal, focused workspace to orchestrate that workflow without getting in the way.

```
┌─────────────────┬──────────┬───────────────────┐
│                 │          │                   │
│   Agent         │  File    │   Text Viewer     │
│   (chat + tools)│  Tree    │   (syntax-colored)│
│                 │          │───────────────────│
│                 │          │   Terminal        │
│                 │          │   (on demand)     │
└─────────────────┴──────────┴───────────────────┘
  ← agent works →   ← browse & inspect →
```

## Why FlyCrys

- **Purely agentic.** No editor, no keybindings to learn, no plugin ecosystem. You give instructions, agents execute. That's it.
- **Native & fast.** Built in Rust with GTK4. Starts in under a second. No Electron, no browser, no runtime overhead.
- **Minimal by design.** One binary. Small footprint. Does one thing well — lets you work with AI agents on your codebase.
- **Multi-workspace tabs.** One agent per workspace, multiple tabs. Each workspace is a focused session on a project directory.
- **Linux-first.** Built for Linux desktops. Respects your system theme. Feels at home on GNOME, KDE, Sway, i3.

## Quick Start

### 1. Install system dependencies

**Ubuntu / Debian:**
```bash
sudo apt install libgtk-4-dev libvte-2.91-gtk4-dev
```

**Fedora:**
```bash
sudo dnf install gtk4-devel vte291-gtk4-devel
```

**Arch:**
```bash
sudo pacman -S gtk4 vte4
```

### 2. Install Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 3. Build & run

```bash
git clone https://github.com/AIsavvyAI/flycrys.git
cd flycrys
cargo build --release
./target/release/flycrys
```

## What It Does

**Left panel** — agent panel. A streaming chat session with Claude Code CLI. You see tool calls as they happen, watch files get created and modified in real time, and send follow-up instructions. A thinking spinner shows while the agent is working.

**Center** — file tree. Single-click to browse. Right-click to copy paths, open a terminal, or send files to an agent.

**Right panel** — read-only text viewer with syntax highlighting (20+ languages via syntect). Terminal below it when you need one (VTE4, spawns your `$SHELL`).

**Drag & drop** files from the tree onto the agent panel to reference them. That's the whole interaction model — point agents at code, tell them what to do, watch it happen.

### Agent Features

- Real-time streaming responses with markdown rendering
- Tool calls shown inline with spinners while running
- Pause / Resume / Stop agent processes (SIGSTOP / SIGCONT / SIGTERM)
- Multi-turn conversations with session resume
- Agent profiles: Default, Security, Research, and custom
- Tab spinner indicates which workspace's agent is actively working

### Workspace Features

- Multiple workspace tabs (one per project directory)
- Session persistence — window state, pane positions, agent configs autosaved
- Light / dark theme toggle (follows system preference)
- 10 MB file size guard on text viewer

## Project Structure

```
src/
tests/
```

## Tech Stack

| Crate | Purpose |
|-------|---------|
| gtk4 0.10 | UI toolkit (GTK4 Rust bindings) |
| vte4 0.9 | Embedded terminal emulator |
| syntect 5 | Syntax highlighting |
| pulldown-cmark 0.12 | Markdown parsing |
| serde + serde_json | Claude CLI stream-json protocol |
| uuid | Workspace/session IDs |
| libc | Process signal handling |

## Roadmap

- [x] File tree with lazy loading
- [x] Syntax-highlighted text viewer
- [x] Embedded VTE4 terminal
- [x] Agent panel with Claude Code CLI streaming
- [x] Markdown rendering in responses
- [x] Drag-and-drop files to agent chat
- [x] Multi-workspace tabs
- [x] Session persistence
- [x] Agent profiles
- [x] Session resume (continue previous conversations)
- [ ] Agent model selector (Haiku / Sonnet / Opus)
- [ ] MCP server configuration
- [ ] Budget controls & cost display
- [ ] Git worktree isolation per agent

## Philosophy

FlyCrys is deliberately simple. The goal is a fast, stable, minimal surface for agentic development — not another feature-heavy IDE. FlyCrys handles one thing: giving you a clean window into your codebase and your agent.

Contributions that keep things simple and fast are welcome. Features that add complexity without clear agentic value are not.

## License

MIT License — see [LICENSE](LICENSE) for details.
