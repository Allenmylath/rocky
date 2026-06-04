use crate::serve::events::{BuildEvent, DiagnosticLevel, RustcDiagnostic};
use crate::state::session::BuildTarget;

// ── Line prefix stripping ────────────────────────────────────────────────────
//
// dx serve prefixes every line with a timestamp and tag:
//   "21:28:19 [cargo] error: ..."
//   "21:28:19 [dev] Build failed: ..."
//
// We strip the prefix and route by tag.

#[derive(Debug, PartialEq)]
enum LineTag {
    Cargo,
    Dev,
    Other,
}

struct PrefixedLine<'a> {
    tag: LineTag,
    /// The content after the "[tag] " prefix, ANSI codes stripped
    content: String,
    _raw: &'a str,
}

/// Strip ANSI escape codes from a string
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // consume until 'm'
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

/// Parse a dx serve output line into tag + content
/// Format: "HH:MM:SS [tag] content" or "  N.NNs  TAG content"
fn parse_prefix(raw: &str) -> PrefixedLine<'_> {
    let clean = strip_ansi(raw);

    // Match "[cargo]" or "[dev]" anywhere in first ~30 chars
    let (tag, content) = if let Some(pos) = clean.find("[cargo]") {
        let rest = clean[pos + 7..].trim_start().to_string();
        (LineTag::Cargo, rest)
    } else if let Some(pos) = clean.find("[dev]") {
        let rest = clean[pos + 5..].trim_start().to_string();
        (LineTag::Dev, rest)
    } else if let Some(pos) = clean.find("INFO") {
        let rest = clean[pos + 4..].trim_start().to_string();
        (LineTag::Dev, rest)
    } else if let Some(pos) = clean.find("ERROR") {
        let rest = clean[pos + 5..].trim_start().to_string();
        (LineTag::Dev, rest)
    } else {
        (LineTag::Other, clean)
    };

    PrefixedLine {
        tag,
        content,
        _raw: raw,
    }
}

// ── Diagnostic header parsing ────────────────────────────────────────────────
//
// Patterns we need to match in [cargo] content:
//
//   error[E0308]: mismatched types        ← error with code
//   error: expected item, found keyword   ← error without code
//   warning[W...]: ...                    ← warning with code
//    --> src\main.rs:8:1                  ← location line
//   8 | let x: i32 = ...                 ← snippet line
//     | ^^^                              ← annotation line
//   error: could not compile `rocky`     ← end-of-errors sentinel
//   warning: `rocky` generated N warnings ← end-of-warnings

struct DiagHeader {
    level: DiagnosticLevel,
    code: Option<String>,
    message: String,
}

