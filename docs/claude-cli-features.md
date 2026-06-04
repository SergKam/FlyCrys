# Claude CLI Integration — Feature Research

> Feature survey based on Claude Code CLI **v2.1.78**.
> AskUserQuestion / control-protocol section verified on Claude Code CLI **v2.1.154** (2026-06-04).
> Current spawn command: `claude -p --output-format stream-json --verbose --include-partial-messages --input-format stream-json --permission-prompt-tool stdio`
> (Previously ended in `--dangerously-skip-permissions`; replaced — see the AskUserQuestion section.)

## Current State

FlyCrys spawns a **fresh, stateless** `claude -p` process for each conversation. There is no session persistence, no model selection, and no way to configure MCP servers from the GUI. Every conversation starts from scratch and is lost when the process exits.

---

## 0. AskUserQuestion via the stdio control protocol — IMPLEMENTED

> **Status:** Implemented. **Verified on CLI v2.1.154** (2026-06-04). See the
> regression checklist at the end of this section before bumping the CLI.

### Problem

`AskUserQuestion` (the tool Claude calls to ask the user a multiple-choice
clarifying question, added in CLI v2.0.21) does **not** work as a plain tool in
headless `-p` mode. When we ran with `--dangerously-skip-permissions`, the model
emitted the `tool_use` and the CLI immediately produced its own error
tool_result with the literal content **`"Answer questions?"`** (`is_error: true`),
because there is no interactive TUI to render the selector. The model then gives
up ("I've asked the question and am now waiting…") and ends the turn.

### Mechanism (how the official SDK does it)

`AskUserQuestion` is routed through the **SDK control protocol**, the same channel
the Agent SDK's `canUseTool` callback uses for tool-permission prompts. Enabling
it has two parts:

1. **Spawn flag** `--permission-prompt-tool stdio` — routes permission /
   AskUserQuestion decisions to the client over stdio instead of failing them.
   (Discovered in `claude-agent-sdk-python`'s `subprocess_cli.py`; it is **not
   listed in `claude --help`** on v2.1.154 but is accepted.)
2. **Initialize handshake** — right after spawn, the client sends an `initialize`
   control_request. The CLI acks with a `control_response` (carrying available
   commands/skills, which we ignore).

Then, for each tool needing a decision, the CLI sends a `can_use_tool`
control_request and **blocks until we reply** with a `control_response`.

### Verified wire format (v2.1.154)

```jsonc
// 1. client → CLI, immediately after spawn
{"type":"control_request","request_id":"flycrys_init",
 "request":{"subtype":"initialize","hooks":null}}

// 2. CLI → client, for AskUserQuestion (and every non-allowlisted tool)
{"type":"control_request","request_id":"<uuid>",
 "request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion",
   "tool_use_id":"toolu_…",
   "input":{"questions":[{"question":"…","header":"…","multiSelect":false,
     "options":[{"label":"…","description":"…"}]}]}}}

// 3. client → CLI, answering (request_id MUST match #2)
{"type":"control_response","response":{"subtype":"success","request_id":"<uuid>",
 "response":{"behavior":"allow","updatedInput":{
   "questions":[…echoed…],
   "answers":{"<question text>":"<chosen label>"}}}}}

// 4. CLI → client, the resulting clean tool_result (no more "Answer questions?")
//   content: 'Your questions have been answered: "…"="Blue". You can now continue…'
```

- Multi-select answers: value is an **array** of labels (or a comma-joined string).
- Deny instead of allow: `{"behavior":"deny","message":"…"}`.
- Ordinary tools (Bash, Write, …) also arrive as `can_use_tool` once skip-permissions
  is gone; we auto-`allow` them (see below).

### FlyCrys implementation

- **`services/cli/claude.rs`**
  - Spawn: `--dangerously-skip-permissions` → `--permission-prompt-tool stdio`.
  - stdin is now `Arc<Mutex<Option<ChildStdin>>>` (`SharedStdin`) because the
    reader thread (auto-allow) and the GTK thread (messages, answers) both write.
  - `spawn()` writes the `initialize` request before the first user message.
  - New `ClaudeEvent::ControlRequest`; the reader thread handles it:
    `AskUserQuestion` → emit `AgentDomainEvent::AskUserQuestion`; any other tool →
    auto-`allow` immediately (preserves the old skip-permissions UX — **same
    security posture**, just expressed via the protocol); unknown subtypes → error
    reply so the CLI never blocks.
  - `AgentBackend::answer_question(request_id, updated_input)` writes the
    `control_response`.
