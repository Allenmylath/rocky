use crate::agent::client::{ModelClient, ProviderConfig};
use crate::agent::context::ConversationContext;
use crate::agent::loop_runner::{run_turn, AgentEvent};
use crate::serve::events::{BuildEvent, BuildStage};
use crate::serve::process::ServeHandle;
use crate::fs::read::read_files_parallel;
use crate::state::chat::{ChatMessage, ChatState, StepKind, StepStatus, WorkflowStep};
use crate::state::session::SessionState;
use crate::ui::root::Root;
use dioxus::desktop::{use_window, Config, WindowBuilder};
use dioxus::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

pub fn run() {
    let window = WindowBuilder::new()
        .with_title("Rocky")
        .with_maximized(true);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(App);
}

#[component]
pub fn App() -> Element {
    let session = use_context_provider(|| Signal::new(SessionState::default()));
    let chat = use_context_provider(|| Signal::new(ChatState::default()));
    let config_sig = use_context_provider(|| Signal::new(ProviderConfig::default()));
    let win = use_window();

    use_effect(move || {
        win.set_maximized(true);
    });

    use_effect(move || {
        let config = config_sig.read().clone();
        spawn(async move {
            if let Some(path) = crate::config::load_last_project() {
                start_project(path, session, chat, config, false).await;
            }
        });
    });

    rsx! { Root {} }
}

// ── Project lifecycle ────────────────────────────────────────────────────────

