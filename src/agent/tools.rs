use serde_json::{json, Value};

/// Returns the tool definitions to send to the Anthropic API
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "read_file",
            "description": "Read the contents of a file in the target Dioxus project.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root, e.g. 'src/main.rs'"
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "write_file",
            "description": "Write or overwrite a file in the target Dioxus project. Creates parent directories if needed. This will trigger dx serve to hot reload.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root, e.g. 'src/components/header.rs'"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "list_files",
            "description": "List all Rust source files in the project's src/ directory, plus root-level config files (Cargo.toml, Dioxus.toml).",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "run_command",
            "description": "Run a cargo or dx command in the project directory and return its output. Use for: fixing dependencies (`cargo add serde@1`), formatting (`cargo fmt`), checking before serving (`cargo check`), scaffolding (`dx new --name foo`). Do NOT use to start or restart dx serve — Rocky manages that.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to run, e.g. 'cargo add tokio@1' or 'cargo fmt'"
                    }
                },
                "required": ["command"]
            }
        }),
    ]
}

/// Names of all registered tools — for validation
pub const TOOL_NAMES: &[&str] = &["read_file", "write_file", "list_files", "run_command"];