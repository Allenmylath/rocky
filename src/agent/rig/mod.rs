pub mod agents;
pub mod tools;

use rig_core::providers::{anthropic, openai};
use rig_core::completion::{Chat, Message, Prompt};

/// Which LLM provider to use via Rig.
#[derive(Clone, Debug)]
pub enum RigBackend {
    OpenAi(String),
    Anthropic(String),
}

impl RigBackend {
    pub fn from_env() -> anyhow::Result<Self> {
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                return Ok(RigBackend::Anthropic(key));
            }
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if !key.is_empty() {
                return Ok(RigBackend::OpenAi(key));
            }
        }
        anyhow::bail!("No API key found. Set OPENAI_API_KEY or ANTHROPIC_API_KEY.")
    }
}

/// A Rig-backed agent. We rebuild the underlying Rig `Agent<M>` on every call
/// because the type is generic over the completion model.
pub struct RigAgent {
    backend: RigBackend,
    model: String,
    preamble: String,
}

impl RigAgent {
    pub fn new(
        backend: RigBackend,
        model: impl Into<String>,
        preamble: impl Into<String>,
    ) -> Self {
        Self {
            backend,
            model: model.into(),
            preamble: preamble.into(),
        }
    }

    /// One-shot prompt. If the agent has tools registered (see `ToolAgent`),
    /// Rig will auto-call them and return the final text result.
    pub async fn prompt(&self, text: &str) -> anyhow::Result<String> {
        match &self.backend {
            RigBackend::OpenAi(key) => {
                let client = openai::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .build();
                Ok(agent.prompt(text).await?)
            }
            RigBackend::Anthropic(key) => {
                let client = anthropic::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .build();
                Ok(agent.prompt(text).await?)
            }
        }
    }

    /// Chat with history. `history` should contain previous turns.
    /// The new `prompt` is the latest user message.
    pub async fn chat(&self, prompt: &str, history: Vec<Message>) -> anyhow::Result<String> {
        match &self.backend {
            RigBackend::OpenAi(key) => {
                let client = openai::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .build();
                Ok(agent.chat(prompt, history).await?)
            }
            RigBackend::Anthropic(key) => {
                let client = anthropic::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .build();
                Ok(agent.chat(prompt, history).await?)
            }
        }
    }
}

/// A Rig-backed agent that carries native tools (read_file, write_file, etc.).
/// Because `Agent<M>` is generic, we rebuild it on every prompt.
pub struct ToolAgent {
    backend: RigBackend,
    model: String,
    preamble: String,
    project_root: std::path::PathBuf,
}

impl ToolAgent {
    pub fn new(
        backend: RigBackend,
        model: impl Into<String>,
        preamble: impl Into<String>,
        project_root: std::path::PathBuf,
    ) -> Self {
        Self {
            backend,
            model: model.into(),
            preamble: preamble.into(),
            project_root,
        }
    }

    /// Prompt the agent. Rig handles the tool loop automatically.
    /// **Note**: this runs headless — UI progress events are not emitted.
    /// For UI-integrated tool calling, use the existing `loop_runner` path.
    pub async fn prompt(&self, text: &str) -> anyhow::Result<String> {
        match &self.backend {
            RigBackend::OpenAi(key) => {
                let client = openai::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .tool(tools::ReadFile {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::WriteFile {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::ListFiles {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::RunCommand {
                        project_root: self.project_root.clone(),
                    })
                    .build();
                Ok(agent.prompt(text).await?)
            }
            RigBackend::Anthropic(key) => {
                let client = anthropic::Client::new(key);
                let agent = client
                    .agent(&self.model)
                    .preamble(&self.preamble)
                    .tool(tools::ReadFile {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::WriteFile {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::ListFiles {
                        project_root: self.project_root.clone(),
                    })
                    .tool(tools::RunCommand {
                        project_root: self.project_root.clone(),
                    })
                    .build();
                Ok(agent.prompt(text).await?)
            }
        }
    }
}
