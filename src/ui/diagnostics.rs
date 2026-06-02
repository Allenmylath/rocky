use dioxus::prelude::*;

#[component]
pub fn DiagnosticsPanel() -> Element {
    rsx! { div { "diagnostics panel" } }
}

#[component]
pub fn AutoFixBanner() -> Element {
    rsx! { div { "auto fix banner" } }
}