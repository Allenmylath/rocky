use crate::state::chat::{ChatState, StepStatus, WorkflowTurn};
use crate::state::session::{AutoFixState, BuildStatus, SessionState};
use dioxus::prelude::*;

#[component]
pub fn WorkflowPanel() -> Element {
    let session = use_context::<Signal<SessionState>>();
    let mut chat = use_context::<Signal<ChatState>>();

    let turns = chat.read().workflow_turns.clone();
    let build_status = session.read().build_status.clone();
    let auto_fix = session.read().auto_fix.clone();

    rsx! {
        div {
            style: "
                display: flex;
                flex-direction: column;
                height: 100%;
                overflow: hidden;
            ",

            div {
                style: "
                    padding: 10px 14px 8px;
                    font-size: 11px;
                    color: #555;
                    text-transform: uppercase;
                    letter-spacing: 0.08em;
                    border-bottom: 1px solid #1f1f1f;
                    flex-shrink: 0;
                ",
                "Workflow"
            }

            BuildStatusBar { status: build_status.clone() }

            { auto_fix_inline(&auto_fix, session) }

            div {
                style: "
                    flex: 1;
                    overflow-y: auto;
                    padding: 8px 0;
                ",

                if turns.is_empty() {
                    div {
                        style: "
                            padding: 20px 14px;
                            color: #444;
                            font-size: 12px;
                            text-align: center;
                        ",
                        "No activity yet."
                        br {}
                        span {
                            style: "color: #333; font-size: 11px;",
                            "Send a message to start."
                        }
                    }
                } else {
                    for turn in turns.iter() {
                        TurnGroup {
                            turn: turn.clone(),
                            on_toggle: {
                                let turn_id = turn.id;
                                move |_| chat.write().toggle_turn(turn_id)
                            }
                        }
                    }
                }
            }

            { diagnostics_section(&build_status) }
        }
    }
}

#[component]
fn BuildStatusBar(status: BuildStatus) -> Element {
    let (label, bg, color) = match &status {
        BuildStatus::Idle => ("○ idle", "#111", "#444"),
        BuildStatus::Building => ("⏳ building...", "#1a1200", "#f59e0b"),
        BuildStatus::Success => ("✓ ready", "#0a1f0a", "#22c55e"),
        BuildStatus::Failed(d) => {
            let _ = d;
            ("✗ failed", "#1a0a0a", "#ef4444")
        }
    };

    let label = if let BuildStatus::Failed(d) = &status {
        format!("✗ {} error{}", d.len(), if d.len() == 1 { "" } else { "s" })
    } else {
        label.to_string()
    };

    rsx! {
        div {
            style: "
                padding: 6px 14px;
                background: {bg};
                color: {color};
                font-size: 12px;
                font-family: monospace;
                border-bottom: 1px solid #1f1f1f;
                flex-shrink: 0;
            ",
            "{label}"
        }
    }
}

fn auto_fix_inline(auto_fix: &AutoFixState, mut session: Signal<SessionState>) -> Element {
    match auto_fix {
        AutoFixState::Countdown(n) => rsx! {
            div {
                style: "
                    padding: 6px 14px;
                    background: #1c0f00;
                    border-bottom: 1px solid #3d1f00;
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    flex-shrink: 0;
                ",
                span {
                    style: "color: #f97316; font-size: 12px;",
                    "⚡ auto-fix in {n}s"
                }
                button {
                    style: "
                        background: transparent;
                        border: 1px solid #3d1f00;
                        color: #666;
                        padding: 2px 8px;
                        border-radius: 3px;
                        font-size: 11px;
                        cursor: pointer;
                    ",
                    onclick: move |_| session.write().stop_auto_fix(),
                    "stop"
                }
            }
        },
        AutoFixState::Stopped => rsx! {
            div {
                style: "
                    padding: 5px 14px;
                    color: #444;
                    font-size: 11px;
                    border-bottom: 1px solid #1f1f1f;
                    flex-shrink: 0;
                ",
                "auto-fix stopped"
            }
        },
        AutoFixState::Idle => rsx! { div {} },
    }
}

#[component]
fn TurnGroup(turn: WorkflowTurn, on_toggle: EventHandler<()>) -> Element {
    let icon = turn.status.icon();
    let color = turn.status.color();
    let expanded = turn.expanded;
    let chevron = if expanded { "▾" } else { "▸" };

    rsx! {
        div {
            style: "border-bottom: 1px solid #1a1a1a;",

            div {
                style: "
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    padding: 7px 14px;
                    cursor: pointer;
                    user-select: none;
                ",
                onclick: move |_| on_toggle.call(()),

                span {
                    style: "color: {color}; font-size: 13px; width: 14px; flex-shrink: 0;",
                    if turn.status == StepStatus::Running {
                        "⏳"
                    } else {
                        "{icon}"
                    }
                }

                span {
                    style: "
                        flex: 1;
                        font-size: 12px;
                        color: #bbb;
                        font-family: monospace;
                    ",
                    "{turn.title}"
                }

                span {
                    style: "color: #444; font-size: 11px;",
                    "{turn.steps.len()} steps  {chevron}"
                }
            }

            if expanded {
                div {
                    style: "padding: 0 14px 8px 14px;",
                    for step in turn.steps.iter() {
                        div {
                            style: "
                                display: flex;
                                align-items: baseline;
                                gap: 8px;
                                padding: 3px 0;
                            ",

                            span {
                                style: "
                                    color: {step.status.color()};
                                    font-size: 11px;
                                    width: 12px;
                                    flex-shrink: 0;
                                    font-family: monospace;
                                ",
                                "{step.status.icon()}"
                            }

                            span {
                                style: "
                                    font-size: 11px;
                                    color: #777;
                                    font-family: monospace;
                                    word-break: break-all;
                                ",
                                "{step.label()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn diagnostics_section(status: &BuildStatus) -> Element {
    match status {
        BuildStatus::Failed(diags) if !diags.is_empty() => {
            let mut expanded = use_signal(|| true);
            let chevron = if *expanded.read() { "▾" } else { "▸" };
            let count = diags.len();
            let error_label = format!("✗ {} error{}", count, if count == 1 { "" } else { "s" });

            rsx! {
                div {
                    style: "
                        border-top: 1px solid #2a1010;
                        flex-shrink: 0;
                        max-height: 280px;
                        display: flex;
                        flex-direction: column;
                    ",

                    div {
                        style: "
                            padding: 6px 14px;
                            display: flex;
                            align-items: center;
                            justify-content: space-between;
                            cursor: pointer;
                            user-select: none;
                            background: #160808;
                        ",
                        onclick: move |_| {
                            let cur = *expanded.read();
                            expanded.set(!cur);
                        },
                        span {
                            style: "color: #ef4444; font-size: 11px;",
                            "{error_label}"
                        }
                        span {
                            style: "color: #555; font-size: 11px;",
                            "{chevron}"
                        }
                    }

                    if *expanded.read() {
                        div {
                            style: "overflow-y: auto; padding: 8px 14px;",
                            for diag in diags.iter() {
                                div {
                                    style: "
                                        margin-bottom: 10px;
                                        font-family: monospace;
                                        font-size: 11px;
                                    ",
                                    div {
                                        style: "color: #ef4444; margin-bottom: 2px;",
                                        if let Some(code) = &diag.code {
                                            "error[{code}]: {diag.message}"
                                        } else {
                                            "error: {diag.message}"
                                        }
                                    }
                                    div {
                                        style: "color: #555;",
                                        { format!("→ {}:{}", diag.file, diag.line) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => rsx! { div {} },
    }
}
