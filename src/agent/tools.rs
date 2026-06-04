use serde_json::{json, Value};

/// Returns the tool definitions to send to the Anthropic API.
///
/// Batch tools (`read_files`, `write_files`) allow the model to read or write
/// multiple files in a single turn, which Rocky executes in parallel.
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "read_file",
            "description": "Read the contents of a single file in the target Dioxus project.",
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
            "name": "read_files",
            "description": "Read multiple files at once. Use this instead of multiple individual read_file calls when you need to examine several files in the same turn. Files are read in parallel.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of file paths relative to the project root, e.g. ['src/main.rs', 'src/lib.rs']"
                    }
                },
                "required": ["paths"]
            }
        }),
        json!({
            "name": "write_file",
            "description": "Write or overwrite a single file in the target Dioxus project. Creates parent directories if needed. This will trigger dx serve to hot reload.",
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
            "name": "write_files",
            "description": "Write multiple files at once. Use this when you need to update several files in the same turn (e.g. a component + its styles + a test). Files are written in parallel.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "files": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["path", "content"]
                        },
                        "description": "List of {path, content} objects to write"
                    }
                },
                "required": ["files"]
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
pub const TOOL_NAMES: &[&str] = &[
    "read_file",
    "read_files",
    "write_file",
    "write_files",
    "list_files",
    "run_command",
];
