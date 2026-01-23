use anyhow::Result;
use wake_core::Database;

pub async fn run(count: usize) -> Result<()> {
    let db = Database::open()?;
    let commands = db.get_recent_commands(count)?;

    if commands.is_empty() {
        println!("No commands recorded yet.");
        println!("Run 'wake shell' to start a recorded session.");
        return Ok(());
    }

    for cmd in commands.iter().rev() {
        let exit_indicator = match cmd.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "?",
        };

        let time = cmd.started_at.format("%H:%M:%S");
        let duration = cmd.duration_ms.map(|ms| format!(" ({ms}ms)")).unwrap_or_default();

        println!("{exit_indicator} [{time}]{duration} {}", cmd.command);

        // Show truncated output preview
        if let Some(output) = &cmd.output {
            let preview: String = output
                .lines()
                .take(3)
                .map(|l| format!("  │ {}", truncate_line(l, 72)))
                .collect::<Vec<_>>()
                .join("\n");

            if !preview.is_empty() {
                println!("{preview}");
                let line_count = output.lines().count();
                if line_count > 3 {
                    println!("  │ ... ({} more lines)", line_count - 3);
                }
            }
        }

        if cmd.truncated {
            println!("  │ [output truncated]");
        }

        println!();
    }

    Ok(())
}

fn truncate_line(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
