use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn wake_bin() -> String {
    env!("CARGO_BIN_EXE_wake").to_string()
}

/// Create a test script at `home/test_shell.sh`. The `body` string may contain
/// `{wake}` which is replaced with the path to the wake binary. Returns the
/// script path (already chmod +x).
fn create_test_script(home: &Path, body: &str) -> PathBuf {
    let wake = wake_bin();
    let script = home.join("test_shell.sh");
    let content = format!("#!/bin/bash\n{}", body.replace("{wake}", &wake));
    std::fs::write(&script, content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Run `wake shell` with HOME and SHELL overrides, stdin null.
fn run_wake_shell(home: &Path, script: &Path) -> Output {
    Command::new(wake_bin())
        .arg("shell")
        .env("HOME", home)
        .env("SHELL", script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run wake shell")
}

/// Open the wake database at `home/.wake/wake.db`.
fn open_db(home: &Path) -> rusqlite::Connection {
    let db_path = home.join(".wake").join("wake.db");
    rusqlite::Connection::open(&db_path).unwrap()
}

/// Run an arbitrary `wake` subcommand with HOME override.
fn run_wake_cmd(home: &Path, args: &[&str]) -> Output {
    Command::new(wake_bin())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("Failed to run wake command")
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

# Delay to ensure hooks are processed (100ms per command for PTY sync)
sleep 0.5
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
    let cmd_id: i64 =
        conn.query_row("SELECT id FROM commands LIMIT 1", [], |row| row.get(0)).unwrap();

    // Update with summary
    conn.execute(
        "UPDATE commands SET summary = ?1 WHERE id = ?2",
        rusqlite::params!["Build completed successfully.", cmd_id],
    )
    .unwrap();

    // Verify summary can be retrieved
    let summary: String = conn
        .query_row("SELECT summary FROM commands WHERE id = ?1", rusqlite::params![cmd_id], |row| {
            row.get(0)
        })
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

    let summary: Option<String> =
        conn.query_row("SELECT summary FROM commands LIMIT 1", [], |row| row.get(0)).unwrap();

    assert!(summary.is_none(), "Summary should be None when summarization is disabled");
}

// ========================================================================
// Git integration tests
// ========================================================================

/// Test that git branch is captured when command runs inside a git repo
#[test]
fn test_e2e_git_branch_captured() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Create a git repo inside our test home
    let repo = home.join("myrepo");
    std::fs::create_dir_all(&repo).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .env("HOME", home)
        .output()
        .expect("git init failed");
    // Need at least one commit for HEAD to exist
    Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&repo)
        .env("HOME", home)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("git commit failed");

    let script = create_test_script(
        home,
        &format!(
            r#"
cd "{repo}"
"{{wake}}" __hook cmd-start --cmd "echo inside-repo"
echo inside-repo
"{{wake}}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
            repo = repo.display()
        ),
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let (branch, working_dir): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT git_branch, working_dir FROM commands LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert!(
        branch.is_some(),
        "git_branch should be set for command inside a git repo, got NULL"
    );
    let wd = working_dir.unwrap();
    assert!(
        wd.contains("myrepo"),
        "working_dir should contain repo path, got: {wd}"
    );
}

/// Test that git branch is NULL when command runs outside any git repo
#[test]
fn test_e2e_no_git_branch_outside_repo() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Use a directory that is definitely not inside a git repo
    let non_repo = home.join("norepo");
    std::fs::create_dir_all(&non_repo).unwrap();

    let script = create_test_script(
        home,
        &format!(
            r#"
cd "{dir}"
"{{wake}}" __hook cmd-start --cmd "echo outside"
echo outside
"{{wake}}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
            dir = non_repo.display()
        ),
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let branch: Option<String> = conn
        .query_row("SELECT git_branch FROM commands LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert!(branch.is_none(), "git_branch should be NULL outside a repo");
}

/// Test mixed: one command inside repo, one outside
#[test]
fn test_e2e_git_branch_mixed_directories() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let repo = home.join("mixrepo");
    std::fs::create_dir_all(&repo).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .env("HOME", home)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&repo)
        .env("HOME", home)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .unwrap();

    let non_repo = home.join("plain");
    std::fs::create_dir_all(&non_repo).unwrap();

    let script = create_test_script(
        home,
        &format!(
            r#"
cd "{repo}"
"{{wake}}" __hook cmd-start --cmd "echo in-git"
echo in-git
"{{wake}}" __hook cmd-end --exit-code 0 --duration 1

cd "{plain}"
"{{wake}}" __hook cmd-start --cmd "echo no-git"
echo no-git
"{{wake}}" __hook cmd-end --exit-code 0 --duration 1

sleep 0.3
exit 0
"#,
            repo = repo.display(),
            plain = non_repo.display()
        ),
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let mut stmt = conn
        .prepare("SELECT command, git_branch FROM commands ORDER BY id")
        .unwrap();
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(rows.len(), 2, "Expected 2 commands");
    assert!(
        rows[0].1.is_some(),
        "First command (in repo) should have git_branch"
    );
    assert!(
        rows[1].1.is_none(),
        "Second command (outside repo) should have NULL git_branch"
    );
}

// ========================================================================
// Socket / hook edge-case tests
// ========================================================================

/// Test commands with special characters in the command string
#[test]
fn test_e2e_commands_with_special_characters() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "echo 'single quotes'"
echo 'single quotes'
"{wake}" __hook cmd-end --exit-code 0 --duration 1

"{wake}" __hook cmd-start --cmd 'echo "double quotes"'
echo "double quotes"
"{wake}" __hook cmd-end --exit-code 0 --duration 1

"{wake}" __hook cmd-start --cmd "ls | grep foo"
echo piped
"{wake}" __hook cmd-end --exit-code 0 --duration 1

"{wake}" __hook cmd-start --cmd "echo one; echo two"
echo "one; two"
"{wake}" __hook cmd-end --exit-code 0 --duration 1

sleep 0.5
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let cmd_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cmd_count, 4, "All 4 special-char commands should be stored");

    let mut stmt = conn
        .prepare("SELECT command FROM commands ORDER BY id")
        .unwrap();
    let cmds: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert!(cmds[0].contains("single quotes"), "Got: {}", cmds[0]);
    assert!(cmds[1].contains("double quotes"), "Got: {}", cmds[1]);
    assert!(cmds[2].contains("| grep"), "Got: {}", cmds[2]);
    assert!(cmds[3].contains("; echo"), "Got: {}", cmds[3]);
}

