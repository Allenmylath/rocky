use crate::serve::events::{BuildEvent, DiagnosticLevel, RustcDiagnostic};
use crate::state::session::BuildTarget;

/// Stateful parser — accumulates lines across calls to handle multi-line diagnostic blocks
pub struct StderrParser {
    /// Lines accumulated for the current diagnostic block
    current_block: Vec<String>,
    /// The diagnostic being built (set when we see an `error[` header)
    current_diag: Option<PartialDiagnostic>,
    /// Which target we're currently seeing output from
    current_target: BuildTarget,
}

struct PartialDiagnostic {
    level: DiagnosticLevel,
    code: Option<String>,
    message: String,
    file: String,
    line: u32,
    col: u32,
    snippet: Vec<String>,
    target: BuildTarget,
}

impl StderrParser {
    pub fn new() -> Self {
        Self {
            current_block: vec![],
            current_diag: None,
            current_target: BuildTarget::Unknown,
        }
    }

    /// Feed one line of stderr — returns zero or more events
    pub fn feed_line(&mut self, line: &str) -> Vec<BuildEvent> {
        // TODO: implement in serve/parser.rs task
        vec![]
    }

    /// Flush any in-progress diagnostic at end of stream
    pub fn flush(&mut self) -> Vec<BuildEvent> {
        // TODO: implement in serve/parser.rs task
        vec![]
    }
}