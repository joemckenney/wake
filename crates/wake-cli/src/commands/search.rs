use anyhow::Result;
use wake_core::Database;

pub async fn run(query: &str) -> Result<()> {
    let db = Database::open()?;
    let commands = db.search_commands(query)?;

    if commands.is_empty() {
        println!("No commands matching '{query}' found.");
        return Ok(());
    }

    println!("Found {} command(s) matching '{query}':\n", commands.len());

    for cmd in &commands {
        let exit_indicator = match cmd.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "?",
        };

        let time = cmd.started_at.format("%Y-%m-%d %H:%M:%S");

        println!("{exit_indicator} [{time}] {}", cmd.command);

        if let Some(dir) = &cmd.working_dir {
            println!("  dir: {dir}");
        }

        // Show matching output context
        if let Some(output) = &cmd.output {
            let matching_lines: Vec<_> = output
                .lines()
                .enumerate()
                .filter(|(_, line)| line.to_lowercase().contains(&query.to_lowercase()))
                .take(3)
                .collect();

            for (line_num, line) in matching_lines {
                let truncated =
                    if line.len() > 80 { format!("{}...", &line[..80]) } else { line.to_string() };
                println!("  {}: {truncated}", line_num + 1);
            }
        }

        println!();
    }

    Ok(())
}
