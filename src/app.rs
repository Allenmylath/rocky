use crate::agent::client::AnthropicClient;
use crate::agent::context::ConversationContext;
use crate::agent::loop_runner::{run_turn, AgentEvent};
use crate::serve::process::ServeHandle;
use crate::serve::events::BuildEvent;
use crate::state::chat::{ChatMessage, ChatState};
use crate::state::session::SessionState;

use crate::ui::root::Root;
use dioxus::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

/// App entry point — provides global signals and launches the UI
pub fn run() {
    dioxus::launch(App);
}

#[component]
pub fn App() -> Element {
    // ── Global signals ───────────────────────────────────────────────────────
    use_context_provider(|| Signal::new(SessionState::default()));
    use_context_provider(|| Signal::new(ChatState::default()));
    // The API key — in future this comes from settings UI
    use_context_provider(|| Signal::new(ApiKey(
        std::env::var("ANTHROPIC_API_KEY").unwrap_or_default()
    )));

    rsx! { Root {} }
}

/// Wrapper so we can store the API key in context
#[derive(Clone)]
pub struct ApiKey(pub String);

// ── Project lifecycle ────────────────────────────────────────────────────────

/// Called from the project picker UI when the user selects a directory.
/// Spawns `dx serve` and wires up all background tasks.
pub async fn start_project(
    project_path: PathBuf,
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    api_key: String,
) {
    // Update session with the chosen project
    session.write().project_path = Some(project_path.clone());
    session.write().serve_running = true;

    // Add a system message to chat
    chat.write().push(ChatMessage::system(format!(
        "Opened project: {}",
        project_path.display()
    )));

    // Spawn dx serve
    match ServeHandle::spawn(project_path.clone()).await {
        Ok((handle, event_rx)) => {
            // Store handle — we need to keep it alive
            // For now we just leak it; a proper solution uses a Signal<Option<ServeHandle>>
            // We'll address this when we add the stop button
            std::mem::forget(handle);

            // Start consuming build events
            spawn_build_event_loop(event_rx, session, chat.clone(), api_key, project_path);
        }
        Err(e) => {
            chat.write().push(ChatMessage::system(format!(
                "Failed to start dx serve: {}",
                e
            )));
            session.write().serve_running = false;
        }
    }
}

// ── Build event loop ─────────────────────────────────────────────────────────

/// Consumes BuildEvents from dx serve and updates session state.
/// When a build fails and auto-fix countdown expires, fires the agent.
fn spawn_build_event_loop(
    mut event_rx: tokio::sync::mpsc::Receiver<BuildEvent>,
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    api_key: String,
    project_path: PathBuf,
) {
    spawn(async move {
        let mut pending_diagnostics = vec![];

        while let Some(event) = event_rx.recv().await {
            match event {
                BuildEvent::BuildStarted => {
                    pending_diagnostics.clear();
                    session.write().on_build_started();
                    chat.write().push(ChatMessage::system("Building...".to_string()));
                }

                BuildEvent::BuildSuccess => {
                    pending_diagnostics.clear();
                    session.write().on_build_success();
                    chat.write().push(ChatMessage::system("✓ Build succeeded".to_string()));
                }

                BuildEvent::DiagnosticEmitted(diag) => {
                    if diag.is_error() {
                        pending_diagnostics.push(diag);
                    }
                }

                BuildEvent::BuildFailed => {
                    let diags = pending_diagnostics.drain(..).collect::<Vec<_>>();
                    let count = diags.len();
                    session.write().on_build_failed(diags.clone());
                    chat.write().push(ChatMessage::auto_fix(diags.clone()));

                    // Spawn the countdown task
                    spawn_countdown_loop(
                        session,
                        chat.clone(),
                        api_key.clone(),
                        project_path.clone(),
                        diags,
                    );

                    tracing::debug!("Build failed with {} errors, countdown started", count);
                }

                BuildEvent::ProcessExited { code } => {
                    session.write().serve_running = false;
                    chat.write().push(ChatMessage::system(format!(
                        "dx serve exited (code: {:?})",
                        code
                    )));
                    break;
                }

                BuildEvent::StdoutLine(line) => {
                    tracing::trace!("dx serve: {}", line);
                }
            }
        }
    });
}

