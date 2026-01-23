use anyhow::Result;
use wake_core::Database;

pub async fn run() -> Result<()> {
    let db = Database::open()?;

    // Check if we're in an active session
    if let Ok(session_id) = std::env::var("WAKE_SESSION") {
        if let Some(session) = db.get_session(&session_id)? {
            let cmd_count = db.count_session_commands(&session_id)?;
            println!("Active session: {}", session.id);
            println!("  Started: {}", session.started_at.format("%Y-%m-%d %H:%M:%S"));
            if let Some(shell) = &session.shell {
                println!("  Shell: {shell}");
            }
            if let Some(root) = &session.project_root {
                println!("  Project: {root}");
            }
            println!("  Commands: {cmd_count}");
            return Ok(());
        }
    }

    // Show most recent session
    if let Some(session) = db.get_most_recent_session()? {
        let cmd_count = db.count_session_commands(&session.id)?;
        let status = if session.ended_at.is_some() { "ended" } else { "active" };

        println!("Most recent session: {} ({status})", session.id);
        println!("  Started: {}", session.started_at.format("%Y-%m-%d %H:%M:%S"));
        if let Some(ended) = session.ended_at {
            println!("  Ended: {}", ended.format("%Y-%m-%d %H:%M:%S"));
        }
        if let Some(shell) = &session.shell {
            println!("  Shell: {shell}");
        }
        if let Some(root) = &session.project_root {
            println!("  Project: {root}");
        }
        println!("  Commands: {cmd_count}");
    } else {
        println!("No sessions recorded yet.");
        println!("Run 'wake shell' to start a recorded session.");
    }

    Ok(())
}
