use std::process::Command;

fn wake_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wake"))
}

#[test]
fn test_help() {
    let output = wake_cmd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Development context recorder"));
    assert!(stdout.contains("shell"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("log"));
    assert!(stdout.contains("dump"));
}

#[test]
fn test_version() {
    let output = wake_cmd().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wake"));
}

#[test]
fn test_init_zsh() {
    let output = wake_cmd().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("WAKE_SESSION"));
    assert!(stdout.contains("__wake_preexec"));
    assert!(stdout.contains("__wake_precmd"));
    assert!(stdout.contains("add-zsh-hook"));
}

#[test]
fn test_init_bash() {
    let output = wake_cmd().args(["init", "bash"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("WAKE_SESSION"));
    assert!(stdout.contains("__wake_preexec"));
    assert!(stdout.contains("PROMPT_COMMAND"));
    assert!(stdout.contains("trap"));
}

#[test]
fn test_init_unsupported_shell() {
    let output = wake_cmd().args(["init", "fish"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unsupported shell"));
}

#[test]
fn test_status_no_sessions() {
    // Use a temp HOME to avoid picking up real sessions
    let temp_dir = tempfile::tempdir().unwrap();
    let output = wake_cmd().arg("status").env("HOME", temp_dir.path()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No sessions recorded"));
}

#[test]
fn test_log_no_commands() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = wake_cmd().arg("log").env("HOME", temp_dir.path()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No commands recorded"));
}

#[test]
fn test_search_no_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output =
        wake_cmd().args(["search", "nonexistent"]).env("HOME", temp_dir.path()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No commands matching"));
}

#[test]
fn test_annotate_outside_session() {
    let output =
        wake_cmd().args(["annotate", "test note"]).env_remove("WAKE_SESSION").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WAKE_SESSION not set"));
}

#[test]
fn test_hook_cmd_start_outside_session() {
    let output = wake_cmd()
        .args(["__hook", "cmd-start", "--cmd", "ls"])
        .env_remove("WAKE_SOCKET")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WAKE_SOCKET not set"));
}

#[test]
fn test_hook_cmd_end_outside_session() {
    let output = wake_cmd()
        .args(["__hook", "cmd-end", "--exit-code", "0", "--duration", "100"])
        .env_remove("WAKE_SOCKET")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WAKE_SOCKET not set"));
}

#[test]
fn test_prune_help() {
    let output = wake_cmd().args(["prune", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--force"));
    assert!(stdout.contains("--older-than"));
}

#[test]
fn test_prune_dry_run_no_data() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output =
        wake_cmd().args(["prune", "--dry-run"]).env("HOME", temp_dir.path()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No sessions older than"));
}

#[test]
fn test_prune_with_force() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Create database with old data directly
    let db_dir = home.join(".wake");
    std::fs::create_dir_all(&db_dir).unwrap();
    let conn = rusqlite::Connection::open(db_dir.join("wake.db")).unwrap();
    conn.execute_batch(include_str!("../../wake-core/src/schema.sql")).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, started_at) VALUES ('old', '2020-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO commands (session_id, started_at, command) VALUES ('old', '2020-01-01T00:00:01Z', 'echo test')",
        [],
    )
    .unwrap();
    drop(conn);

    let output = wake_cmd()
        .args(["prune", "--force", "--older-than", "30"])
        .env("HOME", home)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Deleted 1 session"));
}

#[test]
fn test_config_file_is_read() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Create config file
    let wake_dir = home.join(".wake");
    std::fs::create_dir_all(&wake_dir).unwrap();
    std::fs::write(
        wake_dir.join("config.toml"),
        r#"
[retention]
days = 7

[output]
max_mb = 10
"#,
    )
    .unwrap();

    // Run a command that uses the config (prune --dry-run)
    // This tests that config loading works without errors
    let output = wake_cmd().args(["prune", "--dry-run"]).env("HOME", home).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should use 7 days from config
    assert!(stdout.contains("7 days"), "Expected config to use 7 days retention");
}

// Tests for LLM commands (Phase 4)

#[test]
fn test_llm_help() {
    let output = wake_cmd().args(["llm", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("download"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("Manage local LLM") || stdout.contains("summarization"));
}

#[test]
fn test_llm_status() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = wake_cmd().args(["llm", "status"]).env("HOME", temp_dir.path()).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("LLM Summarization Status"));
    assert!(stdout.contains("Model path:"));
    assert!(stdout.contains("Status:"));
}

#[test]
fn test_llm_status_shows_feature_status() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = wake_cmd().args(["llm", "status"]).env("HOME", temp_dir.path()).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Feature:"));
    // Should show either enabled or disabled
    assert!(stdout.contains("enabled") || stdout.contains("disabled"));
}

#[test]
fn test_llm_download_help() {
    let output = wake_cmd().args(["llm", "download", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Download") || stdout.contains("download"));
}

#[test]
fn test_llm_status_help() {
    let output = wake_cmd().args(["llm", "status", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status") || stdout.contains("Status"));
}

#[test]
fn test_summarization_config_is_read() {
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path();

    // Create config with summarization settings
    let wake_dir = home.join(".wake");
    std::fs::create_dir_all(&wake_dir).unwrap();
    std::fs::write(
        wake_dir.join("config.toml"),
        r#"
[summarization]
enabled = true
min_bytes = 2048
"#,
    )
    .unwrap();

    // Run a command - should not crash with summarization config
    let output = wake_cmd().args(["status"]).env("HOME", home).output().unwrap();

    assert!(output.status.success());
}
