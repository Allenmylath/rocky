use crate::agent::client::ModelClient;
use crate::agent::context::{ContentBlock, ConversationContext};
use crate::fs;
use crate::state::chat::{StepStatus, ToolCall};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AgentEvent {
    AssistantText(String),
    /// Tool action started (name, summary) — shown as "running" in workflow
    ActionStarted(String, String),
    ToolCalled(ToolCall),
    FileWritten(String),
    TurnComplete,
    Error(String),
}

pub async fn run_turn(
    client: &ModelClient,
    context: &mut ConversationContext,
    project_root: PathBuf,
    tx: mpsc::Sender<AgentEvent>,
) -> anyhow::Result<()> {
    loop {
        let response = client.send(context).await?;

        let stop_reason = response
            .get("stop_reason")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        let content_blocks = response
            .get("content")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let mut api_blocks: Vec<ContentBlock> = vec![];
        let mut has_tool_use = false;

        for block in &content_blocks {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match block_type {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !text.is_empty() {
                        let _ = tx.send(AgentEvent::AssistantText(text.clone())).await;
                        api_blocks.push(ContentBlock::Text { text });
                    }
                }
                "tool_use" => {
                    has_tool_use = true;
                    let id = block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input = block
                        .get("input")
                        .cloned()
                        .unwrap_or(serde_json::json!({}));

                    let summary = match name.as_str() {
                        "read_file" | "write_file" => input
                            .get("path")
                            .and_then(|p| p.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        "list_files" => "src/".to_string(),
                        "run_command" => input
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        _ => String::new(),
                    };

                    let _ = tx
                        .send(AgentEvent::ActionStarted(name.clone(), summary.clone()))
                        .await;

                    api_blocks.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });

                    let result = execute_tool(&name, &input, &project_root, &tx).await;
                    let result_str = match result {
                        Ok(s) => s,
                        Err(e) => format!("Error: {}", e),
                    };

                    context.push_assistant_blocks(api_blocks.clone());
                    context.push_tool_result(&id, result_str);
                    api_blocks.clear();
                }
                _ => {}
            }
        }

        if !api_blocks.is_empty() {
            context.push_assistant_blocks(api_blocks);
        }

        if !has_tool_use || stop_reason == "end_turn" {
            break;
        }
    }

    let _ = tx.send(AgentEvent::TurnComplete).await;
    Ok(())
}

async fn execute_tool(
    name: &str,
    input: &serde_json::Value,
    project_root: &PathBuf,
    tx: &mpsc::Sender<AgentEvent>,
) -> anyhow::Result<String> {
    match name {
        "read_file" => {
            let path = input
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing path"))?;

            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "read_file".to_string(),
                    summary: path.to_string(),
                    status: StepStatus::Complete,
                }))
                .await;

            fs::read::read_file(project_root, path).await
        }

        "write_file" => {
            let path = input
                .get("path")
                .and_then(|p| p.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing path"))?;
            let content = input
                .get("content")
                .and_then(|c| c.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing content"))?;

            fs::write::write_file(project_root, path, content).await?;

            let _ = tx.send(AgentEvent::FileWritten(path.to_string())).await;
            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "write_file".to_string(),
                    summary: path.to_string(),
                    status: StepStatus::Complete,
                }))
                .await;

            Ok(format!("Written: {}", path))
        }

        "list_files" => {
            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "list_files".to_string(),
                    summary: "src/".to_string(),
                    status: StepStatus::Complete,
                }))
                .await;

            let files = fs::list::list_src_files(project_root).await?;
            Ok(files.join("\n"))
        }

        "run_command" => {
            let command = input
                .get("command")
                .and_then(|c| c.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing command"))?;

            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "run_command".to_string(),
                    summary: command.to_string(),
                    status: StepStatus::Complete,
                }))
                .await;

            fs::command::run_command(project_root, command).await
        }

        unknown => anyhow::bail!("Unknown tool: {}", unknown),
    }
}
