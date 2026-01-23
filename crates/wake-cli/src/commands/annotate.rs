use anyhow::{Context, Result};
use wake_core::Database;

pub async fn run(note: &str) -> Result<()> {
    let session_id = std::env::var("WAKE_SESSION")
        .context("WAKE_SESSION not set - are you inside a wake shell?")?;

    let db = Database::open()?;
    db.insert_annotation(&session_id, note)?;

    println!("Annotation added.");
    Ok(())
}
