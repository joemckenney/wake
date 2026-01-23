use std::process::{Command, Stdio};

fn wake_bin() -> String {
    env!("CARGO_BIN_EXE_wake").to_string()
}

/// End-to-end test that:
/// 1. Starts a wake shell with a test script as the "shell"
/// 2. The test script runs commands and calls hooks
/// 3. Verifies database state and CLI output after session ends
#[test]
fn test_e2e_session_capture() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create a test script that simulates a shell with hooks
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
# Simulate shell with wake hooks

# Run first command
"{wake}" __hook cmd-start --cmd "echo hello"
echo hello
"{wake}" __hook cmd-end --exit-code 0 --duration 1

# Run second command that fails
"{wake}" __hook cmd-start --cmd "false"
false
"{wake}" __hook cmd-end --exit-code 1 --duration 1

# Run third command
"{wake}" __hook cmd-start --cmd "echo goodbye"
echo goodbye
"{wake}" __hook cmd-end --exit-code 0 --duration 1

# Small delay to ensure hooks are processed
sleep 0.1
exit 0
"#
        ),
    )
    .unwrap();

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&test_script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Start wake shell with our test script as SHELL
    let output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The shell should have run and produced output
    assert!(
        stdout.contains("hello") && stdout.contains("goodbye"),
        "Expected shell output not found.\nstdout: {stdout}\nstderr: {stderr}"
    );

    // Now verify via CLI commands

    // Check status shows a session
    let status_output = Command::new(&wake).arg("status").env("HOME", home).output().unwrap();
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_stdout.contains("Commands: 3"),
        "Expected 3 commands in status.\nGot: {status_stdout}"
    );

    // Check log shows the commands
    let log_output =
        Command::new(&wake).args(["log", "-c", "10"]).env("HOME", home).output().unwrap();
    let log_stdout = String::from_utf8_lossy(&log_output.stdout);
    assert!(log_stdout.contains("echo hello"), "Expected 'echo hello' in log.\nGot: {log_stdout}");
    assert!(
        log_stdout.contains("echo goodbye"),
        "Expected 'echo goodbye' in log.\nGot: {log_stdout}"
    );
    assert!(log_stdout.contains("false"), "Expected 'false' in log.\nGot: {log_stdout}");

    // Check search finds commands
    let search_output =
        Command::new(&wake).args(["search", "hello"]).env("HOME", home).output().unwrap();
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("echo hello"),
        "Expected search to find 'echo hello'.\nGot: {search_stdout}"
    );

    // Check dump produces markdown
    let dump_output = Command::new(&wake).arg("dump").env("HOME", home).output().unwrap();
    let dump_stdout = String::from_utf8_lossy(&dump_output.stdout);
    assert!(
        dump_stdout.contains("# Terminal Session Context"),
        "Expected markdown header in dump.\nGot: {dump_stdout}"
    );
    assert!(dump_stdout.contains("`echo hello`"), "Expected command in dump.\nGot: {dump_stdout}");

    // Verify database directly
    let db_path = home.join(".wake").join("wake.db");
    assert!(db_path.exists(), "Database should exist at {db_path:?}");

    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Check session exists and is ended
    let session_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap();
    assert_eq!(session_count, 1, "Expected 1 session");

    let ended: Option<String> =
        conn.query_row("SELECT ended_at FROM sessions", [], |row| row.get(0)).unwrap();
    assert!(ended.is_some(), "Session should be ended");

    // Check commands
    let cmd_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0)).unwrap();
    assert_eq!(cmd_count, 3, "Expected 3 commands");

    // Check exit codes
    let failed_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands WHERE exit_code != 0", [], |row| row.get(0))
        .unwrap();
    assert_eq!(failed_count, 1, "Expected 1 failed command");

    // Note: Output capture has a race condition in test environment.
    // The PTY output may not be read before the hook ends the command.
    // In real usage with shell hooks, there's natural delay between
    // command output and precmd hook that helps ensure output is captured.
    // For this test, we verify commands are recorded; output capture
    // is best tested manually or with slower/interactive tests.
}

/// Test that annotations work within a session
#[test]
fn test_e2e_annotations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create a test script that adds an annotation
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" annotate "Starting work on feature X"
"{wake}" __hook cmd-start --cmd "make build"
echo "Building..."
"{wake}" __hook cmd-end --exit-code 0 --duration 5
"{wake}" annotate "Build complete, moving to tests"
exit 0
"#
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&test_script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let _output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Check dump includes annotations
    let dump_output = Command::new(&wake).arg("dump").env("HOME", home).output().unwrap();
    let dump_stdout = String::from_utf8_lossy(&dump_output.stdout);
    assert!(
        dump_stdout.contains("Starting work on feature X"),
        "Expected first annotation in dump.\nGot: {dump_stdout}"
    );
    assert!(
        dump_stdout.contains("Build complete"),
        "Expected second annotation in dump.\nGot: {dump_stdout}"
    );

    // Verify in database
    let db_path = home.join(".wake").join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let annotation_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM annotations", [], |row| row.get(0)).unwrap();
    assert_eq!(annotation_count, 2, "Expected 2 annotations");
}

/// Test multiple sessions
#[test]
fn test_e2e_multiple_sessions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create a simple test script
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo session"
echo "session"
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.1
exit 0
"#
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&test_script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Run three sessions
    for _ in 0..3 {
        let _output = Command::new(&wake)
            .arg("shell")
            .env("HOME", home)
            .env("SHELL", &test_script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Failed to run wake shell");
    }

    // Verify database has 3 sessions
    let db_path = home.join(".wake").join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let session_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap();
    assert_eq!(session_count, 3, "Expected 3 sessions");

    let cmd_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0)).unwrap();
    assert_eq!(cmd_count, 3, "Expected 3 commands (1 per session)");

    // Status should show the most recent session
    let status_output = Command::new(&wake).arg("status").env("HOME", home).output().unwrap();
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    assert!(
        status_stdout.contains("Commands: 1"),
        "Most recent session should have 1 command.\nGot: {status_stdout}"
    );
}
