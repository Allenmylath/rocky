use crate::agent::client::{Provider, ProviderConfig};
use crate::app::start_project;
use crate::state::chat::ChatState;
use crate::state::session::{SessionState, WarmState};
use crate::templates::warmstart::take_warm_project;
use crate::templates::TemplateKind;
use dioxus::desktop::use_window;
use dioxus::prelude::*;

#[component]
fn TemplateCard(kind: TemplateKind, selected: TemplateKind, on_click: EventHandler<()>) -> Element {
    let is_sel = selected == kind;
    let border = if is_sel { "2px solid #f97316" } else { "2px solid #2a2a2a" };
    let bg = if is_sel { "#1a1a1a" } else { "#111" };

    rsx! {
        div {
            style: "
                width: 200px; height: 140px;
                background: {bg};
                border: {border};
                border-radius: 8px;
                padding: 16px;
                cursor: pointer;
                display: flex;
                flex-direction: column;
                align-items: center;
                justify-content: center;
                transition: border 0.15s;
            ",
            onclick: move |_| on_click.call(()),

            div { style: "font-size: 28px; margin-bottom: 8px;", "{kind.icon()}" }
            div {
                style: "font-weight: 600; font-size: 14px; margin-bottom: 4px; color: #eee;",
                "{kind.display_name()}"
            }
            div {
                style: "font-size: 11px; color: #888; text-align: center; line-height: 1.4;",
                "{kind.description()}"
            }
        }
    }
}

#[component]
pub fn TemplatePicker() -> Element {
    let session = use_context::<Signal<SessionState>>();
    let chat = use_context::<Signal<ChatState>>();
    let config = use_context::<Signal<ProviderConfig>>();
    let mut picking = use_signal(|| false);
    let mut selected = use_signal(|| TemplateKind::Blank);
    let mut description = use_signal(|| String::new());
    let win = use_window();

    let templates = vec![
        TemplateKind::Store,
        TemplateKind::Dashboard,
        TemplateKind::LandingPage,
        TemplateKind::Blank,
    ];

    let on_build = move |_| {
        let kind = *selected.read();
        let desc = description.read().clone();
        if desc.trim().is_empty() && kind == TemplateKind::Blank {
            return;
        }
        let mut session = session.clone();
        let mut chat = chat.clone();
        let cfg = config.read().clone();
        picking.set(true);

        spawn(async move {
            // Attempt to take the warm project
            if let Some(path) = take_warm_project(session, kind, desc.clone()).await {
                session.write().template_kind = Some(kind);
                session.write().initial_prompt = Some(desc);
                start_project(path, session, chat, cfg, false).await;
            } else {
                chat.write().push(crate::state::chat::ChatMessage::system(
                    "Warm project not ready yet — please wait a moment and try again."
                        .to_string(),
                ));
            }
            picking.set(false);
        });
    };

    let on_open_existing = move |_| {
        picking.set(true);
        let session = session.clone();
        let chat = chat.clone();
        let cfg = config.read().clone();
        let win = win.clone();

        spawn(async move {
            win.set_minimized(true);
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Select your Dioxus project folder")
                .pick_folder()
                .await;
            win.set_minimized(false);
            win.set_focus();

            if let Some(folder) = folder {
                let path = crate::config::find_project_root(&folder.path().to_path_buf());
                start_project(path, session, chat, cfg, false).await;
            }
            picking.set(false);
        });
    };

    let is_anthropic = config.read().provider == Provider::Anthropic;
    let active_key = config.read().active_key().to_string();
    let key_missing = active_key.is_empty();
    let warn_msg = if is_anthropic {
        "⚠ ANTHROPIC_API_KEY not set"
    } else {
        "⚠ OPENAI_API_KEY not set"
    };

    let warm_ready = matches!(session.read().warm_state, WarmState::Ready { .. });

    rsx! {
        div {
            style: "display: flex; height: 100vh; width: 100vw; font-family: monospace; background: #111; color: #ccc;",

            // Left panel — template cards
            div {
                style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 40px;",

                div { style: "font-size: 32px; margin-bottom: 8px;", "🪨" }
                div {
                    style: "font-size: 20px; font-weight: bold; color: #f97316; margin-bottom: 4px;",
                    "Rocky"
                }
                div {
                    style: "color: #888; font-size: 14px; margin-bottom: 32px;",
                    "Pick a starter template"
                }

                div {
                    style: "display: grid; grid-template-columns: repeat(2, 200px); gap: 16px;",

                    for t in templates {
                        TemplateCard {
                            kind: t,
                            selected: *selected.read(),
                            on_click: move |_| selected.set(t),
                        }
                    }
                }
            }

            // Right panel — prompt + actions
            div {
                style: "width: 420px; background: #0a0a0a; border-left: 1px solid #222; padding: 40px; display: flex; flex-direction: column; justify-content: center;",

                div {
                    style: "margin-bottom: 12px; color: #aaa; font-size: 12px;",
                    "What do you want to build?"
                }

                textarea {
                    style: "
                        width: 100%;
                        height: 120px;
                        padding: 12px;
                        background: #1a1a1a;
                        border: 1px solid #333;
                        border-radius: 6px;
                        color: #ccc;
                        font-family: monospace;
                        font-size: 13px;
                        resize: none;
                        outline: none;
                        box-sizing: border-box;
                        margin-bottom: 16px;
                    ",
                    placeholder: "Describe what you want to build...",
                    value: "{description.read()}",
                    oninput: move |e| description.set(e.value()),
                }

                {
                    let is_picking = *picking.read();
                    let opacity = if is_picking { "0.6" } else { "1" };
                    let label = if is_picking {
                        if warm_ready { "Building..." } else { "Warming up..." }
                    } else {
                        "Build it"
                    };
                    rsx! {
                        button {
                            style: "
                                width: 100%;
                                padding: 12px;
                                background: #f97316;
                                color: white;
                                border: none;
                                border-radius: 6px;
                                font-weight: 700;
                                font-size: 14px;
                                cursor: pointer;
                                margin-bottom: 12px;
                                opacity: {opacity};
                            ",
                            disabled: is_picking,
                            onclick: on_build,
                            "{label}"
                        }
                    }
                }

                div {
                    style: "text-align: center;",
                    button {
                        style: "
                            background: transparent;
                            border: none;
                            color: #888;
                            font-size: 13px;
                            cursor: pointer;
                            font-family: monospace;
                        ",
                        onclick: on_open_existing,
                        "Open existing project →"
                    }
                }

                if key_missing {
                    div {
                        style: "margin-top: 16px; color: #ef4444; font-size: 12px; text-align: center;",
                        "{warn_msg}"
                    }
                }
            }
        }
    }
}
