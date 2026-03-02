use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HookMessage {
    CmdStart { cmd: String, cwd: PathBuf, timestamp: DateTime<Utc> },
    CmdEnd { exit_code: i32, duration_ms: u64, timestamp: DateTime<Utc> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    CmdStart,
    CmdEnd,
}

impl HookMessage {
    pub fn event(&self) -> HookEvent {
        match self {
            HookMessage::CmdStart { .. } => HookEvent::CmdStart,
            HookMessage::CmdEnd { .. } => HookEvent::CmdEnd,
        }
    }
}