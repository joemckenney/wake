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
