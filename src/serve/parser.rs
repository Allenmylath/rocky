use crate::serve::events::{BuildEvent, BuildStage, DiagnosticLevel, RustcDiagnostic};
use crate::state::session::BuildTarget;

// ── ANSI stripping ───────────────────────────────────────────────────────────

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for cc in chars.by_ref() {
                if cc == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Line classification ──────────────────────────────────────────────────────
//
// dx serve emits lines in two distinct formats depending on the source:
//
// Format A — dx's own tracing logger (timestamped):
//   "  2.05s  INFO Serving your app..."
//   " 26.30s ERROR expected identifier"
//   " 28.27s ERROR Build failed: ..."
//
// Format B — cargo JSON rendered output (tagged):
//   "21:28:19 [cargo] error[E0308]: mismatched types"
//   "21:28:19 [cargo]  --> src/main.rs:8:1"
//   "21:28:19 [dev] Compiling 3 crates..."
//
// Both formats can appear in the same session. Format A errors have no
// file/line info. Format B errors have full diagnostic structure.

#[derive(Debug, PartialEq)]
enum LineKind {
    /// dx tracing INFO line — content after "INFO"
    DxInfo(String),
    /// dx tracing ERROR line — content after "ERROR"  
    DxError(String),
    /// cargo-tagged line — content after "[cargo]"
    Cargo(String),
    /// dev-tagged line — content after "[dev]"
    Dev(String),
    /// unclassified
    Other(String),
}

fn classify(raw: &str) -> LineKind {
    let clean = strip_ansi(raw);

    // Format B: tagged lines take priority
    if let Some(pos) = clean.find("[cargo]") {
        return LineKind::Cargo(clean[pos + 7..].trim_start().to_string());
    }
    if let Some(pos) = clean.find("[dev]") {
        return LineKind::Dev(clean[pos + 5..].trim_start().to_string());
    }

    // Format A: timestamped tracing lines
    // Pattern: optional whitespace, digits, '.', digits, 's', whitespace, LEVEL, space, content
    // e.g. "  2.05s  INFO ..." or " 26.30s ERROR ..."
    let trimmed = clean.trim_start();

    // Find the level keyword after the timestamp
    if let Some(info_pos) = find_level_keyword(trimmed, "INFO") {
        return LineKind::DxInfo(trimmed[info_pos..].trim_start().to_string());
    }
    if let Some(err_pos) = find_level_keyword(trimmed, "ERROR") {
        return LineKind::DxError(trimmed[err_pos..].trim_start().to_string());
    }
    if let Some(warn_pos) = find_level_keyword(trimmed, "WARN") {
        // treat WARN same as INFO for routing purposes
        return LineKind::DxInfo(trimmed[warn_pos..].trim_start().to_string());
    }

    LineKind::Other(clean)
}

/// Find a level keyword that appears after a timestamp-like prefix.
/// Returns the index *after* the keyword if found.
fn find_level_keyword(s: &str, keyword: &str) -> Option<usize> {
    let pos = s.find(keyword)?;
    // Sanity check: there should be only timestamp chars before it
    let before = s[..pos].trim();
    let looks_like_timestamp = before.is_empty()
        || before
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == 's');
    if looks_like_timestamp {
        Some(pos + keyword.len())
    } else {
        None
    }
}

// ── Cargo diagnostic header parsing ─────────────────────────────────────────
//
// Matches:
//   "error[E0308]: mismatched types"
//   "error: expected item, found keyword"
//   "warning[W...]: ..."

struct DiagHeader {
    level: DiagnosticLevel,
    code: Option<String>,
    message: String,
}

