use std::path::Path;

/// Read a file relative to the project root.
/// Returns the content as a UTF-8 string.
pub async fn read_file(project_root: &Path, relative_path: &str) -> anyhow::Result<String> {
    let full_path = project_root.join(relative_path);
    let content = tokio::fs::read_to_string(&full_path).await?;
    Ok(content)
}