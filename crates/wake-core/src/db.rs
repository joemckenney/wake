use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Failed to determine data directory")]
    NoDataDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub project_root: Option<String>,
    pub shell: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: i64,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub command: String,
    pub working_dir: Option<String>,
    pub git_branch: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub output: Option<String>,
    pub output_bytes: Option<i64>,
    pub truncated: bool,
    pub summary: Option<String>,
}

/// Lightweight command metadata without full output (for tiered retrieval)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    pub id: i64,
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub output_bytes: Option<i64>,
    pub truncated: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: i64,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub note: String,
}

/// Statistics about data to be pruned
#[derive(Debug, Clone, Default)]
pub struct PruneStats {
    pub sessions: usize,
    pub commands: usize,
    pub annotations: usize,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open() -> Result<Self, DbError> {
        let path = Self::db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn db_path() -> Result<PathBuf, DbError> {
        dirs::home_dir().map(|h| h.join(".wake").join("wake.db")).ok_or(DbError::NoDataDir)
    }

    fn migrate(&self) -> Result<(), DbError> {
        self.conn.execute_batch(include_str!("schema.sql"))?;

        // Add summary column (v0.4.0+)
        let _ = self.conn.execute("ALTER TABLE commands ADD COLUMN summary TEXT", []); // Ignore error if column already exists

        Ok(())
    }

    // Session operations

    pub fn create_session(
        &self,
        id: &str,
        shell: Option<&str>,
        project_root: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (id, started_at, shell, project_root, metadata) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, now, shell, project_root, metadata],
        )?;
        Ok(())
    }

    pub fn end_session(&self, id: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute("UPDATE sessions SET ended_at = ?1 WHERE id = ?2", params![now, id])?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, project_root, shell, metadata FROM sessions WHERE id = ?1",
        )?;
        let session = stmt
            .query_row(params![id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    started_at: parse_datetime(row.get::<_, String>(1)?),
                    ended_at: row.get::<_, Option<String>>(2)?.map(parse_datetime),
                    project_root: row.get(3)?,
                    shell: row.get(4)?,
                    metadata: row.get(5)?,
                })
            })
            .optional()?;
        Ok(session)
    }

    pub fn get_most_recent_session(&self) -> Result<Option<Session>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, project_root, shell, metadata FROM sessions ORDER BY started_at DESC LIMIT 1",
        )?;
        let session = stmt
            .query_row([], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    started_at: parse_datetime(row.get::<_, String>(1)?),
                    ended_at: row.get::<_, Option<String>>(2)?.map(parse_datetime),
                    project_root: row.get(3)?,
                    shell: row.get(4)?,
                    metadata: row.get(5)?,
                })
            })
            .optional()?;
        Ok(session)
    }

    // Command operations

    pub fn insert_command(
        &self,
        session_id: &str,
        command: &str,
        working_dir: Option<&str>,
        git_branch: Option<&str>,
    ) -> Result<i64, DbError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO commands (session_id, started_at, command, working_dir, git_branch) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, now, command, working_dir, git_branch],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_command(
        &self,
        id: i64,
        exit_code: i32,
        duration_ms: i64,
        output: &str,
        output_raw: &[u8],
        truncated: bool,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let output_bytes = output_raw.len() as i64;
        self.conn.execute(
            "UPDATE commands SET ended_at = ?1, exit_code = ?2, duration_ms = ?3, output = ?4, output_raw = ?5, output_bytes = ?6, truncated = ?7 WHERE id = ?8",
            params![now, exit_code, duration_ms, output, output_raw, output_bytes, truncated, id],
        )?;
        Ok(())
    }

    pub fn get_recent_commands(&self, limit: usize) -> Result<Vec<Command>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, started_at, ended_at, command, working_dir, git_branch, exit_code, duration_ms, output, output_bytes, truncated, summary
             FROM commands ORDER BY started_at DESC LIMIT ?1",
        )?;
        let commands = stmt
            .query_map(params![limit as i64], |row| {
                Ok(Command {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: parse_datetime(row.get::<_, String>(2)?),
                    ended_at: row.get::<_, Option<String>>(3)?.map(parse_datetime),
                    command: row.get(4)?,
                    working_dir: row.get(5)?,
                    git_branch: row.get(6)?,
                    exit_code: row.get(7)?,
                    duration_ms: row.get(8)?,
                    output: row.get(9)?,
                    output_bytes: row.get(10)?,
                    truncated: row.get(11)?,
                    summary: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(commands)
    }

    pub fn get_session_commands(&self, session_id: &str) -> Result<Vec<Command>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, started_at, ended_at, command, working_dir, git_branch, exit_code, duration_ms, output, output_bytes, truncated, summary
             FROM commands WHERE session_id = ?1 ORDER BY started_at ASC",
        )?;
        let commands = stmt
            .query_map(params![session_id], |row| {
                Ok(Command {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: parse_datetime(row.get::<_, String>(2)?),
                    ended_at: row.get::<_, Option<String>>(3)?.map(parse_datetime),
                    command: row.get(4)?,
                    working_dir: row.get(5)?,
                    git_branch: row.get(6)?,
                    exit_code: row.get(7)?,
                    duration_ms: row.get(8)?,
                    output: row.get(9)?,
                    output_bytes: row.get(10)?,
                    truncated: row.get(11)?,
                    summary: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(commands)
    }

    pub fn search_commands(&self, query: &str) -> Result<Vec<Command>, DbError> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, started_at, ended_at, command, working_dir, git_branch, exit_code, duration_ms, output, output_bytes, truncated, summary
             FROM commands WHERE command LIKE ?1 OR output LIKE ?1 ORDER BY started_at DESC LIMIT 50",
        )?;
        let commands = stmt
            .query_map(params![pattern], |row| {
                Ok(Command {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: parse_datetime(row.get::<_, String>(2)?),
                    ended_at: row.get::<_, Option<String>>(3)?.map(parse_datetime),
                    command: row.get(4)?,
                    working_dir: row.get(5)?,
                    git_branch: row.get(6)?,
                    exit_code: row.get(7)?,
                    duration_ms: row.get(8)?,
                    output: row.get(9)?,
                    output_bytes: row.get(10)?,
                    truncated: row.get(11)?,
                    summary: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(commands)
    }

    pub fn count_session_commands(&self, session_id: &str) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get recent command metadata (without full output) for tiered retrieval
    pub fn get_recent_commands_metadata(
        &self,
        limit: usize,
    ) -> Result<Vec<CommandMetadata>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, started_at, command, exit_code, duration_ms, output_bytes, truncated, summary
             FROM commands ORDER BY started_at DESC LIMIT ?1",
        )?;
        let commands = stmt
            .query_map(params![limit as i64], |row| {
                Ok(CommandMetadata {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: parse_datetime(row.get::<_, String>(2)?),
                    command: row.get(3)?,
                    exit_code: row.get(4)?,
                    duration_ms: row.get(5)?,
                    output_bytes: row.get(6)?,
                    truncated: row.get(7)?,
                    summary: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(commands)
    }

    /// Get full output for a specific command by ID
    pub fn get_command_output(&self, id: i64) -> Result<Option<String>, DbError> {
        let output: Option<String> = self
            .conn
            .query_row("SELECT output FROM commands WHERE id = ?1", params![id], |row| row.get(0))
            .optional()?
            .flatten();
        Ok(output)
    }

    /// Update the summary for a command
    pub fn update_command_summary(&self, id: i64, summary: &str) -> Result<(), DbError> {
        self.conn
            .execute("UPDATE commands SET summary = ?1 WHERE id = ?2", params![summary, id])?;
        Ok(())
    }

    // Annotation operations

    pub fn insert_annotation(&self, session_id: &str, note: &str) -> Result<i64, DbError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO annotations (session_id, timestamp, note) VALUES (?1, ?2, ?3)",
            params![session_id, now, note],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_session_annotations(&self, session_id: &str) -> Result<Vec<Annotation>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, timestamp, note FROM annotations WHERE session_id = ?1 ORDER BY timestamp ASC",
        )?;
        let annotations = stmt
            .query_map(params![session_id], |row| {
                Ok(Annotation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    timestamp: parse_datetime(row.get::<_, String>(2)?),
                    note: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(annotations)
    }

    // Prune operations

    /// Preview what would be deleted without actually deleting
    pub fn preview_prune(&self, retention_days: u32) -> Result<PruneStats, DbError> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let sessions: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE started_at < ?1",
            params![cutoff_str],
            |row| row.get(0),
        )?;

        let commands: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commands WHERE session_id IN (SELECT id FROM sessions WHERE started_at < ?1)",
            params![cutoff_str],
            |row| row.get(0),
        )?;

        let annotations: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM annotations WHERE session_id IN (SELECT id FROM sessions WHERE started_at < ?1)",
            params![cutoff_str],
            |row| row.get(0),
        )?;

        Ok(PruneStats {
            sessions: sessions as usize,
            commands: commands as usize,
            annotations: annotations as usize,
        })
    }

    /// Delete sessions older than retention_days and cascade to commands/annotations
    pub fn prune_old_sessions(&self, retention_days: u32) -> Result<PruneStats, DbError> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        // Get stats first
        let stats = self.preview_prune(retention_days)?;

        // Delete in order: annotations, commands, sessions (respecting FK relationships)
        self.conn.execute(
            "DELETE FROM annotations WHERE session_id IN (SELECT id FROM sessions WHERE started_at < ?1)",
            params![cutoff_str],
        )?;

        self.conn.execute(
            "DELETE FROM commands WHERE session_id IN (SELECT id FROM sessions WHERE started_at < ?1)",
            params![cutoff_str],
        )?;

        self.conn.execute("DELETE FROM sessions WHERE started_at < ?1", params![cutoff_str])?;

        Ok(stats)
    }

    /// Run VACUUM to reclaim disk space after deletion
    pub fn vacuum(&self) -> Result<(), DbError> {
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }
}

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)).unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        let db = Database { conn };
        db.migrate().unwrap();
        db
    }

    #[test]
    fn test_create_and_get_session() {
        let db = temp_db();

        db.create_session("test-123", Some("zsh"), Some("/home/user/project"), None).unwrap();

        let session = db.get_session("test-123").unwrap().unwrap();
        assert_eq!(session.id, "test-123");
        assert_eq!(session.shell, Some("zsh".to_string()));
        assert_eq!(session.project_root, Some("/home/user/project".to_string()));
        assert!(session.ended_at.is_none());
    }

    #[test]
    fn test_end_session() {
        let db = temp_db();

        db.create_session("test-456", None, None, None).unwrap();
        db.end_session("test-456").unwrap();

        let session = db.get_session("test-456").unwrap().unwrap();
        assert!(session.ended_at.is_some());
    }

    #[test]
    fn test_most_recent_session() {
        let db = temp_db();

        db.create_session("first", None, None, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.create_session("second", None, None, None).unwrap();

        let recent = db.get_most_recent_session().unwrap().unwrap();
        assert_eq!(recent.id, "second");
    }

    #[test]
    fn test_insert_and_finish_command() {
        let db = temp_db();

        db.create_session("sess-1", None, None, None).unwrap();

        let cmd_id = db.insert_command("sess-1", "ls -la", Some("/home"), Some("main")).unwrap();

        db.finish_command(cmd_id, 0, 150, "file1\nfile2\n", b"file1\nfile2\n", false).unwrap();

        let commands = db.get_session_commands("sess-1").unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "ls -la");
        assert_eq!(commands[0].exit_code, Some(0));
        assert_eq!(commands[0].duration_ms, Some(150));
        assert_eq!(commands[0].output, Some("file1\nfile2\n".to_string()));
        assert_eq!(commands[0].git_branch, Some("main".to_string()));
    }

    #[test]
    fn test_recent_commands() {
        let db = temp_db();

        db.create_session("sess-2", None, None, None).unwrap();

        for i in 0..5 {
            let id = db.insert_command("sess-2", &format!("cmd{i}"), None, None).unwrap();
            db.finish_command(id, 0, 100, "", b"", false).unwrap();
        }

        let recent = db.get_recent_commands(3).unwrap();
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].command, "cmd4");
        assert_eq!(recent[1].command, "cmd3");
        assert_eq!(recent[2].command, "cmd2");
    }

    #[test]
    fn test_search_commands() {
        let db = temp_db();

        db.create_session("sess-3", None, None, None).unwrap();

        let id1 = db.insert_command("sess-3", "cargo build", None, None).unwrap();
        db.finish_command(id1, 0, 100, "Compiling...", b"", false).unwrap();

        let id2 = db.insert_command("sess-3", "cargo test", None, None).unwrap();
        db.finish_command(id2, 0, 100, "running 5 tests", b"", false).unwrap();

        let id3 = db.insert_command("sess-3", "git status", None, None).unwrap();
        db.finish_command(id3, 0, 100, "nothing to commit", b"", false).unwrap();

        // Search by command
        let results = db.search_commands("cargo").unwrap();
        assert_eq!(results.len(), 2);

        // Search by output
        let results = db.search_commands("tests").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "cargo test");
    }

    #[test]
    fn test_count_session_commands() {
        let db = temp_db();

        db.create_session("sess-4", None, None, None).unwrap();

        for _ in 0..3 {
            let id = db.insert_command("sess-4", "echo hi", None, None).unwrap();
            db.finish_command(id, 0, 10, "hi", b"hi", false).unwrap();
        }

        let count = db.count_session_commands("sess-4").unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_annotations() {
        let db = temp_db();

        db.create_session("sess-5", None, None, None).unwrap();

        db.insert_annotation("sess-5", "Starting feature work").unwrap();
        db.insert_annotation("sess-5", "Bug found in auth module").unwrap();

        let annotations = db.get_session_annotations("sess-5").unwrap();
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].note, "Starting feature work");
        assert_eq!(annotations[1].note, "Bug found in auth module");
    }

    #[test]
    fn test_nonexistent_session() {
        let db = temp_db();

        let session = db.get_session("does-not-exist").unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn test_prune_old_sessions() {
        let db = temp_db();

        // Create old session (manually set old timestamp)
        db.conn
            .execute(
                "INSERT INTO sessions (id, started_at) VALUES (?1, ?2)",
                params!["old-session", "2020-01-01T00:00:00Z"],
            )
            .unwrap();

        // Create recent session
        db.create_session("new-session", None, None, None).unwrap();

        // Add commands to both
        db.conn
            .execute(
                "INSERT INTO commands (session_id, started_at, command) VALUES (?1, ?2, ?3)",
                params!["old-session", "2020-01-01T00:00:01Z", "old cmd"],
            )
            .unwrap();
        db.insert_command("new-session", "new cmd", None, None).unwrap();

        // Prune with 30 day retention
        let stats = db.prune_old_sessions(30).unwrap();

        assert_eq!(stats.sessions, 1);
        assert_eq!(stats.commands, 1);

        // Verify old session is gone
        assert!(db.get_session("old-session").unwrap().is_none());

        // Verify new session remains
        assert!(db.get_session("new-session").unwrap().is_some());
    }

    #[test]
    fn test_preview_prune() {
        let db = temp_db();

        // Create old session
        db.conn
            .execute(
                "INSERT INTO sessions (id, started_at) VALUES (?1, ?2)",
                params!["old-session", "2020-01-01T00:00:00Z"],
            )
            .unwrap();

        // Preview should show 1 session
        let stats = db.preview_prune(30).unwrap();
        assert_eq!(stats.sessions, 1);

        // But session should still exist
        assert!(db.get_session("old-session").unwrap().is_some());
    }

    #[test]
    fn test_prune_cascades_to_annotations() {
        let db = temp_db();

        // Create old session with annotation
        db.conn
            .execute(
                "INSERT INTO sessions (id, started_at) VALUES (?1, ?2)",
                params!["old-session", "2020-01-01T00:00:00Z"],
            )
            .unwrap();
        db.insert_annotation("old-session", "old note").unwrap();

        let stats = db.prune_old_sessions(30).unwrap();

        assert_eq!(stats.annotations, 1);

        // Verify annotation is deleted (and session is gone)
        assert!(db.get_session("old-session").unwrap().is_none());
    }

    #[test]
    fn test_prune_nothing_to_delete() {
        let db = temp_db();

        // Create only recent session
        db.create_session("new-session", None, None, None).unwrap();

        let stats = db.prune_old_sessions(30).unwrap();

        assert_eq!(stats.sessions, 0);
        assert_eq!(stats.commands, 0);
        assert_eq!(stats.annotations, 0);

        // Session should still exist
        assert!(db.get_session("new-session").unwrap().is_some());
    }

    // Tests for tiered retrieval (Phase 1)

    #[test]
    fn test_get_recent_commands_metadata() {
        let db = temp_db();

        db.create_session("sess-meta", None, None, None).unwrap();

        // Create commands with varying output sizes
        let id1 = db.insert_command("sess-meta", "ls", None, None).unwrap();
        db.finish_command(id1, 0, 50, "file1.txt", b"file1.txt", false).unwrap();

        let id2 = db.insert_command("sess-meta", "cat large.txt", None, None).unwrap();
        let large_output = "x".repeat(10000);
        db.finish_command(id2, 0, 100, &large_output, large_output.as_bytes(), true).unwrap();

        let id3 = db.insert_command("sess-meta", "echo hello", None, None).unwrap();
        db.finish_command(id3, 0, 10, "hello", b"hello", false).unwrap();

        let metadata = db.get_recent_commands_metadata(10).unwrap();
        assert_eq!(metadata.len(), 3);

        // Most recent first
        assert_eq!(metadata[0].command, "echo hello");
        assert_eq!(metadata[0].exit_code, Some(0));
        assert_eq!(metadata[0].output_bytes, Some(5)); // "hello".len()
        assert!(!metadata[0].truncated);

        // Second command had truncated output
        assert_eq!(metadata[1].command, "cat large.txt");
        assert!(metadata[1].truncated);
        assert_eq!(metadata[1].output_bytes, Some(10000));

        // First command
        assert_eq!(metadata[2].command, "ls");
    }

    #[test]
    fn test_get_recent_commands_metadata_limit() {
        let db = temp_db();

        db.create_session("sess-limit", None, None, None).unwrap();

        for i in 0..10 {
            let id = db.insert_command("sess-limit", &format!("cmd{i}"), None, None).unwrap();
            db.finish_command(id, 0, 10, "", b"", false).unwrap();
        }

        let metadata = db.get_recent_commands_metadata(3).unwrap();
        assert_eq!(metadata.len(), 3);
        assert_eq!(metadata[0].command, "cmd9");
        assert_eq!(metadata[1].command, "cmd8");
        assert_eq!(metadata[2].command, "cmd7");
    }

    #[test]
    fn test_get_recent_commands_metadata_empty() {
        let db = temp_db();

        let metadata = db.get_recent_commands_metadata(10).unwrap();
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_get_command_output() {
        let db = temp_db();

        db.create_session("sess-output", None, None, None).unwrap();

        let id = db.insert_command("sess-output", "echo test", None, None).unwrap();
        db.finish_command(id, 0, 10, "test output here", b"test output here", false).unwrap();

        let output = db.get_command_output(id).unwrap();
        assert_eq!(output, Some("test output here".to_string()));
    }

    #[test]
    fn test_get_command_output_empty() {
        let db = temp_db();

        db.create_session("sess-empty", None, None, None).unwrap();

        let id = db.insert_command("sess-empty", "true", None, None).unwrap();
        db.finish_command(id, 0, 5, "", b"", false).unwrap();

        let output = db.get_command_output(id).unwrap();
        assert_eq!(output, Some("".to_string()));
    }

    #[test]
    fn test_get_command_output_nonexistent() {
        let db = temp_db();

        let output = db.get_command_output(99999).unwrap();
        assert!(output.is_none());
    }

    #[test]
    fn test_get_command_output_large() {
        let db = temp_db();

        db.create_session("sess-large", None, None, None).unwrap();

        let large_output = "line\n".repeat(10000);
        let id = db.insert_command("sess-large", "generate", None, None).unwrap();
        db.finish_command(id, 0, 1000, &large_output, large_output.as_bytes(), false).unwrap();

        let output = db.get_command_output(id).unwrap();
        assert_eq!(output.unwrap().len(), large_output.len());
    }

    #[test]
    fn test_update_command_summary() {
        let db = temp_db();

        db.create_session("sess-summary", None, None, None).unwrap();

        let id = db.insert_command("sess-summary", "cargo build", None, None).unwrap();
        db.finish_command(id, 0, 5000, "Compiling...\nFinished.", b"", false).unwrap();

        // Initially no summary
        let commands = db.get_recent_commands(1).unwrap();
        assert!(commands[0].summary.is_none());

        // Update summary
        db.update_command_summary(id, "Build completed successfully with no errors.").unwrap();

        // Verify summary is stored
        let commands = db.get_recent_commands(1).unwrap();
        assert_eq!(
            commands[0].summary,
            Some("Build completed successfully with no errors.".to_string())
        );
    }

    #[test]
    fn test_update_command_summary_in_metadata() {
        let db = temp_db();

        db.create_session("sess-sum-meta", None, None, None).unwrap();

        let id = db.insert_command("sess-sum-meta", "npm test", None, None).unwrap();
        db.finish_command(id, 0, 3000, "Tests passed", b"", false).unwrap();

        db.update_command_summary(id, "All 42 tests passed.").unwrap();

        // Verify summary appears in metadata
        let metadata = db.get_recent_commands_metadata(1).unwrap();
        assert_eq!(metadata[0].summary, Some("All 42 tests passed.".to_string()));
    }

    #[test]
    fn test_update_command_summary_overwrite() {
        let db = temp_db();

        db.create_session("sess-overwrite", None, None, None).unwrap();

        let id = db.insert_command("sess-overwrite", "test", None, None).unwrap();
        db.finish_command(id, 0, 100, "output", b"output", false).unwrap();

        // Set initial summary
        db.update_command_summary(id, "First summary").unwrap();

        // Overwrite with new summary
        db.update_command_summary(id, "Updated summary").unwrap();

        let commands = db.get_recent_commands(1).unwrap();
        assert_eq!(commands[0].summary, Some("Updated summary".to_string()));
    }

    #[test]
    fn test_summary_field_in_command_struct() {
        let db = temp_db();

        db.create_session("sess-struct", None, None, None).unwrap();

        let id = db.insert_command("sess-struct", "ls -la", None, None).unwrap();
        db.finish_command(id, 0, 50, "total 100\ndrwxr-xr-x", b"", false).unwrap();

        db.update_command_summary(id, "Listed 100 items in directory.").unwrap();

        // Test get_recent_commands includes summary
        let commands = db.get_recent_commands(1).unwrap();
        assert_eq!(commands[0].summary, Some("Listed 100 items in directory.".to_string()));

        // Test get_session_commands includes summary
        let session_commands = db.get_session_commands("sess-struct").unwrap();
        assert_eq!(session_commands[0].summary, Some("Listed 100 items in directory.".to_string()));

        // Test search_commands includes summary
        let search_results = db.search_commands("ls").unwrap();
        assert_eq!(search_results[0].summary, Some("Listed 100 items in directory.".to_string()));
    }

    #[test]
    fn test_commands_without_summary() {
        let db = temp_db();

        db.create_session("sess-nosummary", None, None, None).unwrap();

        let id = db.insert_command("sess-nosummary", "pwd", None, None).unwrap();
        db.finish_command(id, 0, 10, "/home/user", b"/home/user", false).unwrap();

        // Command without summary should have None
        let commands = db.get_recent_commands(1).unwrap();
        assert!(commands[0].summary.is_none());

        let metadata = db.get_recent_commands_metadata(1).unwrap();
        assert!(metadata[0].summary.is_none());
    }

    #[test]
    fn test_mixed_commands_with_and_without_summaries() {
        let db = temp_db();

        db.create_session("sess-mixed", None, None, None).unwrap();

        // Command without summary
        let id1 = db.insert_command("sess-mixed", "cd /tmp", None, None).unwrap();
        db.finish_command(id1, 0, 5, "", b"", false).unwrap();

        // Command with summary
        let id2 = db.insert_command("sess-mixed", "cargo build", None, None).unwrap();
        db.finish_command(id2, 0, 5000, "Compiling...", b"", false).unwrap();
        db.update_command_summary(id2, "Build succeeded.").unwrap();

        // Command without summary
        let id3 = db.insert_command("sess-mixed", "ls", None, None).unwrap();
        db.finish_command(id3, 0, 20, "file.txt", b"", false).unwrap();

        let metadata = db.get_recent_commands_metadata(10).unwrap();
        assert_eq!(metadata.len(), 3);

        // Most recent first
        assert!(metadata[0].summary.is_none()); // ls
        assert_eq!(metadata[1].summary, Some("Build succeeded.".to_string())); // cargo build
        assert!(metadata[2].summary.is_none()); // cd
    }
}
