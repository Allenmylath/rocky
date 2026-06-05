use std::path::{Path, PathBuf};

pub mod store;
pub mod warmstart;

/// Available starter templates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TemplateKind {
    Blank,
    Store,
    Dashboard,
    LandingPage,
    ChatApp,
}

impl TemplateKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            TemplateKind::Blank => "Blank",
            TemplateKind::Store => "Store",
            TemplateKind::Dashboard => "Dashboard",
            TemplateKind::LandingPage => "Landing Page",
            TemplateKind::ChatApp => "Chat App",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            TemplateKind::Blank => "📄",
            TemplateKind::Store => "🛍️",
            TemplateKind::Dashboard => "📊",
            TemplateKind::LandingPage => "🚀",
            TemplateKind::ChatApp => "💬",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            TemplateKind::Blank => "Minimal counter app — a clean slate.",
            TemplateKind::Store => "Product grid + cart sidebar. E-commerce starter.",
            TemplateKind::Dashboard => "Charts + sidebar nav. Data-heavy starter.",
            TemplateKind::LandingPage => "Hero + features + CTA. Marketing starter.",
            TemplateKind::ChatApp => "Message list + input. Real-time starter.",
        }
    }
}

/// A template is a bag of (relative_path, content) pairs that can be written to
/// a project directory.  All content is baked into the binary via `include_str!`
/// style constants so no runtime asset loading is needed.
pub struct Template {
    pub kind: TemplateKind,
    pub files: Vec<(String, String)>,
}

impl Template {
    /// The current "blank" counter app (matches the original sample project).
    pub fn blank() -> Self {
        let cargo_toml = r#"[package]
name = "app"
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
            p { "This project is ready to edit." }
            button {
                style: "padding: 12px 24px; font-size: 16px; cursor: pointer; margin-top: 20px;",
                onclick: move |_| count += 1,
                "Clicked {count} times"
            }
        }
    }
}
"#;

        Self {
            kind: TemplateKind::Blank,
            files: vec![
                ("Cargo.toml".to_string(), cargo_toml.to_string()),
                ("src/main.rs".to_string(), main_rs.to_string()),
            ],
        }
    }

    /// A minimal e-commerce store with product grid and cart.
    pub fn store() -> Self {
        Self {
            kind: TemplateKind::Store,
            files: vec![
                ("Cargo.toml".to_string(), store::CARGO_TOML.to_string()),
                ("src/main.rs".to_string(), store::MAIN_RS.to_string()),
                ("src/state.rs".to_string(), store::STATE_RS.to_string()),
                ("src/components/mod.rs".to_string(), store::COMPONENTS_MOD_RS.to_string()),
                (
                    "src/components/product_grid.rs".to_string(),
                    store::PRODUCT_GRID_RS.to_string(),
                ),
                ("src/components/cart.rs".to_string(), store::CART_RS.to_string()),
                (
                    "src/components/header.rs".to_string(),
                    store::HEADER_RS.to_string(),
                ),
            ],
        }
    }

    /// Scaffold this template into `dest_dir`, creating parent directories as needed.
    /// Overwrites existing files.
    pub async fn scaffold(&self, dest_dir: &Path) -> anyhow::Result<()> {
        for (rel_path, content) in &self.files {
            let path = dest_dir.join(rel_path);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&path, content).await?;
        }
        Ok(())
    }
}

/// Convenience: get the directory where warm projects live.
pub fn warm_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("rocky").join("warm")
}

/// Convenience: get the directory where user projects live.
pub fn projects_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("rocky").join("projects")
}

/// Generate a slug from a free-form description, e.g. "Kerala handicrafts" → "kerala-handicrafts".
pub fn slugify(description: &str) -> String {
    description
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
}

/// Copy a directory recursively (includes hidden files).
/// Used to clone a warm project into a user project directory.
pub async fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let src = src.as_ref().to_path_buf();
    let dst = dst.as_ref().to_path_buf();

    tokio::fs::create_dir_all(&dst).await?;

    let mut entries = tokio::fs::read_dir(&src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            Box::pin(copy_dir_all(&src_path, &dst_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}
