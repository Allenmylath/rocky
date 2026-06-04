use std::path::PathBuf;

/// Directory where the sample project lives (e.g. %APPDATA%/rocky/sample-project)
pub fn sample_project_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("rocky").join("sample-project")
}

/// Create a minimal working Dioxus desktop project if it doesn't already exist.
/// Returns the path to the project directory.
pub async fn ensure_sample_project() -> anyhow::Result<PathBuf> {
    let dir = sample_project_dir();

    // Already scaffolded?
    if dir.join("Cargo.toml").exists() && dir.join("src").join("main.rs").exists() {
        return Ok(dir);
    }

    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::create_dir_all(dir.join("src")).await?;

    let cargo_toml = r#"[package]
name = "rocky-sample"
version = "0.1.0"
edition = "2021"

[dependencies]
dioxus = { version = "0.7.9", features = ["desktop"] }
"#;

    let main_rs = r#"use dioxus::prelude::*;

fn main() {
    dioxus::LaunchBuilder::desktop().launch(App);
}

#[component]
fn App() -> Element {
    let mut count = use_signal(|| 0);

    rsx! {
        div {
            style: "text-align: center; padding: 40px; font-family: sans-serif;",
            h1 { "Hello from Rocky! 🪨" }
            p { "This sample project is working correctly." }
            p { "Build succeeded — you're ready to start coding." }
            button {
                style: "padding: 12px 24px; font-size: 16px; cursor: pointer; margin-top: 20px;",
                onclick: move |_| count += 1,
                "Clicked {count} times"
            }
        }
    }
}
"#;

    let dioxus_toml = r#"[application]
name = "rocky-sample"
default_platform = "desktop"

[web.app]
title = "Rocky Sample"
"#;

    tokio::fs::write(dir.join("Cargo.toml"), cargo_toml).await?;
    tokio::fs::write(dir.join("src").join("main.rs"), main_rs).await?;
    tokio::fs::write(dir.join("Dioxus.toml"), dioxus_toml).await?;

    tracing::info!("Sample project scaffolded at {}", dir.display());
    Ok(dir)
}
