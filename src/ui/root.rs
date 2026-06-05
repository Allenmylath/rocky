use crate::agent::client::ProviderConfig;
use crate::state::chat::ChatState;
use crate::state::session::SessionState;
use crate::ui::chat::ChatPanel;
use crate::ui::diagnostics::BuildLogPanel;
use crate::ui::template_picker::TemplatePicker;
use crate::ui::workflow::WorkflowPanel;
use dioxus::prelude::*;

#[component]
pub fn Root() -> Element {
    let mut session = use_context::<Signal<SessionState>>();
    let chat = use_context::<Signal<ChatState>>();
    let config_signal = use_context::<Signal<ProviderConfig>>();

    let on_restart = move |_| {
        let path = session.read().project_path.clone();
        if let Some(project_path) = path {
            let config = config_signal.read().clone();
            spawn(async move {
                crate::app::start_project(project_path, session, chat, config, false).await;
            });
        }
    };

    let on_leave_sample = move |_| {
        session.write().project_path = None;
        session.write().is_sample_project = false;
        session.write().serve_running = false;
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: monospace; background: #111; color: #ccc;",

            div {
                style: "
                    padding: 8px 16px;
                    background: #1a1a1a;
                    color: #ccc;
                    font-size: 13px;
                    display: flex;
                    align-items: center;
                    gap: 12px;
                    border-bottom: 1px solid #2a2a2a;
                ",
                span { style: "color: #f97316; font-weight: bold;", "🪨 Rocky" }
                if let Some(name) = session.read().project_name() {
                    span { style: "color: #555;", "›" }
                    span { style: "color: #888;", "{name}" }
                }
                if session.read().project_path.is_some() && !session.read().serve_running {
                    div { style: "margin-left: auto;",
                        button {
                            style: "
                                background: #1a2a1a;
                                border: 1px solid #2a4a2a;
                                color: #4ade80;
                                padding: 3px 10px;
                                border-radius: 4px;
                                font-family: monospace;
                                font-size: 12px;
                                cursor: pointer;
                            ",
                            onclick: on_restart,
                            "▶ Start dx serve"
                        }
                    }
                }
            }

            if session.read().is_sample_project {
                div {
                    style: "
                        padding: 6px 16px;
                        background: #1a2a1a;
                        color: #4ade80;
                        font-size: 12px;
                        display: flex;
                        align-items: center;
                        justify-content: space-between;
                        border-bottom: 1px solid #2a4a2a;
                    ",
                    span { "🎓 Sample project — verify the build works, then open your own project." }
                    button {
                        style: "
                            background: transparent;
                            border: 1px solid #4ade80;
                            color: #4ade80;
                            padding: 2px 10px;
                            border-radius: 4px;
                            font-family: monospace;
                            font-size: 11px;
                            cursor: pointer;
                        ",
                        onclick: on_leave_sample,
                        "Open real project →"
                    }
                }
            }

            if session.read().project_path.is_none() {
                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center;",
                    TemplatePicker {}
                }
            } else {
                div {
                    style: "flex: 1; display: flex; overflow: hidden;",

                    div {
                        style: "flex: 1; display: flex; flex-direction: column; border-right: 1px solid #222; min-width: 0;",
                        ChatPanel {}
                    }

                    div {
                        style: "width: 360px; flex-shrink: 0; display: flex; flex-direction: column; overflow: hidden; background: #0f0f0f;",
                        div {
                            style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",
                            WorkflowPanel {}
                        }
                        BuildLogPanel {}
                    }
                }
            }
        }
    }
}
