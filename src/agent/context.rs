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
        crate::agent::prompts::system_prompt()
    }
}