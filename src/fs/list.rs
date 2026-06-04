use std::path::Path;

/// List all .rs files in the project's src/ directory, plus key root-level files.
/// Returns paths relative to the project root.
///
/// The filesystem walk runs in `spawn_blocking` so it doesn't starve the tokio runtime.
pub async fn list_src_files(project_root: &Path) -> anyhow::Result<Vec<String>> {
    let root = project_root.to_path_buf();

    let mut result = tokio::task::spawn_blocking(move || {
        let mut files = vec![];

        // Root-level config files
        for name in &["Cargo.toml", "Dioxus.toml", "Cargo.lock", "AGENTS.md"] {
            if root.join(name).exists() {
                files.push(name.to_string());
            }
        }

        // Walk src/ recursively
        let src_path = root.join("src");
        if src_path.exists() {
            let mut stack = vec![src_path];
            while let Some(dir) = stack.pop() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            stack.push(path);
                        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                            if let Ok(rel) = path.strip_prefix(&root) {
                                files.push(rel.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }

        files
    })
    .await?;

    // Sort for deterministic output
    result.sort();
    Ok(result)
}
