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

/// Build stage mirroring Dioxus CLI's internal builder stages.
/// Emitted as `BuildEvent::Progress` so the UI can show compile/bundle progress.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildStage {
    Initializing,
    Compiling { current: usize, total: usize, krate: String },
    Bundling,
    Optimizing,
    Success,
    Failed,
    Aborted,
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
        if self.file.is_empty() {
            format!("{}{}", code, self.message)
        } else {
            format!(
                "{}:{}: {}{}",
                self.file, self.line, code, self.message
            )
        }
    }

    /// Full block including snippet — used in auto-fix prompt
    pub fn to_prompt_block(&self) -> String {
        let level_str = match self.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note => "note",
        };
        let mut out = if self.file.is_empty() {
            format!("{}: {}\n", level_str, self.message)
        } else {
            let mut loc = format!(
                "{} --> {}:{}:{}\n",
                level_str, self.file, self.line, self.col,
            );
            if let Some(code) = &self.code {
                loc = format!("error[{}]{}", code, loc.trim_start_matches("error"));
            }
            loc
        };
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

    /// dx serve encountered a fatal setup error (e.g. missing Cargo.toml)
    /// Carries a user-friendly message with a suggested fix.
    FatalError(String),

    /// Build stage progress update (compiling, bundling, etc.)
    Progress { stage: BuildStage },

    /// A specific crate started compiling
    CompilingCrate { krate: String },
}