use crate::serve::events::{BuildEvent, RustcDiagnostic};
use crate::serve::parser::StderrParser;
use std::path::Path;

/// Run `cargo check` in the given project directory and parse the stderr
/// to extract full rustc diagnostics with file/line/column info.
///
/// This is a fallback for when `dx serve` only outputs vague dx-tracing
/// error lines (Format A) without file/location context. By running
/// `cargo check` we get the standard rustc human-readable format which
/// includes `--> file:line:col` locations and code snippets.
pub async fn run(project_path: &Path) -> anyhow::Result<Vec<RustcDiagnostic>> {
    let output = tokio::process::Command::new("cargo")
        .args(["check"])
        .current_dir(project_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run cargo check: {}. Is cargo installed?", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut parser = StderrParser::new();
    let mut diagnostics = Vec::new();

    for line in stderr.lines() {
        // Prepend a fake `[cargo]` tag so the existing parser routes the line
        // through its cargo-diagnostic state machine.
        let tagged = format!("[cargo] {}", line);
        for event in parser.feed_line(&tagged) {
            if let BuildEvent::DiagnosticEmitted(diag) = event {
                diagnostics.push(diag);
            }
        }
    }

    for event in parser.flush() {
        if let BuildEvent::DiagnosticEmitted(diag) = event {
            diagnostics.push(diag);
        }
    }

    Ok(diagnostics)
}
