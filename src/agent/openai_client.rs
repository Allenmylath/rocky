use crate::agent::context::{ApiContent, ContentBlock, ConversationContext};
use crate::agent::tools::tool_definitions;
use anyhow::Result;
use serde_json::{json, Value};

pub const OPENAI_MODEL: &str = "gpt-4o";
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiClient {
    client: reqwest::Client,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
        }
    }

    /// Send a message and return a response normalized to Anthropic's JSON shape
    /// so that loop_runner.rs can parse it without any provider-specific logic.
    pub async fn send(&self, context: &ConversationContext) -> Result<Value> {
        let mut messages = vec![
            json!({"role": "system", "content": ConversationContext::system_prompt()}),
        ];
        messages.extend(to_openai_messages(context));

        let body = json!({
            "model": OPENAI_MODEL,
            "max_tokens": 8192,
            "messages": messages,
            "tools": openai_tool_definitions(),
            "tool_choice": "auto",
        });

        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let value: Value = response.json().await?;

        if !status.is_success() {
            anyhow::bail!(
                "OpenAI API error {}: {}",
                status,
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error")
            );
        }

        normalize_openai_response(value)
    }
}

/// Convert ConversationContext (Anthropic format) → OpenAI message array.
/// One Anthropic message maps to one or more OpenAI messages (tool results split out).
fn to_openai_messages(context: &ConversationContext) -> Vec<Value> {
    context
        .messages
        .iter()
        .flat_map(|msg| match &msg.content {
            ApiContent::Text(text) => vec![json!({"role": msg.role, "content": text})],

            ApiContent::Blocks(blocks) if msg.role == "assistant" => {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let tool_calls: Vec<Value> = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": input.to_string()
                            }
                        })),
                        _ => None,
                    })
                    .collect();

                if tool_calls.is_empty() {
                    vec![json!({"role": "assistant", "content": text})]
                } else {
                    vec![json!({"role": "assistant", "content": text, "tool_calls": tool_calls})]
                }
            }

            ApiContent::Blocks(blocks) => {
                // User role with tool results — each becomes a separate "tool" message
                blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                        } => Some(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content
                        })),
                        _ => None,
                    })
                    .collect()
            }
        })
        .collect()
}

/// Map OpenAI response → Anthropic-like JSON so loop_runner.rs needs zero changes.
fn normalize_openai_response(value: Value) -> Result<Value> {
    let choice = value["choices"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| anyhow::anyhow!("No choices in OpenAI response"))?;

    let stop_reason = match choice["finish_reason"].as_str().unwrap_or("stop") {
        "tool_calls" => "tool_use",
        _ => "end_turn",
    };

    let message = &choice["message"];
    let mut content_blocks: Vec<Value> = vec![];

    if let Some(text) = message["content"].as_str() {
        if !text.is_empty() {
            content_blocks.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for tc in tool_calls {
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            content_blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input
            }));
        }
    }

    Ok(json!({"stop_reason": stop_reason, "content": content_blocks}))
}

/// Anthropic uses `input_schema`; OpenAI uses `parameters` under `function`.
fn openai_tool_definitions() -> Vec<Value> {
    tool_definitions()
        .into_iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool["name"],
                    "description": tool["description"],
                    "parameters": tool["input_schema"]
                }
            })
        })
        .collect()
}
