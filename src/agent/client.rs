use crate::agent::context::ConversationContext;
use crate::agent::tools::tool_definitions;
use anyhow::Result;
use serde_json::{json, Value};

pub const ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const MAX_TOKENS: u32 = 8192;

pub struct AnthropicClient {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
        }
    }

    /// Send a message to the Anthropic API and return the raw response Value.
    /// The agent loop in loop_runner.rs handles tool-use parsing.
    pub async fn send(
        &self,
        context: &ConversationContext,
    ) -> Result<Value> {
        let body = json!({
            "model": ANTHROPIC_MODEL,
            "max_tokens": MAX_TOKENS,
            "system": ConversationContext::system_prompt(),
            "tools": tool_definitions(),
            "messages": context.messages,
        });

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let value: Value = response.json().await?;

        if !status.is_success() {
            anyhow::bail!(
                "Anthropic API error {}: {}",
                status,
                value.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
            );
        }

        Ok(value)
    }
}