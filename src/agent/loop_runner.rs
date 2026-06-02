use crate::agent::client::AnthropicClient;
use crate::agent::context::{ContentBlock, ConversationContext};
use crate::fs;
use crate::state::chat::ToolCall;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Events the agent loop sends back to the UI
#[derive(Debug)]
pub enum AgentEvent {
    /// Assistant text response chunk
    AssistantText(String),
    /// A tool was called
    ToolCalled(ToolCall),
    /// A file was written — triggers dx serve hot reload
    FileWritten(String),
    /// The agent turn is complete
    TurnComplete,
    /// An error occurred in the agent loop
    Error(String),
}

/// Run one agent turn.
/// Sends events back over `tx` as the turn progresses.
pub async fn run_turn(
    client: &AnthropicClient,
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

        // Collect content blocks from response
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

                    api_blocks.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });

                    // Execute the tool
                    let result = execute_tool(&name, &input, &project_root, &tx).await;
                    let result_str = match result {
                        Ok(s) => s,
                        Err(e) => format!("Error: {}", e),
                    };

                    // Push assistant blocks and tool result to context
                    context.push_assistant_blocks(api_blocks.clone());
                    context.push_tool_result(&id, result_str);
                    api_blocks.clear();
                }
                _ => {}
            }
        }

        // Push any remaining assistant blocks
        if !api_blocks.is_empty() {
            context.push_assistant_blocks(api_blocks);
        }

        // If no tool use or stop_reason is end_turn, we're done
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

            let _ = tx
                .send(AgentEvent::FileWritten(path.to_string()))
                .await;
            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "write_file".to_string(),
                    summary: path.to_string(),
                }))
                .await;

            Ok(format!("Written: {}", path))
        }

        "list_files" => {
            let _ = tx
                .send(AgentEvent::ToolCalled(ToolCall {
                    name: "list_files".to_string(),
                    summary: "src/".to_string(),
                }))
                .await;

            let files = fs::list::list_src_files(project_root).await?;
            Ok(files.join("\n"))
        }

        unknown => anyhow::bail!("Unknown tool: {}", unknown),
    }
}