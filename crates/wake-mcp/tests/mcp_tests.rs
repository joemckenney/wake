use std::io::Write;
use std::process::{Command, Stdio};

fn wake_mcp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
}

fn send_messages(messages: &[&str]) -> String {
    let mut child = wake_mcp()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start wake-mcp");

    {
        let stdin = child.stdin.as_mut().unwrap();
        for msg in messages {
            writeln!(stdin, "{msg}").unwrap();
        }
    }

    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_initialize() {
    let output = send_messages(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
    ]);

    assert!(output.contains(r#""jsonrpc":"2.0""#));
    assert!(output.contains(r#""id":1"#));
    assert!(output.contains(r#""protocolVersion":"2024-11-05""#));
    assert!(output.contains(r#""name":"wake-mcp""#));
    assert!(output.contains(r#""tools""#));
}

#[test]
fn test_tools_list() {
    let output = send_messages(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ]);

    assert!(output.contains(r#""name":"wake_status""#));
    assert!(output.contains(r#""name":"wake_log""#));
    assert!(output.contains(r#""name":"wake_search""#));
    assert!(output.contains(r#""name":"wake_dump""#));
    assert!(output.contains(r#""name":"wake_annotate""#));
}

#[test]
fn test_tool_call_status() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_status","arguments":{{}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
    // With empty database, should report no sessions
    assert!(stdout.contains("No sessions recorded") || stdout.contains("Session:"));
}

#[test]
fn test_tool_call_log() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_log","arguments":{{"count":5}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
}

#[test]
fn test_tool_call_search() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_search","arguments":{{"query":"test"}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
    assert!(stdout.contains("No commands matching") || stdout.contains("Found"));
}

#[test]
fn test_unknown_method() {
    let output = send_messages(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"unknown/method","params":{}}"#,
    ]);

    assert!(output.contains(r#""error""#));
    assert!(output.contains("Unknown method"));
}

#[test]
fn test_parse_error() {
    let output = send_messages(&["not valid json"]);

    assert!(output.contains(r#""error""#));
    assert!(output.contains("Parse error"));
}

// Tests for new tiered retrieval tools (Phase 1)

#[test]
fn test_tools_list_includes_new_tools() {
    let output = send_messages(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    ]);

    // Verify new tiered retrieval tools are present
    assert!(output.contains(r#""name":"wake_list_commands""#));
    assert!(output.contains(r#""name":"wake_get_output""#));
}

#[test]
fn test_tool_call_list_commands_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_list_commands","arguments":{{"count":10}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
    // With empty database, should report no commands
    assert!(stdout.contains("No commands recorded"));
}

#[test]
fn test_tool_call_list_commands_default_count() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                // Call without count argument - should use default
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_list_commands","arguments":{{}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
}

#[test]
fn test_tool_call_get_output_empty_ids() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                // Call with empty ids array
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_get_output","arguments":{{"ids":[]}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should return error for empty ids
    assert!(stdout.contains("No command IDs") || stdout.contains("error"));
}

#[test]
fn test_tool_call_get_output_nonexistent_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                // Call with nonexistent id
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_get_output","arguments":{{"ids":[99999]}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
    assert!(stdout.contains("not found") || stdout.contains("no output"));
}

#[test]
fn test_tool_call_get_output_multiple_ids() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            {
                let stdin = child.stdin.as_mut().unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}}}}}}"#).unwrap();
                writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).unwrap();
                // Call with multiple nonexistent ids
                writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"wake_get_output","arguments":{{"ids":[1,2,3]}}}}}}"#).unwrap();
            }
            child.wait_with_output().unwrap()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""type":"text""#));
    // Should have sections for each ID
    assert!(stdout.contains("Command ID: 1") || stdout.contains("not found"));
}
