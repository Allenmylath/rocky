use crate::serve::events::{BuildStage, RustcDiagnostic};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum BuildTarget {
    Client,
    Server,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildStatus {
    Idle,
    Building,
    Success,
    Failed(Vec<RustcDiagnostic>),
}

impl BuildStatus {
    pub fn is_failed(&self) -> bool {
        matches!(self, BuildStatus::Failed(_))
    }

    pub fn diagnostics(&self) -> Option<&Vec<RustcDiagnostic>> {
        match self {
            BuildStatus::Failed(diags) => Some(diags),
            _ => None,
        }
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics()
            .map(|d| d.iter().filter(|d| d.is_error()).count())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutoFixState {
    Idle,
    Countdown(u8),
    Stopped,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub project_path: Option<PathBuf>,
    pub build_status: BuildStatus,
    pub auto_fix: AutoFixState,
    pub serve_running: bool,
    pub auto_fix_iterations: u8,
    /// How many times we've tried to fix a fatal startup error (cargo metadata etc.)
    pub fatal_fix_iterations: u8,
    /// Raw lines from dx serve — last 200 kept for the log panel
    pub raw_log: Vec<String>,
    /// True when the currently open project is the built-in sample project.
    /// Used to show a "sample mode" banner and to know when the user has
    /// moved on to a real project.
    pub is_sample_project: bool,
    /// Current build stage for UI progress display
    pub build_stage: Option<BuildStage>,
    /// Which crate is currently being compiled
    pub current_compiling_crate: Option<String>,
}

pub const AUTO_FIX_COUNTDOWN_SECS: u8 = 5;

impl Default for SessionState {
    fn default() -> Self {
        Self {
            project_path: None,
            build_status: BuildStatus::Idle,
            auto_fix: AutoFixState::Idle,
            serve_running: false,
            auto_fix_iterations: 0,
            fatal_fix_iterations: 0,
            raw_log: Vec::new(),
            is_sample_project: false,
            build_stage: None,
            current_compiling_crate: None,
        }
    }
}

impl SessionState {
    pub fn project_name(&self) -> Option<&str> {
        self.project_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
    }

    pub fn on_build_started(&mut self) {
        self.build_status = BuildStatus::Building;
        self.auto_fix = AutoFixState::Idle;
        self.build_stage = Some(BuildStage::Initializing);
        self.current_compiling_crate = None;
    }

    pub fn on_progress(&mut self, stage: BuildStage) {
        self.build_stage = Some(stage);
    }

    pub fn on_compiling_crate(&mut self, krate: String) {
        self.current_compiling_crate = Some(krate);
    }

    pub fn on_build_success(&mut self) {
        self.build_status = BuildStatus::Success;
        self.auto_fix = AutoFixState::Idle;
        self.auto_fix_iterations = 0;
        self.fatal_fix_iterations = 0;
    }

    pub fn on_build_failed(&mut self, diagnostics: Vec<RustcDiagnostic>) {
        self.build_status = BuildStatus::Failed(diagnostics);
        self.auto_fix = AutoFixState::Countdown(AUTO_FIX_COUNTDOWN_SECS);
    }

    pub fn stop_auto_fix(&mut self) {
        self.auto_fix = AutoFixState::Stopped;
    }

    pub fn push_log(&mut self, line: String) {
        self.raw_log.push(line);
        if self.raw_log.len() > 200 {
            self.raw_log.remove(0);
        }
    }

    pub fn tick_countdown(&mut self) -> bool {
        if let AutoFixState::Countdown(n) = self.auto_fix {
            if n == 0 {
                self.auto_fix = AutoFixState::Idle;
                self.auto_fix_iterations += 1;
                return true;
            } else {
                self.auto_fix = AutoFixState::Countdown(n - 1);
            }
        }
        false
    }
}