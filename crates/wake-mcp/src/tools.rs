use crate::protocol::{Tool, ToolCallResult};
use serde::Deserialize;
use serde_json::json;
use wake_core::Database;

pub fn list_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "wake_status",
            description: "Get current or most recent terminal session status",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        Tool {
            name: "wake_log",
            description: "Get recent commands with their output from terminal sessions",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "description": "Number of commands to return (default: 10)",
                        "default": 10
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "wake_list_commands",
            description: "List recent commands with metadata only (no full output). Returns id, command, exit_code, output_bytes, and summary. Use wake_get_output to fetch full output for specific commands.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "description": "Number of commands to return (default: 20)",
                        "default": 20
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "wake_get_output",
            description: "Get full output for specific command IDs. Use after wake_list_commands to fetch output for commands of interest.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ids": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "List of command IDs to fetch output for"
                    }
                },
                "required": ["ids"]
            }),
        },
        Tool {
            name: "wake_search",
            description: "Search command history by command text or output content",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to match against commands and output"
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "wake_dump",
            description:
                "Export a terminal session as markdown context, including all commands and outputs",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "Session ID to dump (default: most recent session)"
                    }
                },
                "required": []
            }),
        },
        Tool {
            name: "wake_annotate",
            description: "Add a note/annotation to the current terminal session",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "note": {
                        "type": "string",
                        "description": "Note text to add as annotation"
                    }
                },
                "required": ["note"]
            }),
        },
    ]
}

#[derive(Debug, Deserialize, Default)]
struct LogArgs {
    #[serde(default = "default_count")]
    count: usize,
}

fn default_count() -> usize {
    10
}

#[derive(Debug, Deserialize, Default)]
struct ListCommandsArgs {
    #[serde(default = "default_list_count")]
    count: usize,
}

fn default_list_count() -> usize {
    20
}

#[derive(Debug, Deserialize)]
struct GetOutputArgs {
    ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
}

#[derive(Debug, Deserialize, Default)]
struct DumpArgs {
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnnotateArgs {
    note: String,
}

pub fn call_tool(db: &Database, name: &str, arguments: serde_json::Value) -> ToolCallResult {
    match name {
        "wake_status" => tool_status(db),
        "wake_log" => {
            let args: LogArgs = serde_json::from_value(arguments).unwrap_or_default();
            tool_log(db, args.count)
        }
        "wake_list_commands" => {
            let args: ListCommandsArgs = serde_json::from_value(arguments).unwrap_or_default();
            tool_list_commands(db, args.count)
        }
        "wake_get_output" => match serde_json::from_value::<GetOutputArgs>(arguments) {
            Ok(args) => tool_get_output(db, &args.ids),
            Err(e) => ToolCallResult::error(format!("Invalid arguments: {e}")),
        },
        "wake_search" => match serde_json::from_value::<SearchArgs>(arguments) {
            Ok(args) => tool_search(db, &args.query),
            Err(e) => ToolCallResult::error(format!("Invalid arguments: {e}")),
        },
        "wake_dump" => {
            let args: DumpArgs = serde_json::from_value(arguments).unwrap_or_default();
            tool_dump(db, args.session_id.as_deref())
        }
        "wake_annotate" => match serde_json::from_value::<AnnotateArgs>(arguments) {
            Ok(args) => tool_annotate(db, &args.note),
            Err(e) => ToolCallResult::error(format!("Invalid arguments: {e}")),
        },
        _ => ToolCallResult::error(format!("Unknown tool: {name}")),
    }
}

fn tool_status(db: &Database) -> ToolCallResult {
    // Check for current session via env
    let session_id = std::env::var("WAKE_SESSION").ok();

    let session = if let Some(ref id) = session_id {
        match db.get_session(id) {
            Ok(s) => s,
            Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
        }
    } else {
        match db.get_most_recent_session() {
            Ok(s) => s,
            Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
        }
    };

    let Some(session) = session else {
        return ToolCallResult::text(
            "No sessions recorded yet. Run 'wake shell' to start a recorded session.",
        );
    };

    let cmd_count = db.count_session_commands(&session.id).unwrap_or(0);
    let status = if session.ended_at.is_some() { "ended" } else { "active" };

    let mut output = format!(
        "Session: {} ({})\nStarted: {}\n",
        session.id,
        status,
        session.started_at.format("%Y-%m-%d %H:%M:%S")
    );

    if let Some(ended) = session.ended_at {
        output.push_str(&format!("Ended: {}\n", ended.format("%Y-%m-%d %H:%M:%S")));
    }
    if let Some(shell) = &session.shell {
        output.push_str(&format!("Shell: {shell}\n"));
    }
    if let Some(root) = &session.project_root {
        output.push_str(&format!("Project: {root}\n"));
    }
    output.push_str(&format!("Commands: {cmd_count}"));

    ToolCallResult::text(output)
}

fn tool_log(db: &Database, count: usize) -> ToolCallResult {
    let commands = match db.get_recent_commands(count) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
    };

