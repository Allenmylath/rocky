use crate::agent::prompts;
use crate::agent::rig::{RigAgent, RigBackend, ToolAgent};
use std::path::PathBuf;

/// A collection of task-specific Rig agents for the Rocky app.
///
/// Each agent is optimized for a different workflow:
/// - **RockyCoder**: writes / edits Dioxus code (full tool access)
/// - **ErrorFixer**: diagnoses and fixes build errors
/// - **Architect**: high-level design and architecture advice
/// - **Chat**: quick Q&A without tool expectations
/// - **Brainstorm**: creative exploration of ideas
pub struct AgentSuite {
    backend: RigBackend,
}

impl AgentSuite {
    pub fn new(backend: RigBackend) -> Self {
        Self { backend }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self::new(RigBackend::from_env()?))
    }

    /// The main Dioxus coding assistant — full context, all tools.
    pub fn rocky_coder(&self) -> RigAgent {
        RigAgent::new(
            self.backend.clone(),
            self.model(),
            prompts::system_prompt(),
        )
    }

    /// Coder agent with Rig-native tool support.
    /// Rig will automatically call tools and return the final result.
    /// **Note**: this runs headless (no UI progress events).
    pub fn rocky_coder_with_tools(&self, project_root: PathBuf) -> ToolAgent {
        ToolAgent::new(
            self.backend.clone(),
            self.model(),
            prompts::system_prompt(),
            project_root,
        )
    }

    /// Surgical build-error fixer. Minimal changes only.
    pub fn error_fixer(&self) -> RigAgent {
        RigAgent::new(
            self.backend.clone(),
            self.model(),
            r#"You are a Rust/Dioxus build-error specialist.
Analyze compiler diagnostics and produce the smallest possible fix.
Never rewrite an entire file when a 1-line change will do.
Always prefer `if let` or `match` over `.unwrap()`."#,
        )
    }

    /// High-level architecture advisor.
    pub fn architect(&self) -> RigAgent {
        RigAgent::new(
            self.backend.clone(),
            self.model(),
            r#"You are a senior Rust architect specializing in Dioxus desktop apps.
You advise on project structure, state management, component design, and performance.
Explain trade-offs. Do not write code unless explicitly asked."#,
        )
    }

    /// Quick chat for simple questions — no tool expectations.
    pub fn chat(&self) -> RigAgent {
        RigAgent::new(
            self.backend.clone(),
            self.model(),
            r#"You are Rocky, a concise assistant for Dioxus and Rust.
Answer briefly. If the user needs file changes, tell them to switch to Coder mode."#,
        )
    }

    /// Brainstorming / creative mode.
    pub fn brainstorm(&self) -> RigAgent {
        RigAgent::new(
            self.backend.clone(),
            self.model(),
            r#"You are a creative technical partner.
Help the user explore ideas, compare approaches, and plan features.
No code output unless explicitly requested."#,
        )
    }

    fn model(&self) -> String {
        match &self.backend {
            RigBackend::OpenAi(_) => "o4-mini".to_string(),
            RigBackend::Anthropic(_) => "claude-sonnet-4-5".to_string(),
        }
    }
}
