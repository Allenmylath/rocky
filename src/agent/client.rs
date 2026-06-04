use crate::agent::context::ConversationContext;
use crate::agent::openai_client::OpenAiClient;
use crate::agent::tools::tool_definitions;
use anyhow::Result;
use serde_json::{json, Value};

pub const ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const MAX_TOKENS: u32 = 8192;

// ── Provider selection ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Provider {
    Anthropic,
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: Provider,
    pub anthropic_key: String,
    pub openai_key: String,
}

impl ProviderConfig {
    pub fn active_key(&self) -> &str {
        match self.provider {
            Provider::Anthropic => &self.anthropic_key,
            Provider::OpenAi => &self.openai_key,
        }
    }

    pub fn set_active_key(&mut self, key: String) {
        match self.provider {
            Provider::Anthropic => self.anthropic_key = key,
            Provider::OpenAi => self.openai_key = key,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.active_key().is_empty()
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        let anthropic_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        let openai_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        // Prefer Anthropic if both are set; fall back to OpenAI if only that is available.
        let provider = if !anthropic_key.is_empty() {
            Provider::Anthropic
        } else {
            Provider::OpenAi
        };
        Self { provider, anthropic_key, openai_key }
    }
}

// ── Unified client ───────────────────────────────────────────────────────────

pub enum ModelClient {
    Anthropic(AnthropicClient),
    OpenAi(OpenAiClient),
}

impl ModelClient {
    pub fn new(config: &ProviderConfig) -> Self {
        match config.provider {
            Provider::Anthropic => {
                ModelClient::Anthropic(AnthropicClient::new(config.anthropic_key.clone()))
            }
            Provider::OpenAi => {
                ModelClient::OpenAi(OpenAiClient::new(config.openai_key.clone()))
            }
        }
    }

    pub async fn send(&self, context: &ConversationContext) -> Result<Value> {
        match self {
            ModelClient::Anthropic(c) => c.send(context).await,
            ModelClient::OpenAi(c) => c.send(context).await,
        }
    }
}

// ── Anthropic client ─────────────────────────────────────────────────────────

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

    pub async fn send(&self, context: &ConversationContext) -> Result<Value> {
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
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error")
            );
        }

        Ok(value)
    }
}