// ── Countdown loop ───────────────────────────────────────────────────────────

/// Ticks every second during auto-fix countdown.
/// Fires the agent when countdown hits zero, unless the user stopped it.
fn spawn_countdown_loop(
    mut session: Signal<SessionState>,
    chat: Signal<ChatState>,
    api_key: String,
    project_path: PathBuf,
    diagnostics: Vec<crate::serve::events::RustcDiagnostic>,
) {
    spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let should_fire = session.write().tick_countdown();

            if should_fire {
                // Countdown hit zero — fire the agent
                tracing::debug!("Auto-fix countdown expired, firing agent");
                spawn_agent_turn(
                    session,
                    chat,
                    api_key,
                    project_path,
                    diagnostics,
                ).await;
                return;
            }

            // Check if stopped or no longer counting down
            let state = session.read().auto_fix.clone();
            match state {
                crate::state::session::AutoFixState::Stopped => {
                    tracing::debug!("Auto-fix stopped by user");
                    return;
                }
                crate::state::session::AutoFixState::Idle => {
                    // Build succeeded while counting down
                    return;
                }
                crate::state::session::AutoFixState::Countdown(_) => {
                    // Still counting — continue
                }
            }
        }
    });
}

// ── Agent turn ───────────────────────────────────────────────────────────────

/// Fires one agent turn with the current diagnostics as context.
/// Updates chat state as events arrive from the agent.
async fn spawn_agent_turn(
    _session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    api_key: String,
    project_path: PathBuf,
    diagnostics: Vec<crate::serve::events::RustcDiagnostic>,
) {
    chat.write().agent_running = true;

    let client = AnthropicClient::new(api_key);

    // Build conversation context from current chat history
    let mut context = ConversationContext::default();

    // Replay existing conversation into context
    {
        let chat_read = chat.read();
        for msg in chat_read.to_api_messages() {
            match msg.role.as_str() {
                "user" => context.push_user(msg.content),
                "assistant" => context.push_assistant_text(msg.content),
                _ => {}
            }
        }
    }

    // Add the auto-fix message as the new user turn
    let touched_files = chat.read().last_touched_files.clone();
    let fix_message = ConversationContext::build_auto_fix_message(&diagnostics, &touched_files);
    context.push_user(&fix_message);
    chat.write().push(ChatMessage::user(fix_message));

    // Channel for agent events
    let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel(64);

    // Run the agent turn in a separate task
    spawn(async move {
        if let Err(e) = run_turn(&client, &mut context, project_path, agent_tx).await {
            tracing::error!("Agent turn failed: {}", e);
        }
    });

    // Collect agent events and update chat state
    let mut assistant_text = String::new();
    let mut tool_calls = vec![];

    while let Some(event) = agent_rx.recv().await {
        match event {
            AgentEvent::AssistantText(text) => {
                assistant_text.push_str(&text);
            }
            AgentEvent::ToolCalled(tc) => {
                tool_calls.push(tc);
            }
            AgentEvent::FileWritten(path) => {
                chat.write().last_touched_files.push(path.clone());
                tracing::debug!("Agent wrote: {}", path);
            }
            AgentEvent::TurnComplete => {
                // Push the completed assistant message
                if !assistant_text.is_empty() || !tool_calls.is_empty() {
                    chat.write().push(ChatMessage::assistant(
                        assistant_text.clone(),
                        tool_calls.clone(),
                    ));
                }
                break;
            }
            AgentEvent::Error(e) => {
                chat.write().push(ChatMessage::system(format!("Agent error: {}", e)));
                break;
            }
        }
    }


    chat.write().agent_running = false;
}