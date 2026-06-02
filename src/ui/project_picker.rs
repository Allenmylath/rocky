use crate::agent::client::{Provider, ProviderConfig};
use crate::app::start_project;
use crate::state::chat::ChatState;
use crate::state::session::SessionState;
use dioxus::prelude::*;

#[component]
pub fn ProjectPicker() -> Element {
    let session = use_context::<Signal<SessionState>>();
    let chat = use_context::<Signal<ChatState>>();
    let mut config = use_context::<Signal<ProviderConfig>>();
    let mut picking = use_signal(|| false);

    let on_pick = move |_| {
        picking.set(true);
        let session = session.clone();
        let chat = chat.clone();
        let cfg = config.read().clone();

        spawn(async move {
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Select your Dioxus project folder")
                .pick_folder()
                .await;

            if let Some(folder) = folder {
                let path = folder.path().to_path_buf();
                start_project(path, session, chat, cfg).await;
            }

            picking.set(false);
        });
    };

    let is_anthropic = config.read().provider == Provider::Anthropic;
    let active_key = config.read().active_key().to_string();
    let key_missing = active_key.is_empty();

    // Pre-compute button styles to avoid nested quotes inside RSX strings
    let (anthro_bg, anthro_border, anthro_color) = if is_anthropic {
        ("#f97316", "#f97316", "white")
    } else {
        ("#1a1a1a", "#444", "#888")
    };
    let (openai_bg, openai_border, openai_color) = if !is_anthropic {
        ("#f97316", "#f97316", "white")
    } else {
        ("#1a1a1a", "#444", "#888")
    };
    let key_placeholder = if is_anthropic { "sk-ant-..." } else { "sk-..." };
    let warn_msg = if is_anthropic {
        "⚠ ANTHROPIC_API_KEY not set"
    } else {
        "⚠ OPENAI_API_KEY not set"
    };

    rsx! {
        div {
            style: "text-align: center; width: 300px;",

            div { style: "font-size: 32px; margin-bottom: 16px;", "🪨" }
            div {
                style: "font-size: 20px; font-weight: bold; color: #f97316; margin-bottom: 8px;",
                "Rocky"
            }
            div {
                style: "color: #888; font-size: 14px; margin-bottom: 28px;",
                "AI-powered Dioxus development loop"
            }

            // Provider toggle
            div { style: "margin-bottom: 14px; text-align: left;",
                div {
                    style: "color: #aaa; font-size: 12px; margin-bottom: 6px;",
                    "Provider"
                }
                div { style: "display: flex;",
                    button {
                        style: "
                            flex: 1;
                            padding: 8px 0;
                            border: 1px solid {anthro_border};
                            border-radius: 4px 0 0 4px;
                            background: {anthro_bg};
                            color: {anthro_color};
                            font-family: monospace;
                            font-size: 13px;
                            cursor: pointer;
                        ",
                        onclick: move |_| config.write().provider = Provider::Anthropic,
                        "Anthropic"
                    }
                    button {
                        style: "
                            flex: 1;
                            padding: 8px 0;
                            border: 1px solid {openai_border};
                            border-left: none;
                            border-radius: 0 4px 4px 0;
                            background: {openai_bg};
                            color: {openai_color};
                            font-family: monospace;
                            font-size: 13px;
                            cursor: pointer;
                        ",
                        onclick: move |_| config.write().provider = Provider::OpenAi,
                        "OpenAI"
                    }
                }
            }

            // API key input
            div { style: "margin-bottom: 24px; text-align: left;",
                div {
                    style: "color: #aaa; font-size: 12px; margin-bottom: 6px;",
                    "API Key"
                }
                input {
                    r#type: "password",
                    style: "
                        width: 100%;
                        padding: 8px 10px;
                        background: #1a1a1a;
                        border: 1px solid #444;
                        border-radius: 4px;
                        color: #ccc;
                        font-family: monospace;
                        font-size: 13px;
                        box-sizing: border-box;
                        outline: none;
                    ",
                    placeholder: "{key_placeholder}",
                    value: "{active_key}",
                    oninput: move |e| config.write().set_active_key(e.value()),
                }
            }

            button {
                style: "
                    background: #f97316;
                    color: white;
                    border: none;
                    padding: 12px 24px;
                    border-radius: 6px;
                    font-size: 14px;
                    font-family: monospace;
                    cursor: pointer;
                    width: 100%;
                ",
                disabled: *picking.read(),
                onclick: on_pick,
                if *picking.read() { "Opening..." } else { "Open Dioxus Project" }
            }

            if key_missing {
                div {
                    style: "margin-top: 12px; color: #ef4444; font-size: 12px;",
                    "{warn_msg}"
                }
            }
        }
    }
}