fn parse_diag_header(content: &str) -> Option<DiagHeader> {
    let (level_str, rest) = if content.starts_with("error") {
        ("error", &content["error".len()..])
    } else if content.starts_with("warning") {
        ("warning", &content["warning".len()..])
    } else {
        return None;
    };

    let level = DiagnosticLevel::from_str(level_str);

    let (code, message) = if rest.starts_with('[') {
        let end = rest.find(']')?;
        let code = rest[1..end].to_string();
        let msg = rest[end + 1..].trim_start_matches(':').trim().to_string();
        (Some(code), msg)
    } else if rest.starts_with(':') {
        let msg = rest[1..].trim().to_string();
        (None, msg)
    } else {
        return None;
    };

    // Filter sentinels — these are summary lines not real diagnostics
    if message.starts_with("could not compile")
        || message.contains("generated")
        || message.contains("aborting due to")
        || message.is_empty()
    {
        return None;
    }

    Some(DiagHeader {
        level,
        code,
        message,
    })
}

/// Parse " --> src/main.rs:8:1" into (file, line, col)
fn parse_location(content: &str) -> Option<(String, u32, u32)> {
    let content = content.trim();
    if !content.starts_with("-->") {
        return None;
    }
    let loc = content[3..].trim();
    let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let col = parts[0].trim().parse::<u32>().ok()?;
    let line = parts[1].trim().parse::<u32>().ok()?;
    let file = parts[2].trim().replace('\\', "/");
    Some((file, line, col))
}

/// Detect build target from lines mentioning --target <triple>
fn detect_target(content: &str) -> Option<BuildTarget> {
    if content.contains("wasm32-unknown-unknown") {
        Some(BuildTarget::Client)
    } else if content.contains("--target") {
        Some(BuildTarget::Server)
    } else {
        None
    }
}

/// Parse "Compiling X/Y: crate_name" or "Compiling crate_name vX.Y.Z"
/// Returns (current, total, krate_name) if it looks like a progress line,
/// or just (0, 0, krate_name) for a plain "Compiling crate_name" line.
fn parse_compiling_line(content: &str) -> Option<(usize, usize, String)> {
    // Strip "Compiling " prefix
    let rest = content.strip_prefix("Compiling ")?;

    // Try "N/M: name" format first
    if let Some(slash_pos) = rest.find('/') {
        let current_str = &rest[..slash_pos];
        if let Ok(current) = current_str.trim().parse::<usize>() {
            let after_slash = &rest[slash_pos + 1..];
            if let Some(colon_pos) = after_slash.find(':') {
                let total_str = &after_slash[..colon_pos];
                if let Ok(total) = total_str.trim().parse::<usize>() {
                    let krate = after_slash[colon_pos + 1..].trim().to_string();
                    return Some((current, total, krate));
                }
            }
        }
    }

    // Plain "Compiling crate_name vX.Y.Z (path)" — extract just the name
    let krate = rest
        .split_whitespace()
        .next()
        .unwrap_or(rest)
        .to_string();

    Some((0, 0, krate))
}

// ── Parser state machine ─────────────────────────────────────────────────────
//
// Key design decisions vs the old parser:
//
// 1. DxError lines (Format A) are collected into a separate `dx_errors` vec
//    rather than being routed through the cargo diagnostic state machine.
//    They become location-less RustcDiagnostics on BuildFailed.
//
// 2. BuildSuccess is only emitted after we verify no DxError lines or cargo
//    diagnostics are pending. The "Serving your app" line can appear before
//    error output on some dx versions, so we defer the success signal.
//
// 3. The cargo diagnostic state machine is unchanged in structure but now
//    correctly handles a new diagnostic header arriving while in
//    WaitingForLocation (flush + restart).
//
// 4. Progress events are emitted for Compiling lines.

#[derive(Debug, PartialEq)]
enum ParserState {
    Idle,
    WaitingForLocation,
    AccumulatingSnippet,
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

impl PartialDiagnostic {
    fn into_diagnostic(self) -> RustcDiagnostic {
        RustcDiagnostic {
            level: self.level,
            code: self.code,
            message: self.message,
            file: self.file,
            line: self.line,
            col: self.col,
            snippet: self.snippet,
            target: self.target,
        }
    }
}

pub struct StderrParser {
    state: ParserState,

    /// In-progress cargo-format diagnostic (has location)
    current_cargo_diag: Option<PartialDiagnostic>,

    /// Errors from dx's own ERROR lines (no location info)
    dx_errors: Vec<String>,

