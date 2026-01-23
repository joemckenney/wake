use anyhow::Result;
use wake_core::Database;

pub async fn run() -> Result<()> {
    let db = Database::open()?;

    // Get current session or most recent
    let session_id = std::env::var("WAKE_SESSION").ok();
    let session = if let Some(ref id) = session_id {
        db.get_session(id)?
    } else {
        db.get_most_recent_session()?
    };

    let session = match session {
        Some(s) => s,
        None => {
            eprintln!("No sessions found.");
            return Ok(());
        }
    };

    let commands = db.get_session_commands(&session.id)?;
    let annotations = db.get_session_annotations(&session.id)?;

    // Output markdown
    println!("# Terminal Session Context\n");

    println!("**Session ID:** {}", session.id);
    println!("**Started:** {}", session.started_at.format("%Y-%m-%d %H:%M:%S UTC"));
    if let Some(ended) = session.ended_at {
        println!("**Ended:** {}", ended.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(shell) = &session.shell {
        println!("**Shell:** {shell}");
    }
    if let Some(root) = &session.project_root {
        println!("**Project Root:** {root}");
    }
    println!();

    if !annotations.is_empty() {
        println!("## Annotations\n");
        for note in &annotations {
            println!("- **{}:** {}", note.timestamp.format("%H:%M:%S"), note.note);
        }
        println!();
    }

    println!("## Commands\n");

    for cmd in &commands {
        let exit_status = match cmd.exit_code {
            Some(0) => "✓".to_string(),
            Some(code) => format!("✗ (exit {code})"),
            None => "?".to_string(),
        };

        let branch = cmd.git_branch.as_ref().map(|b| format!(" [{b}]")).unwrap_or_default();

        let dir = cmd.working_dir.as_ref().map(|d| format!(" in `{d}`")).unwrap_or_default();

        println!("### `{}`{branch} {exit_status}\n", cmd.command);

        if !dir.is_empty() || cmd.duration_ms.is_some() {
            let duration = cmd.duration_ms.map(|ms| format!("{}ms", ms)).unwrap_or_default();
            println!("_{dir} {duration}_\n");
        }

        if let Some(output) = &cmd.output {
            if !output.trim().is_empty() {
                println!("```");
                // Limit output to avoid overwhelming context
                let lines: Vec<_> = output.lines().collect();
                if lines.len() > 50 {
                    for line in &lines[..25] {
                        println!("{line}");
                    }
                    println!("... ({} lines omitted) ...", lines.len() - 50);
                    for line in &lines[lines.len() - 25..] {
                        println!("{line}");
                    }
                } else {
                    for line in &lines {
                        println!("{line}");
                    }
                }
                println!("```");

                if cmd.truncated {
                    println!("*Output was truncated*");
                }
            }
        }

        println!();
    }

    Ok(())
}
