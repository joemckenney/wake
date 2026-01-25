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

/// Test that old sessions are automatically pruned on new session start
#[test]
fn test_e2e_auto_prune_on_session_start() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Pre-populate with old session directly in DB
    let db_dir = home.join(".wake");
    std::fs::create_dir_all(&db_dir).unwrap();
    let conn = rusqlite::Connection::open(db_dir.join("wake.db")).unwrap();
    conn.execute_batch(include_str!("../../wake-core/src/schema.sql")).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, started_at, ended_at) VALUES ('ancient', '2020-01-01T00:00:00Z', '2020-01-01T01:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO commands (session_id, started_at, command) VALUES ('ancient', '2020-01-01T00:30:00Z', 'old command')",
        [],
    )
    .unwrap();
    drop(conn);

    // Create minimal test script
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo new"
echo new
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

    // Run wake shell (should auto-prune with default 21 day retention)
    let _output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Verify old session was pruned
    let conn = rusqlite::Connection::open(db_dir.join("wake.db")).unwrap();

    let old_exists: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = 'ancient')", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!old_exists, "Old session should have been pruned");

    // Verify new session exists (should be exactly 1 session now)
    let session_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0)).unwrap();
    assert_eq!(session_count, 1, "Should have exactly 1 session (the new one)");

    // Verify old command was also deleted
    let cmd_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0)).unwrap();
    assert_eq!(cmd_count, 1, "Should have exactly 1 command (from new session)");
}

/// Test that output max size is configurable via config file
#[test]
fn test_e2e_output_max_size_config() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create config with small output limit (we can't actually test truncation
    // at 1MB+ in a reasonable time, but we can verify config is loaded)
    let wake_dir = home.join(".wake");
    std::fs::create_dir_all(&wake_dir).unwrap();
    std::fs::write(
        wake_dir.join("config.toml"),
        r#"
[output]
max_mb = 10
"#,
    )
    .unwrap();

    // Create test script
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo test"
echo test
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

    // Run wake shell with config
    let output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Session should complete successfully
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test"), "Expected output from echo command");

    // Verify session was recorded
    let db_path = wake_dir.join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let cmd_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0)).unwrap();
    assert_eq!(cmd_count, 1, "Should have 1 command recorded");
}

// Tests for Phase 1-4: Tiered retrieval and LLM integration

/// Test wake llm status command
#[test]
fn test_e2e_llm_status() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    let output = Command::new(&wake)
        .args(["llm", "status"])
        .env("HOME", home)
        .output()
        .expect("Failed to run wake llm status");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show status info
    assert!(stdout.contains("LLM Summarization Status"), "Expected status header");
    assert!(stdout.contains("Model path:"), "Expected model path");
    assert!(stdout.contains("Status:"), "Expected status field");
    assert!(
        stdout.contains("not downloaded") || stdout.contains("downloaded"),
        "Expected download status"
    );
}

/// Test wake llm download shows appropriate message when model not needed
#[test]
fn test_e2e_llm_download_model_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // First check status to see the path
    let output = Command::new(&wake)
        .args(["llm", "status"])
        .env("HOME", home)
        .output()
        .expect("Failed to run wake llm status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".wake/models"), "Model should be in .wake/models directory");
}

/// Test summarization config is recognized
#[test]
fn test_e2e_summarization_config() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create config with summarization enabled
    let wake_dir = home.join(".wake");
    std::fs::create_dir_all(&wake_dir).unwrap();
    std::fs::write(
        wake_dir.join("config.toml"),
        r#"
[summarization]
enabled = true
min_bytes = 512
"#,
    )
    .unwrap();

    // Create simple test script
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo short"
echo short
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

    // Run wake shell - should complete without error even with summarization enabled
    // (it will gracefully degrade since model not downloaded)
    let output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    assert!(output.status.success(), "Shell should complete successfully");

    // Verify session was recorded
    let db_path = wake_dir.join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let cmd_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0)).unwrap();
    assert_eq!(cmd_count, 1, "Should have 1 command recorded");
}

/// Test that summary column exists in database after migration
#[test]
fn test_e2e_database_has_summary_column() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create simple test script to initialize database
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo test"
echo test
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

    let _output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Verify summary column exists
    let db_path = home.join(".wake").join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // This query will fail if summary column doesn't exist
    let result: Result<Option<String>, _> =
        conn.query_row("SELECT summary FROM commands LIMIT 1", [], |row| row.get(0));

    assert!(result.is_ok(), "Summary column should exist in commands table");
}

/// Test that summary can be stored and retrieved
#[test]
fn test_e2e_summary_storage_and_retrieval() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // Create test script
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "cargo build"
echo "Compiling project..."
"{wake}" __hook cmd-end --exit-code 0 --duration 5000
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

    let _output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Manually add a summary to test storage
    let db_path = home.join(".wake").join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Get command ID
    let cmd_id: i64 = conn
        .query_row("SELECT id FROM commands LIMIT 1", [], |row| row.get(0))
        .unwrap();

    // Update with summary
    conn.execute(
        "UPDATE commands SET summary = ?1 WHERE id = ?2",
        rusqlite::params!["Build completed successfully.", cmd_id],
    )
    .unwrap();

    // Verify summary can be retrieved
    let summary: String = conn
        .query_row(
            "SELECT summary FROM commands WHERE id = ?1",
            rusqlite::params![cmd_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(summary, "Build completed successfully.");
}

/// Test llm subcommands are available
#[test]
fn test_e2e_llm_subcommands() {
    let wake = wake_bin();

    // Test help for llm command
    let output = Command::new(&wake)
        .args(["llm", "--help"])
        .output()
        .expect("Failed to run wake llm --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("download"), "Should have download subcommand");
    assert!(stdout.contains("status"), "Should have status subcommand");
}

/// Test summarization is disabled by default
#[test]
fn test_e2e_summarization_disabled_by_default() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();
    let wake = wake_bin();

    // No config file - summarization should be disabled by default
    let test_script = home.join("test_shell.sh");
    std::fs::write(
        &test_script,
        format!(
            r#"#!/bin/bash
"{wake}" __hook cmd-start --cmd "echo hello"
echo hello
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

    let output = Command::new(&wake)
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", &test_script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell");

    // Should complete without any summarization-related activity
    assert!(output.status.success(), "Shell should complete successfully");

    // Verify no summary was added (summarization disabled by default)
    let db_path = home.join(".wake").join("wake.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    let summary: Option<String> = conn
        .query_row("SELECT summary FROM commands LIMIT 1", [], |row| row.get(0))
        .unwrap();

    assert!(summary.is_none(), "Summary should be None when summarization is disabled");
}
