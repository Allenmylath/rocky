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

/// A single pending tool call extracted from the LLM response.
struct PendingTool {
    id: String,
    name: String,
    input: serde_json::Value,
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

        // ── 1. Collect all content blocks and pending tool calls ─────────────
        let mut api_blocks: Vec<ContentBlock> = vec![];
        let mut pending_tools: Vec<PendingTool> = vec![];

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

                    // Emit UI event before execution
                    let summary = tool_summary(&name, &input);
                    let _ = tx
                        .send(AgentEvent::ActionStarted(name.clone(), summary))
                        .await;

                    api_blocks.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });

                    pending_tools.push(PendingTool { id, name, input });
                }
                _ => {}
            }
        }

        // ── 2. Push the full assistant message (all blocks) ──────────────────
        if !api_blocks.is_empty() {
            context.push_assistant_blocks(api_blocks);
        }

        // ── 3. Execute all tools in parallel ─────────────────────────────────
        let had_tools = !pending_tools.is_empty();
        if had_tools {
            let mut handles = Vec::with_capacity(pending_tools.len());
            for tool in pending_tools {
                let root = project_root.clone();
                let tx_clone = tx.clone();
                handles.push(tokio::spawn(async move {
                    let result = execute_tool(&tool.name, &tool.input, &root, &tx_clone).await;
                    let result_str = match result {
                        Ok(s) => s,
                        Err(e) => format!("Error: {}", e),
                    };
                    (tool.id, result_str)
                }));
            }

            // Await all results and push them into the conversation
            for handle in handles {
                let (id, result_str) = handle.await?;
                context.push_tool_result(&id, result_str);
            }
        }

        if !had_tools || stop_reason == "end_turn" {
            break;
        }
    }

    let _ = tx.send(AgentEvent::TurnComplete).await;
    Ok(())
}

/// Build a short summary string for the UI workflow panel.
fn tool_summary(name: &str, input: &serde_json::Value) -> String {
    match name {
        "read_file" | "write_file" => input
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or("?")
            .to_string(),
        "read_files" => {
            let paths = input
                .get("paths")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            if paths.len() > 40 {
                format!("{} files", paths.matches(", ").count() + 1)
            } else {
                paths
            }
        }
        "write_files" => {
            let count = input
                .get("files")
                .and_then(|f| f.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            format!("{} files", count)
        }
        "list_files" => "src/".to_string(),
        "run_command" => input
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("?")
            .to_string(),
        _ => String::new(),
    }
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

        "read_files" => {
            let paths: Vec<String> = input
                .get("paths")
                .and_then(|p| p.as_array())
                .ok_or_else(|| anyhow::anyhow!("missing paths array"))?
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            if paths.is_empty() {
                anyhow::bail!("paths array is empty");
            }

            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "read_files".to_string(),
                    summary: format!("{} files", paths.len()),
                    status: StepStatus::Complete,
                }))
                .await;

            let results = fs::read::read_files_parallel(project_root, &paths).await;
            let mut out = String::new();
            for (path, content) in results {
                out.push_str(&format!("\n=== {} ===\n{}", path, content));
            }
            Ok(out)
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

        "write_files" => {
            let files_json = input
                .get("files")
                .and_then(|f| f.as_array())
                .ok_or_else(|| anyhow::anyhow!("missing files array"))?;

            let mut files: Vec<(String, String)> = Vec::with_capacity(files_json.len());
            for entry in files_json {
                let path = entry
                    .get("path")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing path in files entry"))?
                    .to_string();
                let content = entry
                    .get("content")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing content in files entry"))?
                    .to_string();
                files.push((path, content));
            }

            let written = fs::write::write_files_parallel(project_root, &files).await;

            for path in &written {
                let _ = tx.send(AgentEvent::FileWritten(path.clone())).await;
            }
            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "write_files".to_string(),
                    summary: format!("{} files", written.len()),
                    status: StepStatus::Complete,
                }))
                .await;

            Ok(format!(
                "Written: {}",
                written.join(", ")
            ))
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
