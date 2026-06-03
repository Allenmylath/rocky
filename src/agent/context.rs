use crate::serve::events::RustcDiagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single message in the Anthropic API format (supports tool use)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: ApiContent,
}

/// Content can be a plain string or an array of content blocks (for tool use)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// The full conversation history — passed to Anthropic on each turn
#[derive(Debug, Clone, Default)]
pub struct ConversationContext {
    pub messages: Vec<ApiMessage>,
}

impl ConversationContext {
    pub fn push_user(&mut self, text: impl Into<String>) {
        self.messages.push(ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(text.into()),
        });
    }

    pub fn push_assistant_text(&mut self, text: impl Into<String>) {
        self.messages.push(ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text(text.into()),
        });
    }

    pub fn push_assistant_blocks(&mut self, blocks: Vec<ContentBlock>) {
        self.messages.push(ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Blocks(blocks),
        });
    }

    pub fn push_tool_result(&mut self, tool_use_id: impl Into<String>, result: impl Into<String>) {
        self.messages.push(ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: result.into(),
            }]),
        });
    }

    /// Build the auto-fix user message from diagnostics
    /// This is the Dyad-equivalent: surgical error-only context
    pub fn build_auto_fix_message(
        diagnostics: &[RustcDiagnostic],
        touched_files: &[String],
    ) -> String {
        let error_count = diagnostics.iter().filter(|d| d.is_error()).count();

        let mut msg = format!(
            "The build failed with {} error{}. Please fix them.\n\n",
            error_count,
            if error_count == 1 { "" } else { "s" }
        );

        // Group by target for clarity
        let server_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.is_error()
                    && matches!(
                        d.target,
                        crate::state::session::BuildTarget::Server
                            | crate::state::session::BuildTarget::Unknown
                    )
            })
            .collect();

        let client_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.is_error()
                    && matches!(d.target, crate::state::session::BuildTarget::Client)
            })
            .collect();

        if !server_errors.is_empty() {
            msg.push_str("## Server errors\n\n");
            for diag in &server_errors {
                msg.push_str(&diag.to_prompt_block());
                msg.push('\n');
            }
        }

        if !client_errors.is_empty() {
            msg.push_str("## Client (WASM) errors\n\n");
            for diag in &client_errors {
                msg.push_str(&diag.to_prompt_block());
                msg.push('\n');
            }
        }

        if !touched_files.is_empty() {
            msg.push_str("## Files you last edited\n\n");
            for f in touched_files {
                msg.push_str(&format!("- {}\n", f));
            }
        }

        msg
    }

    pub fn system_prompt() -> String {
        r#"You are Rocky's AI agent. Rocky is a desktop app that opens a user's Dioxus project and runs `dx serve` for it automatically. You are embedded inside Rocky.

## What Rocky already does (do NOT explain this to users)
- Rocky has already started `dx serve` for the open project.
- Build output (compiler errors, warnings, success) is shown in Rocky's right panel.
- The top bar shows the live build status: "building...", "✓ ready", or "✗ N errors".
- Hot reload is automatic — when you write a file, `dx serve` detects it and rebuilds.
- You do NOT need to tell users how to run cargo, dx serve, or any build commands.

## When users ask about the build / output / status
Answer in ONE sentence pointing them to Rocky's UI. Example:
"Build output is in the right panel — errors show there with file and line, and the top bar shows the live status."
Do NOT explain cargo commands. Do NOT explain dx serve. Do NOT give generic Rust build instructions.

## Tools
- list_files  — list all .rs files in src/
- read_file   — read any file in the project
- write_file  — write/create a file (creates parent dirs; triggers dx serve hot reload)

## When building or changing code — tools first, always
1. Call list_files IMMEDIATELY. No preamble, no planning text before tool calls.
2. Read relevant files (main.rs, Cargo.toml, existing modules).
3. Call write_file for every file to create or change. One call per file.
4. After all files are written, send ONE short sentence summarising what was done.

NEVER output a code block. NEVER explain what you are about to do before doing it.
NEVER ask "would you like me to proceed?" — complete the full feature in one turn.
NEVER stop halfway and ask for permission to continue.

## Wiring rules
- New module `foo`? Add `mod foo;` to the owning file (usually main.rs or a parent mod.rs).
- New component? Import and render it in its parent.
- New dependency needed? Flag it in your one-line summary — you cannot edit Cargo.toml.
- Dioxus desktop: use `#[component]` and `rsx!`. No Axum server unless Cargo.toml already has one.

## Fixing build errors
1. Read the erroring files.
2. Make minimal targeted changes.
3. Write the corrected file(s). No explanation needed.
"#
        .to_string()
    }
}