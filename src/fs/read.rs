use std::path::Path;

/// Read a single file relative to the project root.
/// Returns the content as a UTF-8 string.
pub async fn read_file(project_root: &Path, relative_path: &str) -> anyhow::Result<String> {
    let full_path = project_root.join(relative_path);
    let content = tokio::fs::read_to_string(&full_path).await?;
    Ok(content)
}

/// Read multiple files in parallel, returning (path, content) pairs.
/// Failed reads are logged and skipped rather than aborting the whole batch.
pub async fn read_files_parallel(
    project_root: &Path,
    paths: &[String],
) -> Vec<(String, String)> {
    let mut handles = Vec::with_capacity(paths.len());
    for path in paths {
        let root = project_root.to_path_buf();
        let path = path.clone();
        handles.push(tokio::spawn(async move {
            match tokio::fs::read_to_string(&root.join(&path)).await {
                Ok(content) => Some((path, content)),
                Err(e) => {
                    tracing::warn!("Failed to read {}: {}", path, e);
                    None
                }
            }
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(Some(entry)) = handle.await {
            results.push(entry);
        }
    }
    results
}
