use std::path::Path;

/// Write content to a file relative to the project root.
/// Creates parent directories if they don't exist.
pub async fn write_file(
    project_root: &Path,
    relative_path: &str,
    content: &str,
) -> anyhow::Result<()> {
    let full_path = project_root.join(relative_path);
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&full_path, content).await?;
    Ok(())
}