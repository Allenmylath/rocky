use std::path::Path;

/// Write content to a single file relative to the project root.
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

/// Write multiple files in parallel.
/// Failed writes are logged and skipped rather than aborting the whole batch.
pub async fn write_files_parallel(
    project_root: &Path,
    files: &[(String, String)],
) -> Vec<String> {
    let mut handles = Vec::with_capacity(files.len());
    for (path, content) in files {
        let root = project_root.to_path_buf();
        let path = path.clone();
        let content = content.clone();
        handles.push(tokio::spawn(async move {
            let full_path = root.join(&path);
            if let Some(parent) = full_path.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::warn!("Failed to create dirs for {}: {}", path, e);
                    return None;
                }
            }
            match tokio::fs::write(&full_path, &content).await {
                Ok(()) => Some(path),
                Err(e) => {
                    tracing::warn!("Failed to write {}: {}", path, e);
                    None
                }
            }
        }));
    }

    let mut succeeded = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(Some(path)) = handle.await {
            succeeded.push(path);
        }
    }
    succeeded
}
