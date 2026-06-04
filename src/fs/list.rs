use std::path::Path;

/// List all .rs files in the project's src/ directory, plus key root-level files.
/// Returns paths relative to the project root.
pub async fn list_src_files(project_root: &Path) -> anyhow::Result<Vec<String>> {
    let mut result = vec![];

    // Include root-level config files if they exist
    for name in &["Cargo.toml", "Dioxus.toml", "Cargo.lock", "AGENTS.md"] {
        if project_root.join(name).exists() {
            result.push(name.to_string());
        }
    }

    let src_path = project_root.join("src");
    if src_path.exists() {
        list_recursive(&src_path, project_root, &mut result).await?;
    }

    Ok(result)
}

fn list_recursive<'a>(
    dir: &'a Path,
    root: &'a Path,
    result: &'a mut Vec<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                list_recursive(&path, root, result).await?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(rel) = path.strip_prefix(root) {
                    result.push(rel.to_string_lossy().to_string());
                }
            }
        }
        Ok(())
    })
}