pub async fn start_project(
    project_path: PathBuf,
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    config: ProviderConfig,
    is_sample: bool,
) {
    session.write().project_path = Some(project_path.clone());
    session.write().serve_running = true;
    session.write().is_sample_project = is_sample;

    if !is_sample {
        crate::config::save_last_project(&project_path);
    }

    chat.write().push(ChatMessage::system(format!(
        "Opened project: {}",
        project_path.display()
    )));

    {
        let mut c = chat.write();
        let idx = c.begin_turn("dx serve → starting");
        c.workflow_turns[idx].push_step(WorkflowStep::build("spawning dx serve..."));
    }

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
            chat.write().finish_turn(StepStatus::Failed);
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
        let mut build_turn_active = true;
        let mut had_startup_failure = false;

        while let Some(event) = event_rx.recv().await {
            match event {
                BuildEvent::BuildStarted => {
                    pending_diagnostics.clear();
                    session.write().on_build_started();

                    {
                        let mut c = chat.write();
                        if build_turn_active {
                            c.finish_turn(StepStatus::Complete);
                        }
                        let idx = c.begin_turn("dx serve → building");
                        c.workflow_turns[idx].push_step(WorkflowStep::build("compiling..."));
                        build_turn_active = true;
                    }
                }

                BuildEvent::BuildSuccess => {
                    session.write().on_build_success();

                    {
                        let mut c = chat.write();
                        c.complete_last_step();
                        c.push_step(WorkflowStep {
                            kind: StepKind::Build,
                            summary: "ready ✓".to_string(),
                            status: StepStatus::Complete,
                        });
                        c.finish_turn(StepStatus::Complete);
                        build_turn_active = false;
                    }

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

                    {
                        let mut c = chat.write();
                        c.complete_last_step();
                        c.push_step(WorkflowStep {
                            kind: StepKind::Build,
                            summary: format!("{} error{}", count, if count == 1 { "" } else { "s" }),
                            status: StepStatus::Failed,
                        });
                        c.finish_turn(StepStatus::Failed);
                        build_turn_active = false;
                    }

                    if count > 0 {
                        had_startup_failure = false;
                        session.write().on_build_failed(diags.clone());
                        chat.write().push(ChatMessage::auto_fix(diags.clone()));
                        spawn_countdown_loop(
                            session,
                            chat.clone(),
                            config.clone(),
                            project_path.clone(),
                            diags,
                        );
                    }
                }

                BuildEvent::ProcessExited { code } => {
                    session.write().serve_running = false;
                    if build_turn_active {
                        chat.write().finish_turn(StepStatus::Failed);
                        build_turn_active = false;
                    }

                    // Any non-zero exit with log output → feed raw log to agent.
                    // No pattern matching needed: the LLM reads the output and decides what to fix.
                    let failed = code.map(|c| c != 0).unwrap_or(true);
                    let raw_log = session.read().raw_log.clone();

                    if failed && (had_startup_failure || !raw_log.is_empty()) {
                        let log_text = raw_log
                            .iter()
                            .map(|l| strip_ansi_for_prompt(l))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let fix_prompt = format!(
                            "`dx serve` failed to start. Here is the full output:\n\n```\n{}\n```\n\n\
                            Read the project files and fix the problem.",
                            log_text
                        );
                        chat.write().push(ChatMessage::system(
                            "dx serve failed — asking Rocky to fix it...".to_string(),
                        ));
                        spawn_fatal_fix_turn(
                            session,
                            chat.clone(),
                            config.clone(),
                            project_path.clone(),
                            fix_prompt,
                        );
                    } else {
                        chat.write().push(ChatMessage::system(format!(
                            "dx serve exited (code: {:?})",
                            code
                        )));
                    }
                    break;
                }

                BuildEvent::StdoutLine(line) => {
                    tracing::trace!("dx serve: {}", line);
                    session.write().push_log(line);
                }

                BuildEvent::Progress { stage } => {
                    session.write().on_progress(stage.clone());
                    tracing::debug!("Build progress: {:?}", stage);
                }

                BuildEvent::CompilingCrate { krate } => {
                    session.write().on_compiling_crate(krate.clone());
                    tracing::debug!("Compiling crate: {}", krate);
                }

                BuildEvent::FatalError(msg) => {
                    had_startup_failure = true;
                    chat.write().push(ChatMessage::fatal_error(&msg));
                    tracing::error!("Fatal error from dx serve: {}", msg);
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
                spawn_agent_turn(session, chat, config, project_path, diagnostics).await;
                return;
            }

            let state = session.read().auto_fix.clone();
            match state {
                crate::state::session::AutoFixState::Stopped => return,
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

    let iteration = {
        chat.read()
            .workflow_turns
            .iter()
            .filter(|t| t.title.starts_with("auto-fix"))
            .count() as u8
            + 1
    };

    chat.write()
        .begin_turn(format!("auto-fix {}/5", iteration));

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

    // Inject full source of error-bearing files so the LLM has context
    let diag_files: Vec<String> = diagnostics
        .iter()
        .filter(|d| !d.file.is_empty())
        .map(|d| d.file.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let file_contents = read_files_parallel(&project_path, &diag_files).await;
    for (path, content) in file_contents {
        context.push_file_context(&path, content);
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

            AgentEvent::ActionStarted(name, summary) => {
                let kind = match name.as_str() {
                    "read_file" | "read_files" => StepKind::ReadFile,
                    "write_file" | "write_files" => StepKind::WriteFile,
                    "list_files" => StepKind::ListFiles,
                    _ => StepKind::Build,
                };
                chat.write().push_step(WorkflowStep::new(kind, summary));
            }

            AgentEvent::ToolCalled(tc) => {
                chat.write().complete_last_step();
                tool_calls.push(tc);
            }

            AgentEvent::FileWritten(path) => {
                chat.write().last_touched_files.push(path.clone());
            }

            AgentEvent::TurnComplete => {
                if !assistant_text.is_empty() || !tool_calls.is_empty() {
                    chat.write().push(ChatMessage::assistant(
                        assistant_text.clone(),
                        tool_calls.clone(),
                    ));
                }
                chat.write().finish_turn(StepStatus::Complete);
                break;
            }

            AgentEvent::Error(e) => {
                chat.write().push(ChatMessage::system(format!("Agent error: {}", e)));
                chat.write().finish_turn(StepStatus::Failed);
                break;
            }
        }
    }

    chat.write().agent_running = false;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn strip_ansi_for_prompt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for cc in chars.by_ref() {
                if cc == 'm' { break; }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Fatal error fix turn ──────────────────────────────────────────────────────


fn spawn_fatal_fix_turn(
    mut session: Signal<SessionState>,
    mut chat: Signal<ChatState>,
    config: ProviderConfig,
    project_path: PathBuf,
    prompt: String,
) {
    spawn(async move {
        chat.write().agent_running = true;
        chat.write().begin_turn("fix startup error");

        let client = ModelClient::new(&config);
        let mut context = ConversationContext::default();
        context.push_user(&prompt);

        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel(64);
        let project_path_clone = project_path.clone();
        spawn(async move {
            if let Err(e) = run_turn(&client, &mut context, project_path_clone, agent_tx).await {
                tracing::error!("Fatal fix turn failed: {}", e);
            }
        });

        let mut assistant_text = String::new();
        let mut tool_calls = vec![];
        let mut files_written = false;

        while let Some(event) = agent_rx.recv().await {
            match event {
                AgentEvent::AssistantText(t) => assistant_text.push_str(&t),

                AgentEvent::ActionStarted(name, summary) => {
                    let kind = match name.as_str() {
                        "read_file" => StepKind::ReadFile,
                        "write_file" => StepKind::WriteFile,
                        "list_files" => StepKind::ListFiles,
                        _ => StepKind::Build,
                    };
                    chat.write().push_step(WorkflowStep::new(kind, summary));
                }

                AgentEvent::ToolCalled(tc) => {
                    chat.write().complete_last_step();
                    tool_calls.push(tc);
                }

                AgentEvent::FileWritten(path) => {
                    chat.write().last_touched_files.push(path);
                    files_written = true;
                }

                AgentEvent::TurnComplete => {
                    if !assistant_text.is_empty() || !tool_calls.is_empty() {
                        chat.write().push(ChatMessage::assistant(
                            assistant_text.clone(),
                            tool_calls.clone(),
                        ));
                    }
                    chat.write().finish_turn(StepStatus::Complete);
                    break;
                }

                AgentEvent::Error(e) => {
                    chat.write().push(ChatMessage::system(format!("Agent error: {}", e)));
                    chat.write().finish_turn(StepStatus::Failed);
                    files_written = false; // don't retry on agent error
                    break;
                }
            }
        }

        chat.write().agent_running = false;

        // If the agent wrote files, restart dx serve to verify the fix.
        // The new run will trigger another fatal-fix turn if it fails again.
        // Loops indefinitely until the build succeeds.
        if files_written {
            let iterations = session.read().fatal_fix_iterations;
            session.write().fatal_fix_iterations += 1;
            chat.write().push(ChatMessage::system(format!(
                "Restarting dx serve (attempt {})...",
                iterations + 1,
            )));
            // Brief pause so file writes settle before cargo reads them
            tokio::time::sleep(Duration::from_millis(500)).await;
            start_project(project_path, session, chat, config, false).await;
        }
    });
}
