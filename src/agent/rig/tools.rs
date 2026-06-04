use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── ReadFile ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
}

#[derive(Serialize)]
pub struct ReadFileOutput {
    pub content: String,
}

#[derive(Clone)]
pub struct ReadFile {
    pub project_root: PathBuf,
}

impl Tool for ReadFile {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = ReadFileOutput;
    type Error = anyhow::Error;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read the contents of a file in the target Dioxus project.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root, e.g. 'src/main.rs'"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let content = crate::fs::read::read_file(&self.project_root, &args.path).await?;
        Ok(ReadFileOutput { content })
    }
}

// ── WriteFile ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct WriteFileOutput {
    pub path: String,
}

#[derive(Clone)]
pub struct WriteFile {
    pub project_root: PathBuf,
}

impl Tool for WriteFile {
    const NAME: &'static str = "write_file";
    type Args = WriteFileArgs;
    type Output = WriteFileOutput;
    type Error = anyhow::Error;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Write or overwrite a file in the target Dioxus project. \
Creates parent directories if needed.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root, e.g. 'src/components/header.rs'"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        crate::fs::write::write_file(&self.project_root, &args.path, &args.content).await?;
        Ok(WriteFileOutput { path: args.path })
    }
}

// ── ListFiles ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListFilesArgs {}

#[derive(Serialize)]
pub struct ListFilesOutput {
    pub files: Vec<String>,
}

#[derive(Clone)]
pub struct ListFiles {
    pub project_root: PathBuf,
}

impl Tool for ListFiles {
    const NAME: &'static str = "list_files";
    type Args = ListFilesArgs;
    type Output = ListFilesOutput;
    type Error = anyhow::Error;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List all Rust source files in the project's src/ directory, \
plus root-level config files (Cargo.toml, Dioxus.toml).".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let files = crate::fs::list::list_src_files(&self.project_root).await?;
        Ok(ListFilesOutput { files })
    }
}

// ── RunCommand ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RunCommandArgs {
    pub command: String,
}

#[derive(Serialize)]
pub struct RunCommandOutput {
    pub stdout: String,
}

#[derive(Clone)]
pub struct RunCommand {
    pub project_root: PathBuf,
}

impl Tool for RunCommand {
    const NAME: &'static str = "run_command";
    type Args = RunCommandArgs;
    type Output = RunCommandOutput;
    type Error = anyhow::Error;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run a cargo or dx command in the project directory and return its output. \
Use for: fixing dependencies, formatting, checking, scaffolding.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to run, e.g. 'cargo add serde@1' or 'cargo fmt'"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let stdout = crate::fs::command::run_command(&self.project_root, &args.command).await?;
        Ok(RunCommandOutput { stdout })
    }
}
