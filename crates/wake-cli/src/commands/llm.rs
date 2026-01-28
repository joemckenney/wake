//! LLM model management commands

use anyhow::Result;
use wake_llm::{WakeLlm, DEFAULT_MODEL_SIZE};

/// Show LLM model status
pub async fn status() -> Result<()> {
    let llm = WakeLlm::new();
    let status = llm.status().await;

    println!("LLM Summarization Status");
    println!("========================");
    println!();

    // Model info
    println!("Model:      Qwen2.5-0.5B-Instruct (Q4_K_M)");
    println!("Model path: {}", llm.model_path().display());

    // Model status
    let status_str = match status {
        wake_llm::ModelStatus::NotDownloaded => "not downloaded",
        wake_llm::ModelStatus::Downloaded => "downloaded",
        wake_llm::ModelStatus::Loaded => "loaded in memory",
    };
    println!("Status:     {}", status_str);

    // If downloaded, show file size
    if llm.model_available() {
        if let Ok(metadata) = std::fs::metadata(llm.model_path()) {
            let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
            println!("Size:       {:.1} MB", size_mb);
        }
    } else {
        let expected_mb = DEFAULT_MODEL_SIZE as f64 / (1024.0 * 1024.0);
        println!("Expected:   ~{:.0} MB", expected_mb);
    }

    Ok(())
}

/// Download the LLM model
pub async fn download() -> Result<()> {
    let llm = WakeLlm::new();

    if llm.model_available() {
        println!("Model already downloaded at: {}", llm.model_path().display());
        return Ok(());
    }

    println!("Downloading summarization model (Qwen2.5-0.5B)...");
    println!("Destination: {}", llm.model_path().display());
    println!();

    // Create a progress indicator using indicatif
    use indicatif::{ProgressBar, ProgressStyle};

    let pb = ProgressBar::new(DEFAULT_MODEL_SIZE);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Download with progress
    let pb_clone = pb.clone();
    llm.ensure_model(move |progress| {
        pb_clone.set_position(progress.downloaded);
        if let Some(total) = progress.total {
            pb_clone.set_length(total);
        }
    })
    .await?;

    pb.finish_with_message("Download complete!");
    println!();
    println!("Model downloaded successfully.");
    println!();
    println!("Summarization is enabled by default. To disable, add to ~/.wake/config.toml:");
    println!();
    println!("[summarization]");
    println!("enabled = false");

    Ok(())
}