fn parse_diag_header(content: &str) -> Option<DiagHeader> {
    // Must start with "error" or "warning"
    let (level_str, rest) = if content.starts_with("error") {
        ("error", &content["error".len()..])
    } else if content.starts_with("warning") {
        ("warning", &content["warning".len()..])
    } else {
        return None;
    };

    let level = DiagnosticLevel::from_str(level_str);

    // Check for [CODE] after level
    let (code, message) = if rest.starts_with('[') {
        if let Some(end) = rest.find(']') {
            let code = rest[1..end].to_string();
            let msg = rest[end + 1..].trim_start_matches(':').trim().to_string();
            (Some(code), msg)
        } else {
            return None;
        }
    } else if rest.starts_with(':') {
        let msg = rest[1..].trim().to_string();
        (None, msg)
    } else {
        return None;
    };

    // Filter out the "could not compile" and "generated N warnings" sentinels
    // — these are not real diagnostics
    if message.starts_with("could not compile")
        || message.contains("generated")
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

/// Parse " --> src\main.rs:8:1" into (file, line, col)
fn parse_location(content: &str) -> Option<(String, u32, u32)> {
    let content = content.trim();
    if !content.starts_with("-->") {
        return None;
    }
    let loc = content[3..].trim();

    // Split on last ':' for col, second-to-last for line
    // Handle Windows paths like src\main.rs:8:1
    let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let col = parts[0].trim().parse::<u32>().ok()?;
    let line = parts[1].trim().parse::<u32>().ok()?;
    // Normalize Windows backslashes to forward slashes
    let file = parts[2].trim().replace('\\', "/");

    Some((file, line, col))
}

/// Detect build target from the long "Caused by: process didn't exit" line
/// which contains --target <triple>
fn detect_target(content: &str) -> Option<BuildTarget> {
    if content.contains("wasm32-unknown-unknown") {
        Some(BuildTarget::Client)
    } else if content.contains("--target") {
        // Any other explicit target is the native server build
        Some(BuildTarget::Server)
    } else {
        None
    }
}

// ── Parser state machine ─────────────────────────────────────────────────────

/// What state the parser is currently in
#[derive(Debug, PartialEq)]
enum ParserState {
    /// Waiting for a diagnostic header or lifecycle event
    Idle,
    /// Saw a diagnostic header, waiting for location line " --> "
    WaitingForLocation,
    /// Saw location, accumulating snippet lines
    AccumulatingSnippet,
}

/// Stateful parser — accumulates lines across calls to handle
/// multi-line diagnostic blocks from `dx serve` stderr output
pub struct StderrParser {
    state: ParserState,
    current_diag: Option<PartialDiagnostic>,
    current_target: BuildTarget,
    pending: Vec<BuildEvent>,
    /// Last non-lifecycle error message seen (used as fallback diagnostic when
    /// BuildFailed fires without any cargo-format diagnostics, e.g. WASM config errors)
    last_error_msg: Option<String>,
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
    fn into_event(self) -> BuildEvent {
        BuildEvent::DiagnosticEmitted(RustcDiagnostic {
            level: self.level,
            code: self.code,
            message: self.message,
            file: self.file,
            line: self.line,
            col: self.col,
            snippet: self.snippet,
            target: self.target,
        })
    }
}

impl StderrParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::Idle,
            current_diag: None,
            current_target: BuildTarget::Unknown,
            pending: vec![],
            last_error_msg: None,
        }
    }

    /// Feed one line of dx serve output — returns zero or more events
    pub fn feed_line(&mut self, raw: &str) -> Vec<BuildEvent> {
        let mut events = vec![];

        // Always forward every non-empty line to the raw log panel
        if !raw.trim().is_empty() {
            events.push(BuildEvent::StdoutLine(raw.to_string()));
        }

        let pl = parse_prefix(raw);

        match pl.tag {
            LineTag::Dev => {
                self.handle_dev_line(&pl.content, &mut events);
            }
            LineTag::Cargo => {
                self.handle_cargo_line(&pl.content, &mut events);
            }
            LineTag::Other => {
                if pl.content.contains("Serving your app") {
                    self.flush_current_diag(&mut events);
                    events.push(BuildEvent::BuildSuccess);
                    self.state = ParserState::Idle;
                    self.current_target = BuildTarget::Unknown;
                }
            }
        }

        events
    }

    fn handle_dev_line(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        if content.contains("Serving your app") {
            self.flush_current_diag(events);
            events.push(BuildEvent::BuildSuccess);
            self.state = ParserState::Idle;
            self.current_target = BuildTarget::Unknown;
        } else if content.contains("Build failed") {
            self.flush_current_diag(events);
            // If no cargo-format diagnostics were captured (e.g. WASM config errors
            // that dx emits as plain ERROR lines), synthesize one from the last
            // error message we saw so the user sees something useful.
            if let Some(msg) = self.last_error_msg.take() {
                events.push(BuildEvent::DiagnosticEmitted(RustcDiagnostic {
                    level: DiagnosticLevel::Error,
                    code: None,
                    message: msg,
                    file: String::new(),
                    line: 0,
                    col: 0,
                    snippet: vec![],
                    target: self.current_target.clone(),
                }));
            }
            events.push(BuildEvent::BuildFailed);
            self.state = ParserState::Idle;
        } else if content.contains("Compiling") || content.contains("Rebuilding") {
            self.flush_current_diag(events);
            self.last_error_msg = None;
            events.push(BuildEvent::BuildStarted);
            self.state = ParserState::Idle;
        } else if content.contains("cargo metadata") {
            events.push(BuildEvent::FatalError(content.to_string()));
        } else if !content.is_empty() {
            // Save non-lifecycle dev/error lines as a fallback message.
            // These capture things like "the wasm*-unknown-unknown targets are
            // not supported by default, you may need to enable the 'js' feature"
            self.last_error_msg = Some(content.to_string());
        }
    }

    fn handle_cargo_line(&mut self, content: &str, events: &mut Vec<BuildEvent>) {
        // Detect target from "Caused by" lines
        if let Some(target) = detect_target(content) {
            self.current_target = target;
        }

        match self.state {
            ParserState::Idle => {
                // Try to parse a diagnostic header
                if let Some(header) = parse_diag_header(content) {
                    self.current_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: vec![],
                        target: self.current_target.clone(),
                    });
                    self.state = ParserState::WaitingForLocation;
                }
                // "Compiling <crate>" signals a new build cycle
                else if content.starts_with("Compiling ") {
                    self.flush_current_diag(events);
                    events.push(BuildEvent::BuildStarted);
                }
            }

            ParserState::WaitingForLocation => {
                if let Some((file, line, col)) = parse_location(content) {
                    if let Some(ref mut diag) = self.current_diag {
                        diag.file = file;
                        diag.line = line;
                        diag.col = col;
                    }
                    self.state = ParserState::AccumulatingSnippet;
                } else if let Some(header) = parse_diag_header(content) {
                    // New diagnostic started before we got a location — flush old one
                    self.flush_current_diag(events);
                    self.current_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: vec![],
                        target: self.current_target.clone(),
                    });
                }
            }

            ParserState::AccumulatingSnippet => {
                // A new diagnostic header means the previous block is complete
                if let Some(header) = parse_diag_header(content) {
                    // But skip sentinels like "error: could not compile"
                    self.flush_current_diag(events);
                    self.current_diag = Some(PartialDiagnostic {
                        level: header.level,
                        code: header.code,
                        message: header.message,
                        file: String::new(),
                        line: 0,
                        col: 0,
                        snippet: vec![],
                        target: self.current_target.clone(),
                    });
                    self.state = ParserState::WaitingForLocation;
                } else {
                    // Accumulate snippet line — keep only pipe-prefixed lines
                    // which are the actual code context (not help/note prose)
                    let trimmed = content.trim();
                    if trimmed.starts_with('|')
                        || trimmed.starts_with("-->")
                        || (trimmed.len() > 0
                            && trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                            && trimmed.contains('|'))
                    {
                        if let Some(ref mut diag) = self.current_diag {
                            diag.snippet.push(content.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Flush the current in-progress diagnostic as an event
    fn flush_current_diag(&mut self, events: &mut Vec<BuildEvent>) {
        if let Some(diag) = self.current_diag.take() {
            // Only emit if we have at minimum a message
            // (skip incomplete diagnostics with no location)
            if !diag.message.is_empty() {
                events.push(diag.into_event());
            }
        }
        self.state = ParserState::Idle;
    }

    /// Call at end of stream to flush any in-progress diagnostic
    pub fn flush(&mut self) -> Vec<BuildEvent> {
        let mut events = vec![];
        self.flush_current_diag(&mut events);
        events
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cargo_line(content: &str) -> String {
        format!("21:28:19 [cargo] {}", content)
    }

    fn make_dev_line(content: &str) -> String {
        format!("21:28:19 [dev] {}", content)
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[33mwarning\x1b[0m: something";
        assert_eq!(strip_ansi(input), "warning: something");
    }

    #[test]
    fn test_parse_location_windows() {
        let result = parse_location("--> src\\main.rs:8:1");
        assert_eq!(result, Some(("src/main.rs".to_string(), 8, 1)));
    }

    #[test]
    fn test_parse_location_unix() {
        let result = parse_location("--> src/components/app.rs:42:5");
        assert_eq!(result, Some(("src/components/app.rs".to_string(), 42, 5)));
    }

    #[test]
    fn test_parse_diag_header_with_code() {
        let h = parse_diag_header("error[E0308]: mismatched types").unwrap();
        assert_eq!(h.code, Some("E0308".to_string()));
        assert_eq!(h.message, "mismatched types");
        assert_eq!(h.level, DiagnosticLevel::Error);
    }

    #[test]
    fn test_parse_diag_header_no_code() {
        let h = parse_diag_header("error: expected item, found keyword `let`").unwrap();
        assert!(h.code.is_none());
        assert_eq!(h.message, "expected item, found keyword `let`");
    }

    #[test]
    fn test_parse_diag_header_sentinel_skipped() {
        assert!(parse_diag_header("error: could not compile `rocky`").is_none());
    }

    #[test]
    fn test_full_error_block() {
        let mut parser = StderrParser::new();
        let lines = vec![
            make_cargo_line("error: expected item, found keyword `let`"),
            make_cargo_line(" --> src\\main.rs:8:1"),
            make_cargo_line("  |"),
            make_cargo_line("8 | let x: i32 = \"this is wrong\";"),
            make_cargo_line("  | ^^^"),
            make_cargo_line("  |"),
            make_cargo_line("  | `let` cannot be used for global variables"),
            make_cargo_line("error: could not compile `rocky` due to 1 previous error"),
            make_dev_line("Build failed: cargo build finished with errors"),
        ];

        let mut all_events = vec![];
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
        assert_eq!(diags[0].file, "src/main.rs");
        assert_eq!(diags[0].line, 8);
        assert_eq!(diags[0].col, 1);
        assert!(diags[0].message.contains("expected item"));

        let has_failed = all_events
            .iter()
            .any(|e| matches!(e, BuildEvent::BuildFailed));
        assert!(has_failed);
    }

    #[test]
    fn test_success_event() {
        let mut parser = StderrParser::new();
        let line = "  5.43s  INFO Serving your app: rocky! 🚀";
        let events = parser.feed_line(line);
        assert!(events.iter().any(|e| matches!(e, BuildEvent::BuildSuccess)));
    }

    #[test]
    fn test_target_detection_wasm() {
        assert_eq!(
            detect_target("--target wasm32-unknown-unknown -C opt-level"),
            Some(BuildTarget::Client)
        );
    }

    #[test]
    fn test_target_detection_native() {
        assert_eq!(
            detect_target("--target x86_64-pc-windows-msvc -C debuginfo"),
            Some(BuildTarget::Server)
        );
    }
}