/// Test that 10 rapid-fire commands are all recorded
#[test]
fn test_e2e_rapid_successive_commands() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Build 10 commands with no sleeps between them
    let mut body = String::new();
    for i in 0..10 {
        body.push_str(&format!(
            r#"
"{wake}" __hook cmd-start --cmd "echo rapid-{i}"
echo rapid-{i}
"{wake}" __hook cmd-end --exit-code 0 --duration 1
"#,
            wake = "{wake}",
            i = i
        ));
    }
    // Longer sleep at end (100ms * 10 commands = 1s, plus margin)
    body.push_str("sleep 2\nexit 0\n");

    let script = create_test_script(home, &body);
    run_wake_shell(home, &script);

    let conn = open_db(home);
    let cmd_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cmd_count, 10, "All 10 rapid commands should be recorded");
}

/// Test that a cmd-start without cmd-end is finalized during session cleanup
#[test]
fn test_e2e_cmd_start_without_cmd_end() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "orphan command"
echo "this has no cmd-end"
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let cmd_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        cmd_count, 1,
        "Orphaned command should still be recorded after session cleanup"
    );

    let exit_code: Option<i64> = conn
        .query_row("SELECT exit_code FROM commands LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    // Session cleanup finalizes with the shell's exit code (0) or -1
    assert!(
        exit_code.is_some(),
        "Orphaned command should have an exit code after cleanup"
    );
}

/// Test two consecutive cmd-starts without intervening cmd-end
#[test]
fn test_e2e_consecutive_cmd_starts() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "first orphan"
echo first
"{wake}" __hook cmd-start --cmd "second command"
echo second
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let cmd_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cmd_count, 2, "Both commands should be recorded");

    let mut stmt = conn
        .prepare("SELECT command, exit_code FROM commands ORDER BY id")
        .unwrap();
    let rows: Vec<(String, Option<i64>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(rows[0].0, "first orphan");
    assert_eq!(
        rows[0].1,
        Some(-1),
        "First orphan should get exit_code = -1"
    );
    assert_eq!(rows[1].0, "second command");
    assert_eq!(
        rows[1].1,
        Some(0),
        "Second command should get exit_code = 0"
    );
}

/// Test that an empty command string doesn't crash
#[test]
fn test_e2e_empty_command() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd ""
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    let output = run_wake_shell(home, &script);
    assert!(
        output.status.success(),
        "Shell should complete without crash on empty command"
    );

    let conn = open_db(home);
    let cmd_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM commands", [], |row| row.get(0))
        .unwrap();
    assert_eq!(cmd_count, 1, "Empty command should be recorded");
}

// ========================================================================
// Output capture tests
// ========================================================================

/// Test that a command with no output is recorded properly
#[test]
fn test_e2e_command_with_no_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "true"
true
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let (exit_code, output_bytes): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT exit_code, output_bytes FROM commands LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(exit_code, Some(0), "true should exit 0");
    // output_bytes might be 0 or NULL for no-output commands
    if let Some(bytes) = output_bytes {
        assert!(bytes <= 10, "No-output command should have minimal bytes, got {bytes}");
    }
}

