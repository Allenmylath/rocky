use crate::state::session::SessionState;
use crate::ui::chat::ChatPanel;
use crate::ui::diagnostics::{AutoFixBanner, BuildLogPanel, DiagnosticsPanel};
use crate::ui::project_picker::ProjectPicker;
use dioxus::prelude::*;

#[component]
pub fn Root() -> Element {
    let session = use_context::<Signal<SessionState>>();

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: monospace;",

            // Top bar
            div {
                style: "padding: 8px 16px; background: #1a1a1a; color: #ccc; font-size: 13px; display: flex; align-items: center; gap: 12px;",
                span { style: "color: #f97316; font-weight: bold;", "🪨 Rocky" }
                if let Some(name) = session.read().project_name() {
                    span { style: "color: #888;", "→ {name}" }
                }
                // Build status indicator
                {build_status_badge(&session.read().build_status)}
            }

            // Main content
            if session.read().project_path.is_none() {
                // No project — show picker
                div {
                    style: "flex: 1; display: flex; align-items: center; justify-content: center;",
                    ProjectPicker {}
                }
            } else {
                // Project loaded — show main layout
                div {
                    style: "flex: 1; display: flex; overflow: hidden;",

                    // Left: chat
                    div {
                        style: "flex: 1; display: flex; flex-direction: column; border-right: 1px solid #333;",
                        AutoFixBanner {}
                        ChatPanel {}
                    }

                    // Right: diagnostics + raw log
                    div {
                        style: "width: 380px; overflow-y: auto; display: flex; flex-direction: column;",
                        DiagnosticsPanel {}
                        BuildLogPanel {}
                    }
                }
            }
        }
    }
}

fn build_status_badge(status: &crate::state::session::BuildStatus) -> Element {
    use crate::state::session::BuildStatus;
    let (label, color) = match status {
        BuildStatus::Idle => ("idle", "#666"),
        BuildStatus::Building => ("building...", "#f59e0b"),
        BuildStatus::Success => ("✓ ready", "#22c55e"),
        BuildStatus::Failed(diags) => {
            let _ = diags; // used via label below
            ("✗ failed", "#ef4444")
        }
    };
    let label = if let BuildStatus::Failed(d) = status {
        format!("✗ {} error{}", d.len(), if d.len() == 1 { "" } else { "s" })
    } else {
        label.to_string()
    };

    rsx! {
        span {
            style: "color: {color}; font-size: 12px;",
            "{label}"
        }
    }
}