use crate::serve::events::RustcDiagnostic;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

// ── Workflow tracking ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    Pending,
    Running,
    Complete,
    Failed,
}

impl StepStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            StepStatus::Pending => "○",
            StepStatus::Running => "⏳",
            StepStatus::Complete => "✓",
            StepStatus::Failed => "✗",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            StepStatus::Pending => "#555",
            StepStatus::Running => "#f97316",
            StepStatus::Complete => "#22c55e",
            StepStatus::Failed => "#ef4444",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepKind {
    ReadFile,
    WriteFile,
    ListFiles,
    Build,
    AutoFix { iteration: u8 },
}

impl StepKind {
    pub fn label(&self, summary: &str) -> String {
        match self {
            StepKind::ReadFile => format!("read  {}", summary),
            StepKind::WriteFile => format!("write {}", summary),
            StepKind::ListFiles => "list  src/".to_string(),
            StepKind::Build => summary.to_string(),
            StepKind::AutoFix { iteration } => {
                format!("auto-fix {}/5 → {}", iteration, summary)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowStep {
    pub kind: StepKind,
    pub summary: String,
    pub status: StepStatus,
}

impl WorkflowStep {
    pub fn new(kind: StepKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
            status: StepStatus::Running,
        }
    }

    pub fn build(msg: impl Into<String>) -> Self {
        Self {
            kind: StepKind::Build,
            summary: msg.into(),
            status: StepStatus::Running,
        }
    }

    pub fn label(&self) -> String {
        self.kind.label(&self.summary)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowTurn {
    pub id: u64,
    pub title: String,
    pub steps: Vec<WorkflowStep>,
    pub status: StepStatus,
    pub expanded: bool,
}

impl WorkflowTurn {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            title: title.into(),
            steps: vec![],
            status: StepStatus::Running,
            expanded: true,
        }
    }

    pub fn push_step(&mut self, step: WorkflowStep) {
        self.steps.push(step);
    }

    pub fn complete_last_step(&mut self) {
        for step in self.steps.iter_mut().rev() {
            if step.status == StepStatus::Running {
                step.status = StepStatus::Complete;
                break;
            }
        }
    }

    pub fn finish(&mut self, status: StepStatus) {
        for step in self.steps.iter_mut() {
            if step.status == StepStatus::Running {
                step.status = status.clone();
            }
        }
        self.status = status;
        if self.status == StepStatus::Complete {
            self.expanded = false;
        }
    }
}

// ── ToolCall ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub summary: String,
    pub status: StepStatus,
}

// ── ChatMessage ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub id: u64,
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub triggered_by: Option<Vec<RustcDiagnostic>>,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::User,
            content: content.into(),
            tool_calls: vec![],
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls,
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::System,
            content: content.into(),
            tool_calls: vec![],
            triggered_by: None,
            timestamp: now_ms(),
        }
    }

    pub fn auto_fix(diagnostics: Vec<RustcDiagnostic>) -> Self {
        let count = diagnostics.iter().filter(|d| d.is_error()).count();
        Self {
            id: now_ms(),
            role: MessageRole::System,
            content: format!(
                "Build failed with {} error{}. Auto-fixing...",
                count,
                if count == 1 { "" } else { "s" }
            ),
            tool_calls: vec![],
            triggered_by: Some(diagnostics),
            timestamp: now_ms(),
        }
    }

    pub fn fatal_error(msg: impl Into<String>) -> Self {
        Self {
            id: now_ms(),
            role: MessageRole::System,
            content: msg.into(),
            tool_calls: vec![],
            triggered_by: None,
            timestamp: now_ms(),
        }
    }
}

// ── ChatState ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub last_touched_files: Vec<String>,
    pub agent_running: bool,
    pub workflow_turns: Vec<WorkflowTurn>,
    pub active_turn_idx: Option<usize>,
}

impl ChatState {
    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.last_touched_files.clear();
        self.workflow_turns.clear();
        self.active_turn_idx = None;
    }

    pub fn begin_turn(&mut self, title: impl Into<String>) -> usize {
        let turn = WorkflowTurn::new(title);
        self.workflow_turns.push(turn);
        let idx = self.workflow_turns.len() - 1;
        self.active_turn_idx = Some(idx);
        idx
    }

    pub fn push_step(&mut self, step: WorkflowStep) {
        if let Some(idx) = self.active_turn_idx {
            if let Some(turn) = self.workflow_turns.get_mut(idx) {
                turn.push_step(step);
            }
        }
    }

    pub fn complete_last_step(&mut self) {
        if let Some(idx) = self.active_turn_idx {
            if let Some(turn) = self.workflow_turns.get_mut(idx) {
                turn.complete_last_step();
            }
        }
    }

    pub fn finish_turn(&mut self, status: StepStatus) {
        if let Some(idx) = self.active_turn_idx {
            if let Some(turn) = self.workflow_turns.get_mut(idx) {
                turn.finish(status);
            }
        }
        self.active_turn_idx = None;
    }

    pub fn toggle_turn(&mut self, turn_id: u64) {
        if let Some(turn) = self.workflow_turns.iter_mut().find(|t| t.id == turn_id) {
            turn.expanded = !turn.expanded;
        }
    }

    pub fn to_api_messages(&self) -> Vec<ApiMessage> {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| ApiMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => unreachable!(),
                },
                content: m.content.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
