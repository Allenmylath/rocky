use crate::app::{start_project, ApiKey};
use crate::state::chat::ChatState;
use crate::state::session::SessionState;
use dioxus::prelude::*;

#[component]
pub fn ProjectPicker() -> Element {
    let session = use_context::<Signal<SessionState>>();
    let chat = use_context::<Signal<ChatState>>();
    let api_key_signal = use_context::<Signal<ApiKey>>();

    let mut picking = use_signal(|| false);

    let on_pick = move |_| {
        picking.set(true);
        let session = session.clone();
        let chat = chat.clone();
        let api_key = api_key_signal.read().0.clone();

        spawn(async move {
            // Open native folder picker
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Select your Dioxus project folder")
                .pick_folder()
                .await;

            if let Some(folder) = folder {
                let path = folder.path().to_path_buf();
                start_project(path, session, chat, api_key).await;
            }

            picking.set(false);
        });
    };

    rsx! {
        div {
            style: "text-align: center;",

            div {
                style: "font-size: 32px; margin-bottom: 16px;",
                "🪨"
            }
            div {
                style: "font-size: 20px; font-weight: bold; color: #f97316; margin-bottom: 8px;",
                "Rocky"
            }
            div {
                style: "color: #888; font-size: 14px; margin-bottom: 32px;",
                "AI-powered Dioxus development loop"
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
                ",
                disabled: *picking.read(),
                onclick: on_pick,
                if *picking.read() { "Opening..." } else { "Open Dioxus Project" }
            }

            if api_key_signal.read().0.is_empty() {
                div {
                    style: "margin-top: 16px; color: #ef4444; font-size: 12px;",
                    "⚠ ANTHROPIC_API_KEY not set"
                }
            }
        }
    }
}