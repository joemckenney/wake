use anyhow::{Context, Result};
use chrono::Utc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use wake_core::{Config, Database, GitCache, HookMessage, OutputBuffer, SummarizationConfig};

/// Get the user's shell, trying multiple sources:
/// 1. SHELL environment variable
/// 2. /etc/passwd entry for current user
/// 3. Fall back to /bin/sh
fn get_user_shell() -> String {
    // Try SHELL env var first
    if let Ok(shell) = std::env::var("SHELL") {
        return shell;
    }

    // Try to read from /etc/passwd
    if let Some(shell) = get_shell_from_passwd() {
        return shell;
    }

    // Final fallback
    "/bin/sh".to_string()
}

/// Read the current user's shell from /etc/passwd
fn get_shell_from_passwd() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let file = std::fs::File::open("/etc/passwd").ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().map_while(Result::ok) {
        // Format: username:password:uid:gid:gecos:home:shell
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() >= 7 {
            if let Ok(entry_uid) = fields[2].parse::<u32>() {
                if entry_uid == uid {
                    let shell = fields[6].trim();
                    if !shell.is_empty() {
                        return Some(shell.to_string());
                    }
                }
            }
        }
    }

    None
}

struct PendingCommand {
    id: i64,
    command: String,
    started_at: chrono::DateTime<Utc>,
    buffer: OutputBuffer,
}

/// Info about a completed command to be summarized
struct SummarizeRequest {
    command_id: i64,
    command: String,
    output: String,
}

struct SessionState {
    session_id: String,
    db: Database,
    git_cache: GitCache,
    current_command: Option<PendingCommand>,
    max_output_bytes: usize,
    summarization_config: SummarizationConfig,
    summarize_tx: Option<mpsc::Sender<SummarizeRequest>>,
}

pub async fn run() -> Result<()> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let socket_path = format!("/tmp/wake-{session_id}.sock");

    // Clean up any stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Get user's shell (env var -> /etc/passwd -> /bin/sh fallback)
    let shell = get_user_shell();

    // Detect project root
    let cwd = std::env::current_dir().ok();
    let mut git_cache = GitCache::new();
    let project_root =
        cwd.as_ref().and_then(|p| git_cache.get(p).root).map(|p| p.to_string_lossy().to_string());

    // Initialize database and create session
    let db = Database::open().context("Failed to open database")?;

    // Load config for retention and output settings
    let config = Config::load().unwrap_or_default();

    // Auto-cleanup old sessions (silent, non-blocking)
    let _ = db.prune_old_sessions(config.retention.days);

    let max_output_bytes = config.output.max_bytes();
    let summarization_config = config.summarization.clone();

    // Set up background summarization if enabled
    let summarize_tx = if summarization_config.enabled {
        let (tx, rx) = mpsc::channel::<SummarizeRequest>(100);
        tokio::spawn(run_summarization_task(rx));
        Some(tx)
    } else {
        None
    };

    db.create_session(&session_id, Some(&shell), project_root.as_deref(), None)
        .context("Failed to create session")?;

    // Create PTY
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .context("Failed to open PTY")?;

    // Build command with environment variables
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("WAKE_SESSION", &session_id);
    cmd.env("WAKE_SOCKET", &socket_path);
    if let Some(cwd) = &cwd {
        cmd.cwd(cwd);
    }

    // Spawn child process
    let mut child = pty_pair.slave.spawn_command(cmd).context("Failed to spawn shell")?;

    // Get PTY master for I/O
    let master = pty_pair.master;
    let mut reader = master.try_clone_reader().context("Failed to clone PTY reader")?;
    let mut writer = master.take_writer().context("Failed to take PTY writer")?;

    // Set up terminal raw mode (only if stdin is a TTY)
    let _raw_guard = RawModeGuard::new();

    // Resize PTY to match terminal
    if let Some((cols, rows)) = term_size::dimensions() {
        let _ = master.resize(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    // Create Unix socket for hooks
    let listener = UnixListener::bind(&socket_path).context("Failed to bind Unix socket")?;
    listener.set_nonblocking(true).context("Failed to set socket non-blocking")?;

    // Shared state
    let state = Arc::new(Mutex::new(SessionState {
        session_id: session_id.clone(),
        db,
        git_cache,
        current_command: None,
        max_output_bytes,
        summarization_config,
        summarize_tx,
    }));

    // Channel for coordinating shutdown
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    // Spawn thread to handle stdin -> PTY
    let _stdin_handle = {
        let shutdown_tx = shutdown_tx.clone();
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if writer.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = shutdown_tx.blocking_send(());
        })
    };

    // Spawn thread to handle PTY -> stdout (and buffer output)
    let _pty_handle = {
        let state = Arc::clone(&state);
        let shutdown_tx = shutdown_tx.clone();
        std::thread::spawn(move || {
            let mut stdout = std::io::stdout();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = &buf[..n];
                        // Write to stdout
                        if stdout.write_all(data).is_err() {
                            break;
                        }
                        let _ = stdout.flush();

                        // Buffer for current command
                        if let Ok(mut state) = state.lock() {
                            if let Some(ref mut pending) = state.current_command {
                                pending.buffer.append(data);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = shutdown_tx.blocking_send(());
        })
    };

    // Handle socket connections in a separate thread
    let _socket_thread = {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let mut buf = String::new();
                        if stream.read_to_string(&mut buf).is_ok() {
                            if let Ok(msg) = serde_json::from_str::<HookMessage>(&buf) {
                                handle_hook_message(&state, msg);
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        })
    };

    // Wait for child to exit
    let exit_status = child.wait()?;
    drop(shutdown_tx);

    // Wait for shutdown signal
    let _ = shutdown_rx.recv().await;

    // Finalize any pending command
    {
        let mut state = state.lock().unwrap();
        if let Some(pending) = state.current_command.take() {
            let output_result = pending.buffer.finish();
            let duration_ms = (Utc::now() - pending.started_at).num_milliseconds();
            let _ = state.db.finish_command(
                pending.id,
                exit_status.exit_code() as i32,
                duration_ms,
                &output_result.clean,
                &output_result.raw,
                output_result.truncated,
            );
        }
        let _ = state.db.end_session(&state.session_id);
    }

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);

    std::process::exit(exit_status.exit_code() as i32);
}

