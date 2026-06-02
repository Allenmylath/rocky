use crate::state::session::{AutoFixState, BuildStatus, SessionState};
use dioxus::prelude::*;

/// Shows the auto-fix countdown banner when a build fails
#[component]
pub fn AutoFixBanner() -> Element {
    let mut session = use_context::<Signal<SessionState>>();

    let auto_fix = session.read().auto_fix.clone();

    match auto_fix {
        AutoFixState::Countdown(n) => rsx! {
            div {
                style: "
                    background: #7c2d12;
                    color: #fed7aa;
                    padding: 8px 16px;
                    font-size: 13px;
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                ",
                span {
                    "⚡ Auto-fixing in {n}s..."
                }
                button {
                    style: "
                        background: transparent;
                        border: 1px solid #fed7aa;
                        color: #fed7aa;
                        padding: 4px 10px;
                        border-radius: 4px;
                        font-family: monospace;
                        font-size: 12px;
                        cursor: pointer;
                    ",
                    onclick: move |_| session.write().stop_auto_fix(),
                    "Stop"
                }
            }
        },
        AutoFixState::Stopped => rsx! {
            div {
                style: "background: #1c1917; color: #78716c; padding: 8px 16px; font-size: 12px;",
                "Auto-fix stopped. Fix errors manually or send a message."
            }
        },
        AutoFixState::Idle => rsx! { div {} },
    }
}

/// Shows current diagnostics from the last failed build
#[component]
pub fn DiagnosticsPanel() -> Element {
    let session = use_context::<Signal<SessionState>>();

    let build_status = session.read().build_status.clone();

    rsx! {
        div {
            style: "padding: 12px; height: 100%;",

            div {
                style: "font-size: 12px; color: #666; margin-bottom: 12px; text-transform: uppercase; letter-spacing: 0.05em;",
                "Build Output"
            }

            match build_status {
                BuildStatus::Idle => rsx! {
                    div {
                        style: "color: #555; font-size: 13px;",
                        "Waiting for build..."
                    }
                },
                BuildStatus::Building => rsx! {
                    div {
                        style: "color: #f59e0b; font-size: 13px;",
                        "⟳ Building..."
                    }
                },
                BuildStatus::Success => rsx! {
                    div {
                        style: "color: #22c55e; font-size: 13px;",
                        "✓ Build succeeded"
                    }
                },
                BuildStatus::Failed(diags) => rsx! {
                    div {
                        style: "display: flex; flex-direction: column; gap: 12px;",
                        for diag in diags.iter() {
                            div {
                                style: "
                                    background: #1c0a09;
                                    border: 1px solid #7f1d1d;
                                    border-radius: 6px;
                                    padding: 10px 12px;
                                    font-family: monospace;
                                ",
                                // Error header
                                div {
                                    style: "color: #ef4444; font-size: 12px; font-weight: bold; margin-bottom: 4px;",
                                    if let Some(code) = &diag.code {
                                        "error[{code}]"
                                    } else {
                                        "error"
                                    }
                                }
                                // Message
                                div {
                                    style: "color: #fca5a5; font-size: 12px; margin-bottom: 6px;",
                                    "{diag.message}"
                                }
                                // Location
                                div {
                                    style: "color: #6b7280; font-size: 11px; margin-bottom: 6px;",
                                    { format!("→ {}:{}", diag.file, diag.line) }
                                }
                                // Snippet
                                if !diag.snippet.is_empty() {
                                    div {
                                        style: "
                                            background: #0f0f0f;
                                            border-radius: 4px;
                                            padding: 6px 8px;
                                            font-size: 11px;
                                            color: #9ca3af;
                                            overflow-x: auto;
                                            white-space: pre;
                                        ",
                                        for line in diag.snippet.iter() {
                                            div { "{line}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}