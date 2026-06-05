# Rocky — Agent Guide

Rocky is an AI-powered coding assistant desktop app for Dioxus/Rust projects. It runs `dx serve` on a target project, watches for compiler errors, and auto-fixes them using an LLM agent.

## What Rocky Does

- Opens a Dioxus project and runs `dx serve` in the background
- Parses Rust compiler diagnostics from the build output
- Sends errors to an LLM (Anthropic Claude or OpenAI) with file-reading/writing tools
- Automatically applies fixes and rebuilds
- Shows a chat-style workflow UI with build steps, tool calls, and agent turns

## Tech Stack

- **UI**: Dioxus 0.7.9 (desktop/webview)
- **Async runtime**: Tokio (full features)
- **LLM API**: Direct HTTP via `reqwest` (Anthropic + OpenAI), plus `rig-core` for typed tool abstractions
- **Error handling**: `anyhow`
- **Serialization**: `serde` + `serde_json`
- **Logging**: `tracing`
- **File dialogs**: `rfd`

## Project Structure

```
src/
├── main.rs          # Entry point — init tracing, load .env, launch app
├── app.rs           # App component, project lifecycle, build event loop, agent turn orchestration
├── config.rs        # Project root discovery (walk up to Cargo.toml), last-project persistence
├── agent/           # LLM client, conversation context, agent loop, tools
│   ├── client.rs        # ProviderConfig, ModelClient (unified Anthropic/OpenAI wrapper)
│   ├── context.rs       # ConversationContext (message history, system prompt, tool result formatting)
│   ├── loop_runner.rs   # run_turn() — the core agent loop: send → parse blocks → execute tools → repeat
│   ├── openai_client.rs # OpenAI-specific HTTP client
│   ├── prompts.rs       # Prompt templates (system, auto-fix, continue, etc.)
│   ├── tools.rs         # Tool definition JSON schema for the LLM
│   └── rig/             # Alternative Rig-based agent layer (currently unused but compiled)
│       ├── mod.rs       # RigAgent, ToolAgent — wrappers around rig-core providers
│       ├── agents.rs    # AgentSuite — pre-configured agents (coder, fixer, architect, etc.)
│       └── tools.rs     # Rig Tool trait implementations (ReadFile, WriteFile, ListFiles, RunCommand)
├── fs/              # File system operations used by tools
│   ├── read.rs
│   ├── write.rs
│   ├── list.rs
│   └── command.rs
├── serve/           # dx serve process management
│   ├── process.rs   # ServeHandle::spawn() — runs dx serve, returns (handle, event_rx)
│   ├── parser.rs    # StderrParser — parses dx serve stderr into BuildEvent stream
│   └── events.rs    # BuildEvent enum (BuildStarted, BuildSuccess, BuildFailed, DiagnosticEmitted, etc.)
├── state/           # Shared application state (Dioxus Signals)
│   ├── chat.rs      # ChatState, ChatMessage, WorkflowStep, StepKind, StepStatus, ToolCall
│   └── session.rs   # SessionState (project path, build status, raw log, auto-fix countdown)
└── ui/              # UI components
    ├── root.rs      # Root layout (project picker + chat panel)
    ├── chat.rs      # Chat message rendering
    ├── workflow.rs  # Workflow step / turn rendering
    ├── diagnostics.rs
    └── project_picker.rs
```

## Build & Run

```bash
# Check compilation
cargo check

# Run the desktop app
cargo run

# Or use the Dioxus CLI
dx serve --platform desktop
```

## Environment Variables

Create a `.env` file in the project root:

```
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
```

`ANTHROPIC_API_KEY` is preferred if both are set.

## Architecture Overview

### 1. Project Lifecycle (`app.rs`)

When a project is opened:
1. `start_project()` calls `ServeHandle::spawn(project_path)`
2. Spawns `spawn_build_event_loop()` which listens to `BuildEvent`s
3. On `BuildFailed`, starts a countdown (configurable), then spawns an agent turn
4. On `ProcessExited` with fatal error, spawns `spawn_fatal_fix_turn()`

### 2. Build Event Loop (`app.rs`)

