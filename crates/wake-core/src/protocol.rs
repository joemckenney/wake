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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_start_serialization() {
        let msg = HookMessage::CmdStart {
            cmd: "ls -la".to_string(),
            cwd: PathBuf::from("/home/user"),
            timestamp: DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"cmd_start\""));
        assert!(json.contains("\"cmd\":\"ls -la\""));

        let parsed: HookMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event(), HookEvent::CmdStart);
    }

    #[test]
    fn test_cmd_end_serialization() {
        let msg = HookMessage::CmdEnd { exit_code: 0, duration_ms: 1500, timestamp: Utc::now() };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"cmd_end\""));
        assert!(json.contains("\"exit_code\":0"));
        assert!(json.contains("\"duration_ms\":1500"));

        let parsed: HookMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event(), HookEvent::CmdEnd);
    }

    #[test]
    fn test_event_accessor() {
        let start = HookMessage::CmdStart {
            cmd: "test".to_string(),
            cwd: PathBuf::from("."),
            timestamp: Utc::now(),
        };
        assert_eq!(start.event(), HookEvent::CmdStart);

        let end = HookMessage::CmdEnd { exit_code: 1, duration_ms: 100, timestamp: Utc::now() };
        assert_eq!(end.event(), HookEvent::CmdEnd);
    }
}