fn handle_hook_message(state_arc: &Arc<Mutex<SessionState>>, msg: HookMessage) {
    let mut state = match state_arc.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    match msg {
        HookMessage::CmdStart { cmd, cwd, timestamp } => {
            // Finalize any previous command that wasn't properly ended
            if let Some(pending) = state.current_command.take() {
                let output_result = pending.buffer.finish();
                let duration_ms = (Utc::now() - pending.started_at).num_milliseconds();
                let _ = state.db.finish_command(
                    pending.id,
                    -1, // Unknown exit code
                    duration_ms,
                    &output_result.clean,
                    &output_result.raw,
                    output_result.truncated,
                );
            }

            // Get git info for this directory
            let git_info = state.git_cache.get(&cwd);

            // Insert new command
            let cmd_id = state.db.insert_command(
                &state.session_id,
                &cmd,
                Some(cwd.to_string_lossy().as_ref()),
                git_info.branch.as_deref(),
            );

            if let Ok(id) = cmd_id {
                state.current_command = Some(PendingCommand {
                    id,
                    command: cmd.clone(),
                    started_at: timestamp,
                    buffer: OutputBuffer::with_max_bytes(state.max_output_bytes),
                });
            }
        }
        HookMessage::CmdEnd { exit_code, duration_ms, timestamp: _ } => {
            // Small delay to let PTY reader catch up with fast commands
            // The shell hook fires synchronously but PTY output is async
            // We must delay BEFORE taking the pending command so the buffer can still receive data
            drop(state);
            std::thread::sleep(std::time::Duration::from_millis(100));
            let mut state2 = match state_arc.lock() {
                Ok(s) => s,
                Err(_) => return,
            };

            if let Some(pending) = state2.current_command.take() {
                let output_result = pending.buffer.finish();
                let _ = state2.db.finish_command(
                    pending.id,
                    exit_code,
                    duration_ms as i64,
                    &output_result.clean,
                    &output_result.raw,
                    output_result.truncated,
                );

                // Queue for summarization if enabled and output is large enough
                if state2.summarization_config.enabled
                    && output_result.clean.len() >= state2.summarization_config.min_bytes_usize()
                {
                    if let Some(ref tx) = state2.summarize_tx {
                        let _ = tx.try_send(SummarizeRequest {
                            command_id: pending.id,
                            command: pending.command,
                            output: output_result.clean,
                        });
                    }
                }
            }
        }
    }
}