    if commands.is_empty() {
        return ToolCallResult::text("No commands recorded yet.");
    }

    let mut output = String::new();
    for cmd in commands.iter().rev() {
        let exit_indicator = match cmd.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "?",
        };

        let time = cmd.started_at.format("%H:%M:%S");
        let duration = cmd.duration_ms.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();

        output.push_str(&format!("{exit_indicator} [{time}]{duration} {}\n", cmd.command));

        if let Some(ref out) = cmd.output {
            let lines: Vec<_> = out.lines().take(5).collect();
            for line in &lines {
                output.push_str(&format!("  │ {}\n", truncate(line, 80)));
            }
            let total_lines = out.lines().count();
            if total_lines > 5 {
                output.push_str(&format!("  │ ... ({} more lines)\n", total_lines - 5));
            }
        }
        output.push('\n');
    }

    ToolCallResult::text(output)
}

fn tool_list_commands(db: &Database, count: usize) -> ToolCallResult {
    let commands = match db.get_recent_commands_metadata(count) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
    };

    if commands.is_empty() {
        return ToolCallResult::text("No commands recorded yet.");
    }

    let mut output = String::new();
    for cmd in commands.iter().rev() {
        let exit_indicator = match cmd.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "?",
        };

        let time = cmd.started_at.format("%H:%M:%S");
        let size = cmd.output_bytes.map(format_bytes).unwrap_or_default();
        let truncated = if cmd.truncated { " [truncated]" } else { "" };

        output.push_str(&format!(
            "{exit_indicator} [id:{} {time}] {} ({size}{truncated})\n",
            cmd.id, cmd.command
        ));

        if let Some(ref summary) = cmd.summary {
            output.push_str(&format!("  └ {summary}\n"));
        }
    }

    output.push_str("\nUse wake_get_output with ids to fetch full output for specific commands.");
    ToolCallResult::text(output)
}

fn tool_get_output(db: &Database, ids: &[i64]) -> ToolCallResult {
    if ids.is_empty() {
        return ToolCallResult::error("No command IDs provided");
    }

    let mut output = String::new();
    for &id in ids {
        match db.get_command_output(id) {
            Ok(Some(cmd_output)) => {
                output.push_str(&format!("=== Command ID: {} ===\n", id));
                output.push_str(&cmd_output);
                output.push_str("\n\n");
            }
            Ok(None) => {
                output
                    .push_str(&format!("=== Command ID: {} ===\n(no output or not found)\n\n", id));
            }
            Err(e) => {
                output.push_str(&format!("=== Command ID: {} ===\nError: {}\n\n", id, e));
            }
        }
    }

    ToolCallResult::text(output)
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn tool_search(db: &Database, query: &str) -> ToolCallResult {
    let commands = match db.search_commands(query) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
    };

    if commands.is_empty() {
        return ToolCallResult::text(format!("No commands matching '{query}' found."));
    }

    let mut output = format!("Found {} command(s) matching '{query}':\n\n", commands.len());

    for cmd in &commands {
        let exit_indicator = match cmd.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "?",
        };

        let time = cmd.started_at.format("%Y-%m-%d %H:%M:%S");
        output.push_str(&format!("{exit_indicator} [{time}] {}\n", cmd.command));

        if let Some(dir) = &cmd.working_dir {
            output.push_str(&format!("  dir: {dir}\n"));
        }

        if let Some(ref out) = cmd.output {
            let matching: Vec<_> = out
                .lines()
                .filter(|l| l.to_lowercase().contains(&query.to_lowercase()))
                .take(3)
                .collect();
            for line in matching {
                output.push_str(&format!("  > {}\n", truncate(line, 80)));
            }
        }
        output.push('\n');
    }

    ToolCallResult::text(output)
}

