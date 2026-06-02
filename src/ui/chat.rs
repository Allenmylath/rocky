use crate::agent::client::{ModelClient, ProviderConfig};
use crate::agent::context::ConversationContext;
use crate::agent::loop_runner::{run_turn, AgentEvent};
use crate::state::chat::{ChatMessage, ChatState, MessageRole};
use crate::state::session::SessionState;
use dioxus::prelude::*;
use std::path::PathBuf;

#[component]
pub fn ChatPanel() -> Element {
    let session = use_context::<Signal<SessionState>>();
    let mut chat = use_context::<Signal<ChatState>>();
    let config_signal = use_context::<Signal<ProviderConfig>>();
    let mut input = use_signal(|| String::new());

    let mut on_send = move |_| {
        let text = input.read().trim().to_string();
        if text.is_empty() || chat.read().agent_running {
            return;
        }

        input.set(String::new());
        chat.write().push(ChatMessage::user(text.clone()));

        let config = config_signal.read().clone();
        let project_path = session
            .read()
            .project_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        let mut chat_clone = chat.clone();

        spawn(async move {
            chat_clone.write().agent_running = true;

            let client = ModelClient::new(&config);
            let mut context = ConversationContext::default();

            // Replay history into context
            for msg in chat_clone.read().to_api_messages() {
                match msg.role.as_str() {
                    "user" => context.push_user(msg.content),
                    "assistant" => context.push_assistant_text(msg.content),
                    _ => {}
                }
            }

            let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel(64);

            let agent_task = tokio::spawn(async move {
                let _ = run_turn(&client, &mut context, project_path, agent_tx).await;
            });

            let mut assistant_text = String::new();
            let mut tool_calls = vec![];

            while let Some(event) = agent_rx.recv().await {
                match event {
                    AgentEvent::AssistantText(t) => assistant_text.push_str(&t),
                    AgentEvent::ToolCalled(tc) => tool_calls.push(tc),
                    AgentEvent::FileWritten(path) => {
                        chat_clone.write().last_touched_files.push(path);
                    }
                    AgentEvent::TurnComplete => {
                        if !assistant_text.is_empty() || !tool_calls.is_empty() {
                            chat_clone.write().push(ChatMessage::assistant(
                                assistant_text.clone(),
                                tool_calls.clone(),
                            ));
                        }
                        break;
                    }
                    AgentEvent::Error(e) => {
                        chat_clone
                            .write()
                            .push(ChatMessage::system(format!("Error: {}", e)));
                        break;
                    }
                }
            }

            let _ = agent_task.await;
            chat_clone.write().agent_running = false;
        });
    };

    let on_keydown = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter && !evt.modifiers().shift() {
            on_send(());
        }
    };

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

            // Message list
            div {
                style: "flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 12px;",
                for msg in chat.read().messages.iter() {
                    MessageBubble { message: msg.clone() }
                }
                if chat.read().agent_running {
                    div {
                        style: "color: #888; font-size: 12px; font-style: italic;",
                        "Agent is working..."
                    }
                }
            }

            // Input area
            div {
                style: "padding: 12px; border-top: 1px solid #333; display: flex; gap: 8px;",
                textarea {
                    style: "
                        flex: 1;
                        background: #1a1a1a;
                        border: 1px solid #333;
                        border-radius: 6px;
                        color: #eee;
                        padding: 8px 12px;
                        font-family: monospace;
                        font-size: 13px;
                        resize: none;
                        outline: none;
                    ",
                    rows: "3",
                    placeholder: "Describe what you want to build... (Enter to send)",
                    value: "{input}",
                    oninput: move |evt| input.set(evt.value()),
                    onkeydown: on_keydown,
                }
                button {
                    style: "
                        background: #f97316;
                        color: white;
                        border: none;
                        padding: 8px 16px;
                        border-radius: 6px;
                        font-family: monospace;
                        font-size: 13px;
                        cursor: pointer;
                        align-self: flex-end;
                    ",
                    disabled: chat.read().agent_running,
                    onclick: move |_| on_send(()),
                    "Send"
                }
            }
        }
    }
}

#[component]
fn MessageBubble(message: ChatMessage) -> Element {
    let (bg, color, label) = match message.role {
        MessageRole::User => ("#1e3a5f", "#93c5fd", "you"),
        MessageRole::Assistant => ("#1a1a1a", "#e5e7eb", "rocky"),
        MessageRole::System => ("#1a1a1a", "#6b7280", "system"),
    };

    rsx! {
        div {
            style: "background: {bg}; border-radius: 6px; padding: 10px 14px;",
            div {
                style: "font-size: 11px; color: #666; margin-bottom: 4px;",
                "{label}"
            }
            div {
                style: "color: {color}; font-size: 13px; white-space: pre-wrap; word-break: break-word;",
                "{message.content}"
            }
            // Tool calls
            if !message.tool_calls.is_empty() {
                div {
                    style: "margin-top: 8px; display: flex; flex-direction: column; gap: 4px;",
                    for tc in message.tool_calls.iter() {
                        div {
                            style: "font-size: 11px; color: #666; font-family: monospace;",
                            "⚙ {tc.name}({tc.summary})"
                        }
                    }
                }
            }
        }
    }
}