/// Background task that handles LLM summarization of command outputs
async fn run_summarization_task(mut rx: mpsc::Receiver<SummarizeRequest>) {
    use wake_llm::WakeLlm;

    let llm = WakeLlm::new();

    // Try to ensure model is available (download if needed)
    if !llm.model_available() {
        tracing::info!("Summarization model not downloaded, attempting download...");
        match llm.download_model().await {
            Ok(_) => tracing::info!("Model downloaded successfully"),
            Err(e) => {
                tracing::warn!("Failed to download model: {e}. Summarization disabled.");
                // Drain the channel without processing
                while rx.recv().await.is_some() {}
                return;
            }
        }
    }

    // Open database connection for this task
    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to open database for summarization: {e}");
            return;
        }
    };

    // Process summarization requests
    while let Some(request) = rx.recv().await {
        match llm.summarize(&request.command, &request.output).await {
            Ok(summary) => {
                if let Err(e) = db.update_command_summary(request.command_id, &summary) {
                    tracing::warn!(
                        "Failed to save summary for command {}: {e}",
                        request.command_id
                    );
                }
            }
            Err(e) => {
                tracing::debug!("Failed to summarize command {}: {e}", request.command_id);
            }
        }
    }
}

struct RawModeGuard {
    original: Option<libc::termios>,
}

impl RawModeGuard {
    fn new() -> Self {
        unsafe {
            // Check if stdin is a TTY
            if libc::isatty(libc::STDIN_FILENO) == 0 {
                return Self { original: None };
            }

            let mut original: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut original) != 0 {
                return Self { original: None };
            }

            let mut raw = original;
            libc::cfmakeraw(&mut raw);

            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) != 0 {
                return Self { original: None };
            }

            Self { original: Some(original) }
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if let Some(ref original) = self.original {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_shell_from_passwd_returns_valid_shell() {
        // This test runs as the current user, so it should find a shell
        let shell = get_shell_from_passwd();
        assert!(shell.is_some(), "Should find current user's shell in /etc/passwd");
        let shell = shell.unwrap();
        assert!(shell.starts_with('/'), "Shell should be an absolute path");
        assert!(
            shell.contains("sh") || shell.contains("zsh") || shell.contains("fish"),
            "Shell should be a known shell type: {}",
            shell
        );
    }

    #[test]
    fn test_get_user_shell_returns_shell() {
        let shell = get_user_shell();
        assert!(shell.starts_with('/'), "Shell should be an absolute path");
        assert!(!shell.is_empty(), "Shell should not be empty");
    }

    #[test]
    fn test_get_user_shell_prefers_env_var() {
        // Save original
        let original = std::env::var("SHELL").ok();

        // Set a custom SHELL
        std::env::set_var("SHELL", "/bin/custom-shell");
        let shell = get_user_shell();
        assert_eq!(shell, "/bin/custom-shell");

        // Restore original
        match original {
            Some(val) => std::env::set_var("SHELL", val),
            None => std::env::remove_var("SHELL"),
        }
    }

    #[test]
    fn test_get_user_shell_falls_back_to_passwd() {
        // Save original
        let original = std::env::var("SHELL").ok();

        // Remove SHELL env var
        std::env::remove_var("SHELL");

        let shell = get_user_shell();
        // Should get shell from passwd, not /bin/sh (unless that's actually the user's shell)
        assert!(shell.starts_with('/'), "Shell should be an absolute path");

        // Restore original
        if let Some(val) = original {
            std::env::set_var("SHELL", val);
        }
    }
}