- **`chat_webview.rs`** — `appendQuestionCard()` JS renders an inline card
  (radio/checkbox + Submit); submitting navigates to
  `flycrys://answer-question?rid=…&data=<urlencoded {questions,answers}>`, which
  the navigation policy intercepts and forwards via `set_on_answer_question`.
- **`event_handler.rs`** — renders the card on `AskUserQuestion`; suppresses the
  generic tool-call element and the tool_result for `AskUserQuestion` (the card is
  the only UI).

### Limitations / not yet done

- No free-text "Other" option (protocol supports it via `answers[q]=<text>` or a
  top-level `response` string) — options-only for now.
- No timeout handling; the card waits indefinitely (the CLI blocks until answered).
- `AskUserQuestion` is unavailable in subagents (Task tool) per Anthropic docs.
- Tool-permission requests are blanket-allowed; there is no per-tool approval UI
  yet (the plumbing now exists to add one — see §4).

### Regression checklist for newer CLI versions

When bumping the Claude CLI, re-run `/tmp/auq_probe2.py` (or equivalent) and verify:

1. `--permission-prompt-tool stdio` is still accepted (it's undocumented in `--help`).
2. `initialize` still acks with a `control_response`.
3. `AskUserQuestion` still arrives as `control_request` / `subtype:"can_use_tool"`
   (not a renamed subtype like `sdk_control_request`, and not back to an inline
   `tool_use` that auto-fails).
4. The answer shape is still `{"behavior":"allow","updatedInput":{questions,answers}}`
   nested at `response.response`, correlated by top-level `request_id`.
5. Answering yields a **success** tool_result (watch for a regression to the
   `"Answer questions?"` error string).
6. The question input schema is still `questions[].{question,header,options[].{label,description},multiSelect}`.

---

## 1. Session Management (High Priority)

### Problem
Users lose all conversation context when they close the app or start a new chat. There is no way to continue a previous conversation.

### CLI Capabilities
| Flag | Description |
|---|---|
| `--session-id <uuid>` | Attach the process to a specific session |
| `-c, --continue` | Continue the most recent conversation in the CWD |
| `-r, --resume <id>` | Resume a conversation by session ID |
| `--fork-session` | Branch from an existing session into a new one |
| `-n, --name <name>` | Human-readable session name |
| `--no-session-persistence` | Opt-out (print mode only) |

### What We Get from the Stream
The `system` event at session start already carries `session_id`:
```json
{ "type": "system", "subtype": "init", "session_id": "abc-123-..." }
```
We already parse this in `AgentEvent::System` but currently ignore it.

### Proposed Features

#### a) Capture & Store Session ID
- Save the `session_id` from the `system` init event in `PanelState`.
- Persist a session index file (e.g., `~/.config/flycrys/sessions.json`) mapping `session_id` to metadata: name, CWD, timestamp, last message preview.

#### b) Continue Session
- Pass `--session-id <id>` when spawning a new process to resume a previous session.
- The CLI reloads the full conversation history on its side — we just need the ID.

#### c) Session Picker UI
- A dropdown or dialog listing past sessions (name, date, directory).
- "Continue last" shortcut button in the header bar.
- Search/filter by name or directory.

#### d) Fork Session
- "Branch" button that clones the current conversation into a new session (`--fork-session`) for experimentation.

#### e) Session Naming
- Editable session name in the header bar, passed via `--name`.
- Auto-generate a name from the first user message if not set.

### Implementation Notes
- `-p` mode + `--session-id` should work together — the session is stored by the CLI, we just need to supply the ID on reconnect.
- Consider that `--continue` is CWD-scoped and may be simpler for a first pass.
- Session files live in `~/.claude/sessions/` — we can read them directly to build the picker.

---

## 2. Model & Thinking Effort Selector (High Priority)

### Problem
Currently hardcoded to whatever default model the CLI uses. Users cannot switch between fast/cheap (Haiku) and powerful (Opus) models, or tune thinking effort.

### CLI Capabilities
| Flag | Description |
|---|---|
| `--model <alias-or-id>` | e.g., `sonnet`, `opus`, `haiku`, `claude-sonnet-4-6` |
| `--fallback-model <model>` | Auto-fallback on overload (print mode only) |
| `--effort <level>` | `low`, `medium`, `high`, `max` |

### Proposed Features

#### a) Model Selector
- Dropdown in the agent panel header: **Haiku 4.5 / Sonnet 4.6 / Opus 4.6**.
- Passed as `--model <alias>` when spawning the process.
- Persist last selection in app settings.

#### b) Effort Level Selector
- Compact toggle or slider: **Low / Medium / High / Max**.
- Passed as `--effort <level>` on spawn.
- Consider pairing with model: e.g., Haiku+high, Sonnet+medium, Opus+low as presets.

#### c) Fallback Model
- Optional setting: if primary model is overloaded, fall back automatically.
- `--fallback-model sonnet` when using Opus.

### Implementation Notes
- Both flags are spawn-time only — changing model mid-conversation requires stopping and resuming with `--session-id` + new `--model`.
- Add to `AgentProcess::spawn()` as conditional args.

---

## 3. MCP Server Configuration (Medium Priority)

### Problem
No way to manage MCP servers (external tool providers) from the GUI.

### CLI Capabilities
| Flag | Description |
|---|---|
| `--mcp-config <path-or-json>` | Load MCP servers from config file(s) |
| `--strict-mcp-config` | Only use explicitly provided MCP configs |
| `claude mcp add <name> <cmd> [args]` | Add an MCP server (stdio/sse/http) |
| `claude mcp list` | List configured servers |
| `claude mcp remove <name>` | Remove a server |
| `-e KEY=value` | Env vars for MCP server |
| `-s, --scope` | local / user / project |

### Proposed Features

#### a) MCP Server Viewer
- Read and display configured MCP servers (from `claude mcp list` output or parse `~/.claude/settings.json` and `.mcp.json`).
- Show name, transport type, scope, and status.

#### b) Add/Remove MCP Server Dialog
- Form: name, command/URL, transport (stdio/sse/http), scope, env vars.
- Calls `claude mcp add` / `claude mcp remove` under the hood.

#### c) Per-Session MCP Config
- Pass `--mcp-config <path>` at spawn time to load project-specific tools.
- Toggle `--strict-mcp-config` to isolate from global MCP servers.

### Implementation Notes
- Start simple: just display the list and allow toggling `--strict-mcp-config`.
- MCP config files: `~/.claude/settings.json` (user scope), `.mcp.json` (project scope).

---

## 4. Permission Mode Selector (Medium Priority)

### Problem
Currently hardcoded to `--dangerously-skip-permissions`. This is fine for local dev but not appropriate for all scenarios.

### CLI Capabilities
| Mode | Description |
|---|---|
| `default` | Standard prompting (interactive) |
| `acceptEdits` | Auto-accept file edits, prompt for other actions |
| `plan` | Read-only / planning mode |
| `dontAsk` | Use allowed/disallowed tool lists |
| `auto` | Automatic permission handling |
| `bypassPermissions` | Skip all checks |

Related flags:
- `--allowedTools "Bash(git:*) Edit Read"` — whitelist specific tools
- `--disallowedTools "Write"` — blacklist specific tools

### Proposed Features

#### a) Permission Mode Dropdown
- Selector in settings or header: **Plan / Accept Edits / Auto / Bypass**.
- Maps to `--permission-mode <mode>`.

#### b) Tool Allowlist/Denylist
- Advanced setting: configure which tools are permitted.
- Useful for restricting the agent to read-only operations.

### Implementation Notes
- **Update (v2.1.154):** the earlier claim that "permission prompts don't make sense
  in `-p`/`stream-json`" is **wrong**. With `--permission-prompt-tool stdio` the CLI
  routes each decision to us over the control protocol (see §0). We already removed
  `--dangerously-skip-permissions` and currently auto-allow every request. A real
  per-tool approval UI can now reuse the §0 `can_use_tool` plumbing (return
  `{"behavior":"deny","message":…}` to block).

---

## 5. System Prompt Customization (Medium Priority)

### CLI Capabilities
| Flag | Description |
|---|---|
| `--system-prompt <text>` | Replace the default system prompt |
| `--append-system-prompt <text>` | Append to the default system prompt |

### Proposed Features
- Text field in settings for a custom system prompt append.
- Presets: "Code Review", "Bug Fix", "Explain Code", "Refactor".
- Passed as `--append-system-prompt` on spawn.

---

## 6. Budget Control (Low Priority)

### CLI Capabilities
| Flag | Description |
|---|---|
| `--max-budget-usd <amount>` | Cap API spending for the session |

### Proposed Features
- Budget input field in settings (e.g., $1.00, $5.00, $20.00).
- Display running cost from `result` events (already parsed: `total_cost_usd`).
- Warning when approaching budget limit.

---

## 7. Additional Directories (Low Priority)

### CLI Capabilities
| Flag | Description |
|---|---|
| `--add-dir <dirs>` | Grant the agent access to additional directories |

### Proposed Features
- Button to add extra directories (e.g., shared libs, monorepo siblings).
- Passed as `--add-dir` args on spawn.

---

## 8. Debug & Verbose Mode (Low Priority)

### CLI Capabilities
| Flag | Description |
|---|---|
| `-d, --debug [filter]` | Debug mode with optional category filter |
| `--debug-file <path>` | Write debug logs to file |
| `--verbose` | Already used |

### Proposed Features
- Toggle debug mode for troubleshooting.
- Display or save debug logs.

---

## 9. Worktree Support (Low Priority)

### CLI Capabilities
| Flag | Description |
|---|---|
| `-w, --worktree [name]` | Create a git worktree for the session |
| `--tmux` | Create tmux session for the worktree |

### Proposed Features
- "Work in branch" toggle that creates an isolated worktree.
- Useful for experimental changes without touching the main tree.

---

## 10. Agents & Custom Agents (Future)

### CLI Capabilities
| Flag | Description |
|---|---|
| `--agent <name>` | Use a configured agent |
| `--agents <json>` | Define custom agents inline |
| `claude agents` | List available agents |

### Proposed Features
- Agent selector dropdown (list from `claude agents`).
- Custom agent creation dialog.

---

## Implementation Priority

### Phase 1 — Core (Next)
1. **Session management** — capture session_id, continue/resume, session picker
2. **Model selector** — dropdown for Haiku/Sonnet/Opus
3. **Effort selector** — Low/Medium/High/Max toggle

### Phase 2 — Configuration
4. **Permission mode** — replace dangerously-skip with configurable modes
5. **System prompt** — append custom instructions
6. **Budget control** — cost cap + display

### Phase 3 — Advanced
7. **MCP server management** — list, add, remove
8. **Additional directories** — multi-dir access
9. **Worktree support** — isolated branches
10. **Custom agents** — agent picker and definitions

---

## Spawn Command Template

After implementing these features, `AgentProcess::spawn()` would build something like:

```
claude -p
  --output-format stream-json
  --input-format stream-json
  --verbose
  --include-partial-messages
  --model <selected_model>
  --effort <selected_effort>
  --permission-mode <selected_mode>
  [--session-id <uuid>]           # if resuming
  [--name <session_name>]         # if named
  [--fork-session]                # if branching
  [--append-system-prompt <text>] # if custom prompt set
  [--max-budget-usd <amount>]     # if budget set
  [--add-dir <dir> ...]           # if extra dirs
  [--mcp-config <path>]           # if custom MCP config
  [--fallback-model <model>]      # if fallback set
```

---

## Reference: Session Storage

Sessions are stored by the CLI in `~/.claude/sessions/`. To build a session picker, we can:
1. Read session files directly from that directory
2. Or run `claude -r` which provides an interactive picker (not useful for GUI)
3. Or parse the session JSON files for metadata (id, name, timestamps, message count)

The `system` event with `session_id` is the key link between our GUI state and the CLI's session storage.
