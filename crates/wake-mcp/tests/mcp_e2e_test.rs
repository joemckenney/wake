use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Spawn wake-mcp with HOME override, send JSON-RPC messages, return stdout.
fn send_mcp_messages_with_home(home: &Path, messages: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_wake-mcp"))
        .env("HOME", home)
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

/// Pre-populate a wake database with a session containing the given commands.
/// Returns the session id.
fn seed_session(home: &Path, commands: &[(&str, i32)]) -> String {
    let db_dir = home.join(".wake");
    std::fs::create_dir_all(&db_dir).unwrap();
    let conn = rusqlite::Connection::open(db_dir.join("wake.db")).unwrap();
    conn.execute_batch(include_str!("../../wake-core/src/schema.sql"))
        .unwrap();
    // Add summary column (migration)
    let _ = conn.execute("ALTER TABLE commands ADD COLUMN summary TEXT", []);

    let session_id = "test-session-001";
    conn.execute(
        "INSERT INTO sessions (id, started_at, ended_at) VALUES (?1, '2025-01-15T10:00:00Z', '2025-01-15T10:05:00Z')",
        rusqlite::params![session_id],
    )
    .unwrap();

    for (i, (cmd, exit_code)) in commands.iter().enumerate() {
        let started = format!("2025-01-15T10:0{}:00Z", i + 1);
        let ended = format!("2025-01-15T10:0{}:30Z", i + 1);
        let output_text = format!("output of {cmd}");
        conn.execute(
            "INSERT INTO commands (session_id, started_at, ended_at, command, exit_code, duration_ms, output, output_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, 1000, ?6, ?7)",
            rusqlite::params![
                session_id,
                started,
                ended,
                cmd,
                exit_code,
                output_text,
                output_text.len() as i64,
            ],
        )
        .unwrap();
    }

    session_id.to_string()
}

/// Test MCP wake_status with a pre-recorded session
#[test]
fn test_mcp_status_with_recorded_session() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    seed_session(home, &[("echo hello", 0), ("cargo build", 0)]);

    let output = send_mcp_messages_with_home(
        home,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"wake_status","arguments":{}}}"#,
        ],
    );

    assert!(
        output.contains("Commands: 2"),
        "Status should report 2 commands.\nGot: {output}"
    );
}

/// Test MCP wake_list_commands + wake_get_output with pre-recorded data
#[test]
fn test_mcp_list_and_get_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    seed_session(home, &[("echo hello", 0), ("cargo test", 1)]);

    // First list commands
    let list_output = send_mcp_messages_with_home(
        home,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"wake_list_commands","arguments":{"count":10}}}"#,
        ],
    );

    assert!(
        list_output.contains("echo hello"),
        "List should contain 'echo hello'.\nGot: {list_output}"
    );
    assert!(
        list_output.contains("cargo test"),
        "List should contain 'cargo test'.\nGot: {list_output}"
    );

    // Then get output for command ID 1
    let get_output = send_mcp_messages_with_home(
        home,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"wake_get_output","arguments":{"ids":[1]}}}"#,
        ],
    );

    assert!(
        get_output.contains("output of echo hello"),
        "Get output should contain the output text.\nGot: {get_output}"
    );
}
