# Design: Run Panel (Multi-Terminal + Background Tasks)

## Overview

Replace the single-terminal panel with a tabbed "Run" panel supporting
multiple shell terminals and automatic background-task tracking from
Claude Code agents.

## Tab Types

| Type | Icon | Content | Interactive |
|------|------|---------|------------|
| **Shell** | `utilities-terminal-symbolic` | Full VTE terminal | Read/Write |
| **Background Task** | `system-run-symbolic` | Tailed task output | Read-only |

## Layout

```
┌──────────────────────────────────────────────────┐
│ [bash(1)] [bash(2)] [build ✓] [test ⏳]    [+]  │  ← tab bar
├──────────────────────────────────────────────────┤
│                                                  │
│  $ cargo build --release                         │  ← active tab content
│  Compiling flycrys v0.2.1                        │
│                                                  │
└──────────────────────────────────────────────────┘
```

- **Tab bar**: GTK Notebook (native scroll-on-overflow, drag-reorder)
- **[+] button**: Always creates a new shell tab in the project root
- **No hard limit** on tab count

## Shell Tabs

- Each tab owns one `vte4::Terminal` + one bash process
- New tabs start in workspace `working_directory`
- Default name: `bash(N)` (auto-incremented)
- Rename via right-click context menu

### Right-Click Context Menu (Tab Header Only)

Right-click in the terminal area passes through to the application
running inside the terminal (vim, mc, bash, etc.) — never intercepted.

All custom actions live on the **tab header** right-click menu:

- **Rename** — inline entry popover
- **Copy All Text** — entire scrollback to clipboard
- **Add Selected to Chat** — send selection to agent input field
- **Close Tab** — kills shell, removes tab (blocked if last shell tab)

## Background Task Tabs

### Discovery (dual approach)

1. **Stream-JSON parsing** (primary, instant):
   Parse agent stdout for `tool_use` events with `name: "Bash"` +
   `run_in_background: true`.  Extract the task ID and output file path
   from the response.

2. **Inotify watcher** (fallback, catches restarts):
   Watch `~/.claude/tasks/` for new files.  Match task IDs to the
   current workspace session.

### Lifecycle

- Auto-created when a background task starts
- Tab shows task name or truncated command as label
- Status indicator on tab: ⏳ running, ✓ done, ✗ failed
- **Never auto-closed** — user closes manually
- Content: tail the output file, updated via inotify on writes

### Rendering

- Background task output is **plain text** (no PTY escape codes)
- Use a `gtk::TextView` (not VTE) with monospace font + read-only
- Optionally apply ANSI color parsing if output contains escape codes

## Persistence

### Storage Layout

```
~/.config/flycrys/sessions/
├── {ws_id}.json                 # WorkspaceConfig (includes run_tabs array)
├── {ws_id}_terminal_0.txt       # scrollback for tab 0
├── {ws_id}_terminal_1.txt       # scrollback for tab 1
└── ...
```

### WorkspaceConfig Changes

```rust
// New fields in WorkspaceConfig:
pub run_tabs: Vec<RunTabConfig>,
pub active_run_tab: usize,       // index of the focused tab

pub struct RunTabConfig {
    pub id: String,              // uuid
    pub name: String,            // display name ("bash(1)", "cargo build")
    pub tab_type: RunTabType,    // Shell | BackgroundTask
    pub task_file: Option<String>, // path to task output (background only)
}

pub enum RunTabType {
    Shell,
    BackgroundTask,
}
```

### Lazy Loading

On workspace restore:
1. Read `run_tabs` from config → create tab buttons (zero VTE widgets)
2. On first tab focus:
   - **Shell**: create VTE → restore scrollback → spawn shell
   - **Task**: create TextView → load output file → start inotify tail
3. Unvisited tabs consume ~0 memory

### Dirty Tracking

- Each shell tab has its own `dirty: Rc<Cell<bool>>`
- On save: only write scrollback for dirty tabs
- Background task tabs: no save needed (output lives in Claude's task files)

## First-Run Behavior

- New workspace → one shell tab named `bash(1)`, auto-focused
- Never allow zero tabs — closing last shell tab is blocked (button disabled or ignored)

## Migration

No migration needed.  Old single-terminal workspaces simply start
fresh with a new `bash(1)` tab.  The legacy `{ws_id}_terminal.txt`
file is ignored (harmless leftover, cleaned up on next save).

## Implementation Phases

### Phase 1: Multi-Terminal Tabs
- Replace single VTE with GTK Notebook
- Tab creation/close/rename
- Persistence + lazy loading
- Right-click context menus
- "Copy All Text" and "Add Selected to Chat"

### Phase 2: Background Task Tracking
- Stream-JSON task detection
- Inotify watcher on `~/.claude/tasks/`
- Auto-create task tabs with status indicators
- Plain text viewer for task output

### Phase 3: Polish
- Tab drag-reorder
- Keyboard shortcuts (Ctrl+Shift+T new tab, Ctrl+Shift+W close)
- Tab status colors (red for failed tasks)
- "Add Selected to Chat" integration with agent input widget
