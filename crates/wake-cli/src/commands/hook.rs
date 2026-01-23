use anyhow::{Context, Result};
use chrono::Utc;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use wake_core::HookMessage;

pub async fn cmd_start(cmd: &str) -> Result<()> {
    let socket_path = std::env::var("WAKE_SOCKET")
        .context("WAKE_SOCKET not set - are you inside a wake shell?")?;

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let msg = HookMessage::CmdStart { cmd: cmd.to_string(), cwd, timestamp: Utc::now() };

    send_message(&socket_path, &msg)?;
    Ok(())
}

pub async fn cmd_end(exit_code: i32, duration: u64) -> Result<()> {
    let socket_path = std::env::var("WAKE_SOCKET")
        .context("WAKE_SOCKET not set - are you inside a wake shell?")?;

    let msg = HookMessage::CmdEnd {
        exit_code,
        duration_ms: duration * 1000, // SECONDS to ms
        timestamp: Utc::now(),
    };

    send_message(&socket_path, &msg)?;
    Ok(())
}

fn send_message(socket_path: &str, msg: &HookMessage) -> Result<()> {
    let mut stream =
        UnixStream::connect(socket_path).context("Failed to connect to wake socket")?;

    let json = serde_json::to_string(msg)?;
    stream.write_all(json.as_bytes())?;
    stream.flush()?;

    Ok(())
}