    /// All fully parsed cargo diagnostics so far this build cycle
    cargo_diagnostics: Vec<RustcDiagnostic>,

    current_target: BuildTarget,

    /// True once we've seen "Serving your app" but haven't confirmed
    /// there are no pending errors yet
    pending_success: bool,
}

impl StderrParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::Idle,
            current_cargo_diag: None,
            dx_errors: Vec::new(),
            cargo_diagnostics: Vec::new(),
            current_target: BuildTarget::Unknown,
            pending_success: false,
        }
    }

    /// Feed one raw line from dx serve output.
    /// Returns zero or more events to dispatch.
    pub fn feed_line(&mut self, raw: &str) -> Vec<BuildEvent> {
        let mut events = Vec::new();
        // Snapshot whether we already had a pending success *before* this line.
        // This lets us defer BuildSuccess emission until the NEXT non-error line.
        let had_pending = self.pending_success;

        // Always forward non-empty lines to the raw log
        if !raw.trim().is_empty() {
            events.push(BuildEvent::StdoutLine(raw.to_string()));
        }

        match classify(raw) {
            LineKind::DxInfo(content) => {
                self.handle_dx_info(&content, &mut events);
            }
            LineKind::DxError(content) => {
                self.handle_dx_error(&content, &mut events);
            }
            LineKind::Cargo(content) => {
                self.handle_cargo_line(&content, &mut events);
            }
            LineKind::Dev(content) => {
                self.handle_dev_line(&content, &mut events);
            }
            LineKind::Other(content) => {
                self.handle_other_line(&content, &mut events);
            }
        }

        // Deferred success confirmation:
        // Only emit BuildSuccess if pending_success was already true BEFORE this
        // line, it's still true after handling, this line is structured dx output
        // (not untagged boilerplate), and it's not an error signal.
        let is_structured_dx = matches!(
            classify(raw),
            LineKind::DxInfo(_) | LineKind::DxError(_) | LineKind::Dev(_)
        );
        if had_pending
            && self.pending_success
            && is_structured_dx
            && !raw.contains("ERROR")
            && !raw.contains("Build failed")
        {
            self.try_emit_success(&mut events);
        }

        events
    }

    /// Call at end of stream to flush any pending state
    pub fn flush(&mut self) -> Vec<BuildEvent> {
        let mut events = Vec::new();
        self.flush_cargo_diag(&mut events);
        events
    }

    // ── Line handlers ────────────────────────────────────────────────────────

    fn handle_dx_info(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        if content.contains("Serving your app") {
            // Don't emit success immediately — wait to confirm no errors follow.
            // We set a flag and emit on the next non-error line or on flush.
            self.pending_success = true;
        } else if content.contains("Rebuilding")
            || content.contains("Compiling")
            || content.contains("Starting build")
        {
            self.start_new_build(events);
        } else if content.contains("cargo metadata") {
            events.push(BuildEvent::FatalError(content.to_string()));
        } else if content.contains("Bundling") {
            events.push(BuildEvent::Progress {
                stage: BuildStage::Bundling,
            });
        } else if content.contains("Optimizing") {
            events.push(BuildEvent::Progress {
                stage: BuildStage::Optimizing,
            });
        }
    }

    fn handle_dx_error(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        // If we had a pending success, that's now invalidated
        self.pending_success = false;

        if content.contains("Build failed") {
            self.flush_cargo_diag(events);
            self.emit_build_failed(events);
        } else if content.contains("cargo metadata") {
            events.push(BuildEvent::FatalError(content.to_string()));
        } else {
            // Accumulate as a location-less error for later
            // Strip any remaining ANSI that snuck through
            let clean = strip_ansi(content);
            if !clean.trim().is_empty()
                && !clean.contains("could not compile")
                && !clean.contains("aborting due to")
                && !clean.contains("Some errors have detailed")
                && !clean.contains("For more information about")
            {
                self.dx_errors.push(clean.trim().to_string());
            }
        }
    }

    fn handle_cargo_line(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        // Detect target triple from linker invocation lines
        if let Some(target) = detect_target(content) {
            self.current_target = target;
        }

        // Cancel any pending success when cargo starts emitting diagnostics
        if parse_diag_header(content).is_some() {
            self.pending_success = false;
        }

        match self.state {
            ParserState::Idle => {
                if let Some(header) = parse_diag_header(content) {
                    self.current_cargo_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: Vec::new(),
                        target: self.current_target.clone(),
                    });
                    self.state = ParserState::WaitingForLocation;
                } else if content.starts_with("Compiling ") {
                    self.handle_compiling_line(content, events);
                }
            }

            ParserState::WaitingForLocation => {
                if let Some((file, line, col)) = parse_location(content) {
                    if let Some(ref mut diag) = self.current_cargo_diag {
                        diag.file = file;
                        diag.line = line;
                        diag.col = col;
                    }
                    self.state = ParserState::AccumulatingSnippet;
                } else if let Some(header) = parse_diag_header(content) {
                    // New diagnostic before we got a location — flush incomplete one
                    // An incomplete diagnostic with a message is still useful
                    self.flush_cargo_diag(events);
                    self.current_cargo_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: Vec::new(),
                        target: self.current_target.clone(),
                    });
                    // Stay in WaitingForLocation
                }
            }

            ParserState::AccumulatingSnippet => {
                if let Some(header) = parse_diag_header(content) {
                    self.flush_cargo_diag(events);
                    self.current_cargo_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: Vec::new(),
                        target: self.current_target.clone(),
                    });
                    self.state = ParserState::WaitingForLocation;
                } else {
                    // Accumulate snippet lines — pipe-prefixed or location lines
                    let trimmed = content.trim();
                    let is_snippet = trimmed.starts_with('|')
                        || trimmed.starts_with("-->")
                        || trimmed.starts_with("= ")
                        || (trimmed.len() > 0
                            && trimmed
                                .chars()
                                .next()
                                .map(|c| c.is_ascii_digit())
                                .unwrap_or(false)
                            && trimmed.contains('|'));
                    if is_snippet {
                        if let Some(ref mut diag) = self.current_cargo_diag {
                            diag.snippet.push(content.to_string());
                        }
                    }
                }
            }
        }
    }

    fn handle_dev_line(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        if content.contains("Serving your app") {
            self.pending_success = true;
        } else if content.contains("Build failed") {
            self.pending_success = false;
            self.flush_cargo_diag(events);
            self.emit_build_failed(events);
        } else if content.contains("Compiling") || content.contains("Rebuilding") {
            self.start_new_build(events);
        } else if content.contains("cargo metadata") {
            events.push(BuildEvent::FatalError(content.to_string()));
        }
    }

    fn handle_other_line(&mut self, content: &str, _events: &mut Vec<BuildEvent>) {
        if content.contains("Serving your app") {
            self.pending_success = true;
        }
    }

    fn handle_compiling_line(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        if let Some((current, total, krate)) = parse_compiling_line(content) {
            if current > 0 && total > 0 {
                events.push(BuildEvent::Progress {
                    stage: BuildStage::Compiling {
                        current,
                        total,
                        krate: krate.clone(),
                    },
                });
            }
            events.push(BuildEvent::CompilingCrate { krate });
        }
    }

    // ── State transitions ────────────────────────────────────────────────────

    fn start_new_build(&mut self, events: &mut Vec<BuildEvent>) {
        self.flush_cargo_diag(events);
        self.dx_errors.clear();
        self.cargo_diagnostics.clear();
        self.pending_success = false;
        self.current_target = BuildTarget::Unknown;
        self.state = ParserState::Idle;
        events.push(BuildEvent::BuildStarted);
        events.push(BuildEvent::Progress {
            stage: BuildStage::Initializing,
        });
    }

    /// Flush the in-progress cargo diagnostic into `cargo_diagnostics`
    /// and emit a `DiagnosticEmitted` event
    fn flush_cargo_diag(&mut self, events: &mut Vec<BuildEvent>) {
        if let Some(diag) = self.current_cargo_diag.take() {
            if !diag.message.is_empty() {
                let d = diag.into_diagnostic();
                events.push(BuildEvent::DiagnosticEmitted(d.clone()));
                self.cargo_diagnostics.push(d);
            }
        }
        self.state = ParserState::Idle;
    }

    /// Emit BuildFailed, synthesizing diagnostics from dx ERROR lines
    /// if no cargo-format diagnostics were captured
    fn emit_build_failed(&mut self, events: &mut Vec<BuildEvent>) {
        // If we have no cargo diagnostics but have dx ERROR lines,
        // synthesize location-less diagnostics from them so the LLM
        // gets something useful
        if self.cargo_diagnostics.is_empty() && !self.dx_errors.is_empty() {
            for msg in self.dx_errors.drain(..) {
                let d = RustcDiagnostic {
                    level: DiagnosticLevel::Error,
                    code: None,
                    message: msg,
                    file: String::new(),
                    line: 0,
                    col: 0,
                    snippet: Vec::new(),
                    target: self.current_target.clone(),
                };
                events.push(BuildEvent::DiagnosticEmitted(d.clone()));
                self.cargo_diagnostics.push(d);
            }
        } else {
            self.dx_errors.clear();
        }

        events.push(BuildEvent::BuildFailed);
        events.push(BuildEvent::Progress {
            stage: BuildStage::Failed,
        });

        // Reset for next cycle
        self.cargo_diagnostics.clear();
        self.state = ParserState::Idle;
        self.current_target = BuildTarget::Unknown;
    }

    fn try_emit_success(&mut self, events: &mut Vec<BuildEvent>) {
        if !self.pending_success {
            return;
        }
        self.pending_success = false;
        self.flush_cargo_diag(events);

        // Only emit success if we have no accumulated errors
        if self.cargo_diagnostics.is_empty() && self.dx_errors.is_empty() {
            events.push(BuildEvent::BuildSuccess);
            events.push(BuildEvent::Progress {
                stage: BuildStage::Success,
            });
        }
        // If there are errors, they'll be emitted when BuildFailed arrives
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify() tests ─────────────────────────────────────────────────────

    #[test]
    fn test_classify_dx_info() {
        let line = "  2.05s  INFO Serving your app: dioxus_app! 🚀";
        match classify(line) {
            LineKind::DxInfo(content) => assert!(content.contains("Serving your app")),
            other => panic!("expected DxInfo, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_dx_error() {
        let line = " 26.30s ERROR expected identifier";
        match classify(line) {
            LineKind::DxError(content) => assert_eq!(content, "expected identifier"),
            other => panic!("expected DxError, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_cargo_tagged() {
        let line = "21:28:19 [cargo] error[E0308]: mismatched types";
        match classify(line) {
            LineKind::Cargo(content) => assert!(content.starts_with("error[E0308]")),
            other => panic!("expected Cargo, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_ansi_stripped() {
        let line = "\x1b[33mwarning\x1b[0m: Waiting for cargo-metadata...";
        // No tag, no timestamp level keyword → Other
        match classify(line) {
            LineKind::Other(content) => assert!(content.contains("warning")),
            other => panic!("expected Other, got {:?}", other),
        }
    }

    // ── Real output sample from the bug report ───────────────────────────────

    #[test]
    fn test_real_dx_serve_output_sample() {
        let raw_lines = vec![
            "\x1b[33mwarning\x1b[0m: Waiting for cargo-metadata...",
            "  2.05s  INFO -----------------------------------------------------------------",
            "               Serving your app: dioxus_app! \u{1f680}",
            "               • Press \x1b[33m`ctrl+c`\x1b[0m to exit the server",
            " 26.30s ERROR expected identifier",
            " 26.68s ERROR unresolved import `crate::server::customer`",
            " 26.68s ERROR unresolved import `dioxus_desktop`",
            " 26.94s ERROR cannot find type `Scope` in this scope",
            " 27.47s ERROR cannot find function `use_state` in this scope",
            " 27.98s ERROR mismatched types",
            " 28.05s ERROR this method takes 1 argument but 0 arguments were supplied",
            " 28.21s ERROR Some errors have detailed explanations: E0061, E0308, E0425, E0432.",
            " 28.21s ERROR For more information about an error, try `rustc --explain E0061`.",
            " 28.27s ERROR \x1b[31mBuild failed\x1b[0m: cargo build finished with errors for target: dioxus_app [x86_64-pc-windows-msvc]",
        ];

        let mut parser = StderrParser::new();
        let mut all_events: Vec<BuildEvent> = Vec::new();

        for line in &raw_lines {
            all_events.extend(parser.feed_line(line));
        }
        all_events.extend(parser.flush());

        // Must NOT emit BuildSuccess
        assert!(
            !all_events.iter().any(|e| matches!(e, BuildEvent::BuildSuccess)),
            "BuildSuccess should not be emitted when errors follow"
        );

        // Must emit BuildFailed
        assert!(
            all_events.iter().any(|e| matches!(e, BuildEvent::BuildFailed)),
            "BuildFailed must be emitted"
        );

        // Must emit diagnostics for the real errors (not the sentinels)
        let diags: Vec<_> = all_events
            .iter()
            .filter_map(|e| match e {
                BuildEvent::DiagnosticEmitted(d) => Some(d),
                _ => None,
            })
            .collect();

        assert!(!diags.is_empty(), "should have diagnostics");

        // Sentinels filtered out
        assert!(
            !diags
                .iter()
                .any(|d| d.message.contains("Some errors have detailed")),
            "sentinel lines should be filtered"
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.message.contains("For more information")),
            "help lines should be filtered"
        );

        // Real errors present
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("expected identifier")));
        assert!(messages.iter().any(|m| m.contains("mismatched types")));
        assert!(messages.iter().any(|m| m.contains("unresolved import")));
    }

    // ── Cargo-format diagnostic block ────────────────────────────────────────

    #[test]
    fn test_cargo_format_full_block() {
        let mut parser = StderrParser::new();
        let lines = vec![
            "21:28:19 [cargo] error[E0308]: mismatched types",
            "21:28:19 [cargo]  --> src/main.rs:8:5",
            "21:28:19 [cargo]   |",
            "21:28:19 [cargo] 8 |     let x: i32 = \"hello\";",
            "21:28:19 [cargo]   |                   ^^^^^^^ expected `i32`, found `&str`",
            "21:28:19 [cargo] error: could not compile `rocky`",
            "21:28:19 [dev] Build failed: cargo build finished with errors",
        ];

        let mut all_events = Vec::new();
        for line in &lines {
            all_events.extend(parser.feed_line(line));
        }
        all_events.extend(parser.flush());

        let diags: Vec<_> = all_events
            .iter()
            .filter_map(|e| match e {
                BuildEvent::DiagnosticEmitted(d) => Some(d),
                _ => None,
            })
            .collect();

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Some("E0308".to_string()));
        assert_eq!(diags[0].file, "src/main.rs");
        assert_eq!(diags[0].line, 8);
        assert_eq!(diags[0].col, 5);
        assert!(!diags[0].snippet.is_empty());
    }

    // ── Clean build success ───────────────────────────────────────────────────

    #[test]
    fn test_clean_build_success() {
        let mut parser = StderrParser::new();
        let lines = vec![
            "  1.20s  INFO Compiling rocky v0.1.0",
            "  5.43s  INFO Serving your app: rocky! \u{1f680}",
            "  5.43s  INFO  • Press `ctrl+c` to exit",
        ];

        let mut all_events = Vec::new();
        for line in &lines {
            all_events.extend(parser.feed_line(line));
        }
        all_events.extend(parser.flush());

        assert!(
            all_events
                .iter()
                .any(|e| matches!(e, BuildEvent::BuildSuccess)),
            "clean build should emit BuildSuccess"
        );
        assert!(
            !all_events
                .iter()
                .any(|e| matches!(e, BuildEvent::BuildFailed)),
            "clean build should not emit BuildFailed"
        );
    }

    // ── Progress events ───────────────────────────────────────────────────────

    #[test]
    fn test_progress_compiling_emitted() {
        let mut parser = StderrParser::new();
        let events = parser.feed_line("21:28:19 [cargo] Compiling rocky v0.1.0 (/path)");
        assert!(events.iter().any(|e| matches!(e, BuildEvent::CompilingCrate { .. })));
    }

    // ── Windows path normalization ────────────────────────────────────────────

    #[test]
    fn test_windows_path_normalized() {
        let result = parse_location("--> src\\components\\app.rs:42:5");
        assert_eq!(
            result,
            Some(("src/components/app.rs".to_string(), 42, 5))
        );
    }

    // ── Target detection ─────────────────────────────────────────────────────

    #[test]
    fn test_wasm_target_detected() {
        assert_eq!(
            detect_target("--target wasm32-unknown-unknown"),
            Some(BuildTarget::Client)
        );
    }

    #[test]
    fn test_native_target_detected() {
        assert_eq!(
            detect_target("--target x86_64-pc-windows-msvc"),
            Some(BuildTarget::Server)
        );
    }

    // ── Real dx serve output from broken rocky project ───────────────────────
    // This test uses stderr captured from an actual `dx serve` run on a project
    // with compile errors (mismatched types, unresolved symbol). It validates
    // that the parser handles real-world output including WARN lines and
    // rustc command dumps that follow the actual errors.

    #[test]
    fn test_real_broken_project_output() {
        let raw_lines = vec![
            "  1.38s  INFO -----------------------------------------------------------------",
            "                Serving your app: rocky! 🚀",
            "               • Press `ctrl+c` to exit the server",
            "  5.74s ERROR cannot find `sample_project` in `crate`",
            "  5.76s ERROR cannot find value `unresolved_symbol_12345` in this scope",
            "  7.30s ERROR mismatched types",
            "  7.81s ERROR Some errors have detailed explanations: E0308, E0425, E0433.",
            "  7.81s ERROR For more information about an error, try `rustc --explain E0308`.",
            "  7.84s  WARN error: could not compile `rocky` (bin `rocky`) due to 3 previous errors; 6 warnings emitted",
            "  7.84s  WARN Caused by:",
            "  7.84s  WARN   process didn't exit successfully: `C:\\Users\\...\\rustc.exe` ...",
            "  7.84s ERROR Build failed: cargo build finished with errors for target: rocky [x86_64-pc-windows-msvc]",
        ];

        let mut parser = StderrParser::new();
        let mut all_events: Vec<BuildEvent> = Vec::new();

        for line in &raw_lines {
            all_events.extend(parser.feed_line(line));
        }
        all_events.extend(parser.flush());

        // Must NOT emit BuildSuccess (deferred success should not fire)
        assert!(
            !all_events.iter().any(|e| matches!(e, BuildEvent::BuildSuccess)),
            "BuildSuccess should not be emitted when errors follow"
        );

        // Must emit BuildFailed
        assert!(
            all_events.iter().any(|e| matches!(e, BuildEvent::BuildFailed)),
            "BuildFailed must be emitted"
        );

        // Extract diagnostics
        let diags: Vec<_> = all_events
            .iter()
            .filter_map(|e| match e {
                BuildEvent::DiagnosticEmitted(d) => Some(d),
                _ => None,
            })
            .collect();

        // Should have exactly 3 real errors (sentinels filtered)
        assert_eq!(diags.len(), 3, "Expected 3 diagnostics, got {:?}", diags);

        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("cannot find `sample_project`")));
        assert!(messages.iter().any(|m| m.contains("unresolved_symbol_12345")));
        assert!(messages.iter().any(|m| m.contains("mismatched types")));

        // Sentinels must be filtered
        assert!(
            !messages.iter().any(|m| m.contains("Some errors have detailed")),
            "sentinel lines should be filtered"
        );
        assert!(
            !messages.iter().any(|m| m.contains("For more information")),
            "help lines should be filtered"
        );
        assert!(
            !messages.iter().any(|m| m.contains("could not compile")),
            "cargo summary line should be filtered"
        );
    }
}
