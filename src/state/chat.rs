use crate::serve::events::RustcDiagnostic;
use std::time::{SystemTime, UNIX_EPOCH};

/// Who sent this message
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    /// System-generated — e.g. "Build failed with 3 errors, auto-fixing..."
    System,
}

/// A single tool call the agent made — stored for display in UI
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    /// e.g. the file path that was read/written
    pub summary: String,
}

/// A single message in the conversation
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: u64,
    pub role: MessageRole,
    pub content: String,
    /// Tool calls made during this assistant turn
    pub tool_calls: Vec<ToolCall>,
    /// If this message was triggered by a build failure, attach the diagnostics
    pub triggered_by: Option<Vec<RustcDiagnostic>>,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::User,
            content: content.into(),
            tool_calls: vec![],
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls,
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::System,
            content: content.into(),
            tool_calls: vec![],
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn auto_fix(diagnostics: Vec<RustcDiagnostic>) -> Self {
        let count = diagnostics.iter().filter(|d| d.is_error()).count();
        Self {
            id: now_ms(),
            role: MessageRole::System,
            content: format!(
                "Build failed with {} error{}. Auto-fixing...",
                count,
                if count == 1 { "" } else { "s" }
            ),
            tool_calls: vec![],
            triggered_by: Some(diagnostics),
            timestamp: now_ms(),
        }
    }
}

/// The full conversation — this is what gets sent to Anthropic on each turn
#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    /// Files the agent touched in the last turn — used for context on next loop
    pub last_touched_files: Vec<String>,
    /// Whether the agent is currently running
    pub agent_running: bool,
}

impl ChatState {
    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.last_touched_files.clear();
    }

    /// Build the Anthropic message history from chat state
    /// Only includes user + assistant turns (not system messages)
    pub fn to_api_messages(&self) -> Vec<ApiMessage> {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| ApiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect()
    }
}

/// Minimal Anthropic API message shape
/// Full tool-use messages are built in agent/context.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}