use crate::agent::client::{ModelClient, ProviderConfig};
use crate::agent::context::ConversationContext;
use crate::agent::loop_runner::{run_turn, AgentEvent};
use crate::serve::events::BuildEvent;
use crate::serve::process::ServeHandle;
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
    let session = use_context_provider(|| Signal::new(SessionState::default()));
    let chat = use_context_provider(|| Signal::new(ChatState::default()));
    let config_sig = use_context_provider(|| Signal::new(ProviderConfig::default()));

    // Auto-reopen the last project on launch
    use_effect(move || {
        let config = config_sig.read().clone();
        spawn(async move {
            if let Some(path) = crate::config::load_last_project() {
                start_project(path, session, chat, config).await;
            }
        });
    });

    rsx! { Root {} }
}

// ── Project lifecycle ────────────────────────────────────────────────────────

/// Called from the project picker when the user selects a directory.
pub async fn start_project(
    project_path: PathBuf,
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    config: ProviderConfig,
) {
    session.write().project_path = Some(project_path.clone());
    session.write().serve_running = true;

    crate::config::save_last_project(&project_path);

    chat.write().push(ChatMessage::system(format!(
        "Opened project: {}",
        project_path.display()
    )));

    match ServeHandle::spawn(project_path.clone()).await {
        Ok((handle, event_rx)) => {
            std::mem::forget(handle);
            spawn_build_event_loop(event_rx, session, chat.clone(), config, project_path);
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

fn spawn_build_event_loop(
    mut event_rx: tokio::sync::mpsc::Receiver<BuildEvent>,
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    config: ProviderConfig,
    project_path: PathBuf,
) {
    spawn(async move {
        let mut pending_diagnostics = vec![];
        let mut browser_opened = false;

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

                    if !browser_opened {
                        browser_opened = true;
                        if let Err(e) = open::that_detached("http://localhost:8080") {
                            tracing::warn!("Failed to open browser: {}", e);
                        }
                    }
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

                    spawn_countdown_loop(
                        session,
                        chat.clone(),
                        config.clone(),
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
                    session.write().push_log(line);
                }
            }
        }
    });
}

// ── Countdown loop ───────────────────────────────────────────────────────────

fn spawn_countdown_loop(
    mut session: Signal<SessionState>,
    chat: Signal<ChatState>,
    config: ProviderConfig,
    project_path: PathBuf,
    diagnostics: Vec<crate::serve::events::RustcDiagnostic>,
) {
    spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let should_fire = session.write().tick_countdown();

            if should_fire {
                tracing::debug!("Auto-fix countdown expired, firing agent");
                spawn_agent_turn(session, chat, config, project_path, diagnostics).await;
                return;
            }

            let state = session.read().auto_fix.clone();
            match state {
                crate::state::session::AutoFixState::Stopped => {
                    tracing::debug!("Auto-fix stopped by user");
                    return;
                }
                crate::state::session::AutoFixState::Idle => return,
                crate::state::session::AutoFixState::Countdown(_) => {}
            }
        }
    });
}

// ── Agent turn ───────────────────────────────────────────────────────────────

async fn spawn_agent_turn(
    _session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    config: ProviderConfig,
    project_path: PathBuf,
    diagnostics: Vec<crate::serve::events::RustcDiagnostic>,
) {
    chat.write().agent_running = true;

    let client = ModelClient::new(&config);
    let mut context = ConversationContext::default();

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

    let touched_files = chat.read().last_touched_files.clone();
    let fix_message = ConversationContext::build_auto_fix_message(&diagnostics, &touched_files);
    context.push_user(&fix_message);
    chat.write().push(ChatMessage::user(fix_message));

    let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel(64);

    spawn(async move {
        if let Err(e) = run_turn(&client, &mut context, project_path, agent_tx).await {
            tracing::error!("Agent turn failed: {}", e);
        }
    });

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
