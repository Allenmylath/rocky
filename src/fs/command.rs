use std::path::Path;

/// Run a shell command in the project directory.
/// Returns combined stdout + stderr and exit code context.
pub async fn run_command(project_root: &Path, command: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("Empty command");
    }

    let output = tokio::process::Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(project_root)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run `{}`: {}", command, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().unwrap_or(-1);

    let mut result = String::new();
    if !stdout.trim().is_empty() {
        result.push_str(stdout.trim());
    }
    if !stderr.trim().is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(stderr.trim());
    }
    if code != 0 {
        result.push_str(&format!("\n[exit {}]", code));
    }
    if result.is_empty() {
        result.push_str("ok");
    }

    Ok(result)
}
