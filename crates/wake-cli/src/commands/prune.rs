use anyhow::Result;
use std::io::{self, Write};
use wake_core::{Config, Database};

pub struct PruneOptions {
    pub dry_run: bool,
    pub force: bool,
    pub older_than_days: Option<u32>,
}

pub async fn run(options: PruneOptions) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let retention_days = options.older_than_days.unwrap_or(config.retention.days);

    let db = Database::open()?;

    // Preview what will be deleted
    let stats = db.preview_prune(retention_days)?;

    if stats.sessions == 0 {
        println!("No sessions older than {retention_days} days to delete.");
        return Ok(());
    }

    println!("Found {} session(s) older than {} days:", stats.sessions, retention_days);
    println!("  - {} command(s)", stats.commands);
    println!("  - {} annotation(s)", stats.annotations);

    if options.dry_run {
        println!("\n[Dry run - no data deleted]");
        return Ok(());
    }

    // Confirm unless --force
    if !options.force {
        print!("\nDelete this data? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Perform deletion
    let deleted = db.prune_old_sessions(retention_days)?;
    println!(
        "\nDeleted {} session(s), {} command(s), {} annotation(s)",
        deleted.sessions, deleted.commands, deleted.annotations
    );

    // Vacuum to reclaim space
    db.vacuum()?;

    Ok(())
}