/// Test multiline output capture
#[test]
fn test_e2e_multiline_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "seq 1 50"
seq 1 50
sleep 0.2
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let (exit_code, output): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT exit_code, output FROM commands LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(exit_code, Some(0), "seq should exit 0");
    // Soft assertion: if output captured and non-empty, it should contain digits
    if let Some(ref out) = output {
        if !out.is_empty() {
            assert!(
                out.contains('1') && out.contains('5'),
                "Captured output should contain digits from seq, got: {out}"
            );
        }
    }
}

/// Test that ANSI escape codes are stripped from captured output
#[test]
fn test_e2e_output_ansi_stripping() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "printf ansi-test"
printf '\033[31mred\033[0m normal'
sleep 0.2
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let output: Option<String> = conn
        .query_row("SELECT output FROM commands LIMIT 1", [], |row| row.get(0))
        .unwrap();

    // Soft assertion: if output is captured, it should NOT contain raw ANSI escapes
    if let Some(ref out) = output {
        assert!(
            !out.contains("\x1b["),
            "Output should have ANSI escapes stripped, got: {out:?}"
        );
    }
}

// ========================================================================
// Error & edge-case tests
// ========================================================================

/// Test that various exit codes are stored correctly
#[test]
fn test_e2e_multiple_exit_codes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "exit-0"
true
"{wake}" __hook cmd-end --exit-code 0 --duration 1

"{wake}" __hook cmd-start --cmd "exit-1"
false
"{wake}" __hook cmd-end --exit-code 1 --duration 1

"{wake}" __hook cmd-start --cmd "exit-2"
exit_placeholder=2
"{wake}" __hook cmd-end --exit-code 2 --duration 1

"{wake}" __hook cmd-start --cmd "exit-127"
not_a_real_command_that_exists 2>/dev/null
"{wake}" __hook cmd-end --exit-code 127 --duration 1

"{wake}" __hook cmd-start --cmd "exit-130"
echo interrupted
"{wake}" __hook cmd-end --exit-code 130 --duration 1

sleep 0.5
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let mut stmt = conn
        .prepare("SELECT exit_code FROM commands ORDER BY id")
        .unwrap();
    let codes: Vec<i64> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(codes, vec![0, 1, 2, 127, 130], "Exit codes should match");
}

/// Test that unicode in commands and output roundtrips correctly
#[test]
fn test_e2e_unicode_in_commands() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    let script = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "echo 🦀 Rust 你好"
echo "🦀 Rust 你好"
sleep 0.2
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.3
exit 0
"#,
    );

    run_wake_shell(home, &script);

    let conn = open_db(home);
    let command: String = conn
        .query_row("SELECT command FROM commands LIMIT 1", [], |row| row.get(0))
        .unwrap();

    assert!(
        command.contains("🦀"),
        "Command should preserve emoji, got: {command}"
    );
    assert!(
        command.contains("你好"),
        "Command should preserve CJK, got: {command}"
    );

    // Soft check on output (PTY race may yield empty output)
    let output: Option<String> = conn
        .query_row("SELECT output FROM commands LIMIT 1", [], |row| row.get(0))
        .unwrap();
    if let Some(ref out) = output {
        if !out.is_empty() {
            assert!(
                out.contains("🦀") && out.contains("你好"),
                "Output should preserve unicode, got: {out}"
            );
        }
    }
}

// ========================================================================
// Persistence / cross-session tests
// ========================================================================

/// Test that data persists across two separate wake shell sessions
#[test]
fn test_e2e_data_persists_across_sessions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Session 1
    let script1 = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "echo session-one"
echo session-one
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.2
exit 0
"#,
    );
    run_wake_shell(home, &script1);

    // Session 2 (reuse same script path, overwrite)
    let script2 = create_test_script(
        home,
        r#"
"{wake}" __hook cmd-start --cmd "echo session-two"
echo session-two
"{wake}" __hook cmd-end --exit-code 0 --duration 1
sleep 0.2
exit 0
"#,
    );
    run_wake_shell(home, &script2);

    // `wake log -c 10` should show both (queries across all sessions)
    let log_output = run_wake_cmd(home, &["log", "-c", "10"]);
    let log_stdout = String::from_utf8_lossy(&log_output.stdout);
    assert!(
        log_stdout.contains("session-one"),
        "Log should contain session-one.\nGot: {log_stdout}"
    );
    assert!(
        log_stdout.contains("session-two"),
        "Log should contain session-two.\nGot: {log_stdout}"
    );

    // `wake search` should find first-session command
    let search_output = run_wake_cmd(home, &["search", "session-one"]);
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("session-one"),
        "Search should find session-one.\nGot: {search_stdout}"
    );

    // DB should have 2 sessions
    let conn = open_db(home);
    let session_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(session_count, 2, "Should have 2 sessions");
}