Receives `BuildEvent` from `dx serve`:
- `BuildStarted` → clears diagnostics, begins a workflow turn
- `BuildSuccess` → completes turn, opens browser on first success
- `DiagnosticEmitted(diag)` → collects errors
- `BuildFailed` → completes turn, triggers auto-fix agent turn after countdown
  - **Cargo-check fallback**: If the parsed diagnostics lack file/line info (dx
    only emitted vague Format A error lines), Rocky automatically runs
    `cargo check` in the project directory and swaps in the full rustc
    diagnostics with `--> file:line:col` locations and snippets before
    sending them to the LLM.
- `ProcessExited { code }` → if fatal startup failure, feeds raw log to agent

### 3. Agent Turn (`agent/loop_runner.rs`)

`run_turn()` is the core loop:
1. Send conversation context to LLM via `ModelClient::send()`
2. Parse response content blocks (`text` and `tool_use`)
3. Collect **all** `tool_use` blocks from the response
4. Push the complete assistant message (all blocks) to context **once**
5. Execute **all tools in parallel** via `tokio::spawn`
6. Push all tool results back into conversation context
7. Loop until LLM stops with `end_turn` or no tool_use blocks

**Why parallel?** If the LLM asks to read 3 files in one turn, all 3 reads happen concurrently instead of sequentially.

### 4. Tools

| Tool | Description |
|------|-------------|
| `read_file` | Read a single file relative to project root |
| `read_files` | Read multiple files in parallel (batch) |
| `write_file` | Write/overwrite a single file (creates parent dirs) |
| `write_files` | Write multiple files in parallel (batch) |
| `list_files` | List `src/**/*.rs` + root config files |
| `run_command` | Run `cargo` or `dx` command in project directory |

### 5. State Management

Dioxus Signals are used throughout:
- `SessionState` — project path, serve_running, build_status, raw_log, auto-fix state
- `ChatState` — message history, workflow turns, agent_running flag, last_touched_files
- `ProviderConfig` — selected provider + API keys

Signals are provided at the root `App` component via `use_context_provider()` and consumed with `use_context()`.

## Key Conventions

### Dioxus-specific
- Use `rsx!` macro for JSX-like UI
- **Never use `<form>` elements** — use button `onclick` handlers instead
- Signals are `Copy` — move them into closures freely
- Use `spawn()` for fire-and-forget async; `use_future()` for async bound to component lifetime
- `use_context_provider()` / `use_context::<Signal<T>>()` for shared state

### Agent code
- `anyhow::Result` for error propagation
- Tool errors in `agent/rig/tools.rs` use `ToolError` (custom `std::error::Error` impl), NOT `anyhow::Error`, because `rig_core::tool::Tool::Error` requires `std::error::Error`
- `ConversationContext` manages message formatting for the LLM API
- `ContentBlock` enum tracks text vs tool_use vs tool_result blocks

### Rig integration (`agent/rig/`)
- `CompletionClient` trait must be in scope to call `.agent()` on a Rig client
- `openai::Client::new(key)` and `anthropic::Client::new(key)` return `Result<Client, Error>` — use `?`
- `agent.chat()` takes `&mut Vec<Message>` (mutable borrow)

## Adding New Features

### Adding a new tool
1. Add JSON schema to `agent/tools.rs::tool_definitions()`
2. Add execution logic to `agent/loop_runner.rs::execute_tool()`
3. Add UI step kind to `state/chat.rs::StepKind` if needed
4. Map tool name to `StepKind` in `app.rs` where `ActionStarted` is handled (both `spawn_agent_turn` and `spawn_fatal_fix_turn`)
5. Update `TOOL_NAMES` constant if you use it for validation

### Adding a new UI component
1. Create file in `src/ui/`
2. Export in `src/ui/mod.rs`
3. Import and use in parent component (usually `root.rs` or `app.rs`)

### Changing the LLM provider/model
- Edit `agent/client.rs` — `ANTHROPIC_MODEL` constant or OpenAI model string
- `ProviderConfig::default()` reads from env vars and prefers Anthropic

## Common Issues

- **`anyhow::Error` doesn't implement `std::error::Error` for rig tools** → Use `ToolError` wrapper
- **`.agent()` not found on Client** → Import `rig_core::client::CompletionClient`
- **`.agent()` not found on `Result<T, E>`** → Add `?` after `Client::new()`
- **`chat()` expects `&mut Vec<Message>`** → Declare parameter as `mut history` and pass `&mut history`
