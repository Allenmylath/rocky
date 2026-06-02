use crate::state::session::BuildTarget;

/// Severity level of a rustc diagnostic
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

impl DiagnosticLevel {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "error" => DiagnosticLevel::Error,
            "warning" => DiagnosticLevel::Warning,
            _ => DiagnosticLevel::Note,
        }
    }
}

/// A single rustc diagnostic parsed from `dx serve` stderr
#[derive(Debug, Clone, PartialEq)]
pub struct RustcDiagnostic {
    pub level: DiagnosticLevel,
    /// e.g. "E0502" — None for bare `error:` without a code
    pub code: Option<String>,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    /// Raw lines of the snippet block for display
    pub snippet: Vec<String>,
    /// Which compile target this came from
    pub target: BuildTarget,
}

impl RustcDiagnostic {
    pub fn is_error(&self) -> bool {
        self.level == DiagnosticLevel::Error
    }

    /// One-line summary for LLM context
    pub fn to_prompt_line(&self) -> String {
        let code = self
            .code
            .as_deref()
            .map(|c| format!("[{}] ", c))
            .unwrap_or_default();
        format!(
            "{}:{}: {}{}",
            self.file, self.line, code, self.message
        )
    }

    /// Full block including snippet — used in auto-fix prompt
    pub fn to_prompt_block(&self) -> String {
        let mut out = format!(
            "{} --> {}:{}:{}\n",
            match self.level {
                DiagnosticLevel::Error => "error",
                DiagnosticLevel::Warning => "warning",
                DiagnosticLevel::Note => "note",
            },
            self.file,
            self.line,
            self.col,
        );
        if let Some(code) = &self.code {
            out = format!("error[{}]{}", code, out.trim_start_matches("error"));
        }
        if !self.snippet.is_empty() {
            out.push_str("```\n");
            for line in &self.snippet {
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("```\n");
        }
        out
    }
}

/// Events emitted from the `dx serve` process watcher
/// Sent over a tokio mpsc channel to the rest of the app
#[derive(Debug, Clone)]
pub enum BuildEvent {
    /// dx serve started a new compile cycle
    BuildStarted,

    /// Compile succeeded — both client and server
    BuildSuccess,

    /// A diagnostic was parsed from stderr
    /// These accumulate until BuildSuccess or BuildFailed
    DiagnosticEmitted(RustcDiagnostic),

    /// Build finished with errors — signals end of diagnostic stream
    BuildFailed,

    /// The dx serve process itself exited unexpectedly
    ProcessExited { code: Option<i32> },

    /// Raw stdout line — for showing dx serve output verbatim if needed
    StdoutLine(String),
}