fn tool_dump(db: &Database, session_id: Option<&str>) -> ToolCallResult {
    let session = if let Some(id) = session_id {
        match db.get_session(id) {
            Ok(s) => s,
            Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
        }
    } else {
        match db.get_most_recent_session() {
            Ok(s) => s,
            Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
        }
    };

    let Some(session) = session else {
        return ToolCallResult::text("No sessions found.");
    };

    let commands = match db.get_session_commands(&session.id) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
    };

    let annotations = match db.get_session_annotations(&session.id) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Database error: {e}")),
    };

    let mut output = String::from("# Terminal Session Context\n\n");

    output.push_str(&format!("**Session ID:** {}\n", session.id));
    output.push_str(&format!(
        "**Started:** {}\n",
        session.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    if let Some(ended) = session.ended_at {
        output.push_str(&format!("**Ended:** {}\n", ended.format("%Y-%m-%d %H:%M:%S UTC")));
    }
    if let Some(shell) = &session.shell {
        output.push_str(&format!("**Shell:** {shell}\n"));
    }
    if let Some(root) = &session.project_root {
        output.push_str(&format!("**Project Root:** {root}\n"));
    }
    output.push('\n');

    if !annotations.is_empty() {
        output.push_str("## Annotations\n\n");
        for note in &annotations {
            output.push_str(&format!(
                "- **{}:** {}\n",
                note.timestamp.format("%H:%M:%S"),
                note.note
            ));
        }
        output.push('\n');
    }

    output.push_str("## Commands\n\n");

    for cmd in &commands {
        let exit_status = match cmd.exit_code {
            Some(0) => "✓".to_string(),
            Some(code) => format!("✗ (exit {code})"),
            None => "?".to_string(),
        };

        let branch = cmd.git_branch.as_ref().map(|b| format!(" [{b}]")).unwrap_or_default();

        output.push_str(&format!("### `{}`{branch} {exit_status}\n\n", cmd.command));

        if let Some(dir) = &cmd.working_dir {
            output.push_str(&format!("_in `{dir}`_\n\n"));
        }

        if let Some(ref out) = cmd.output {
            if !out.trim().is_empty() {
                output.push_str("```\n");
                let lines: Vec<_> = out.lines().collect();
                if lines.len() > 50 {
                    for line in &lines[..25] {
                        output.push_str(line);
                        output.push('\n');
                    }
                    output.push_str(&format!("... ({} lines omitted) ...\n", lines.len() - 50));
                    for line in &lines[lines.len() - 25..] {
                        output.push_str(line);
                        output.push('\n');
                    }
                } else {
                    for line in &lines {
                        output.push_str(line);
                        output.push('\n');
                    }
                }
                output.push_str("```\n");

                if cmd.truncated {
                    output.push_str("*Output was truncated*\n");
                }
            }
        }
        output.push('\n');
    }

    ToolCallResult::text(output)
}

fn tool_annotate(db: &Database, note: &str) -> ToolCallResult {
    let session_id =
        match std::env::var("WAKE_SESSION") {
            Ok(id) => id,
            Err(_) => return ToolCallResult::error(
                "Not inside a wake shell session. Run 'wake shell' to start a recorded session.",
            ),
        };

    match db.insert_annotation(&session_id, note) {
        Ok(_) => ToolCallResult::text("Annotation added."),
        Err(e) => ToolCallResult::error(format!("Failed to add annotation: {e}")),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
