//! LLM model management commands

use anyhow::Result;
use std::fs;
use wake_core::ModelManifest;
use wake_llm::{WakeLlm, DEFAULT_MODEL, DEFAULT_MODEL_SIZE};

/// Show LLM model status
pub async fn status() -> Result<()> {
    let llm = WakeLlm::new();
    let status = llm.status().await;

    println!("LLM Summarization Status");
    println!("========================");
    println!();

    // Model info
    println!("Model:      Qwen3-0.6B (Q4_K_M)");
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

    println!("Downloading summarization model (Qwen3-0.6B)...");
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

/// Represents a model file that can be cleaned
struct CleanableModel {
    filename: String,
    path: std::path::PathBuf,
    size: u64,
    reason: CleanReason,
}

enum CleanReason {
    /// Model is tracked in manifest but not the current default model
    OrphanedModel,
    /// Model file exists but is not tracked in manifest
    UntrackedFile,
}

impl CleanableModel {
    fn reason_str(&self) -> &'static str {
        match self.reason {
            CleanReason::OrphanedModel => "orphaned (from previous version)",
            CleanReason::UntrackedFile => "untracked .gguf file",
        }
    }
}

/// Clean orphaned models from previous versions
pub async fn clean(dry_run: bool, force: bool) -> Result<()> {
    let models_dir = ModelManifest::models_dir()?;

    if !models_dir.exists() {
        println!("No models directory found at: {}", models_dir.display());
        return Ok(());
    }

    // Load manifest
    let mut manifest = ModelManifest::load()?;

    // Find all .gguf files in models directory
    let mut cleanable: Vec<CleanableModel> = Vec::new();

    for entry in fs::read_dir(&models_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip non-files and non-gguf files
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // Skip the manifest file itself
        if filename == "manifest.toml" {
            continue;
        }

        // Skip the current default model
        if filename == DEFAULT_MODEL {
            continue;
        }

        // Check if it's a .gguf file
        if !filename.ends_with(".gguf") {
            continue;
        }

        let size = entry.metadata()?.len();

        let reason = if manifest.has_model(&filename) {
            CleanReason::OrphanedModel
        } else {
            CleanReason::UntrackedFile
        };

        cleanable.push(CleanableModel { filename, path, size, reason });
    }

    if cleanable.is_empty() {
        println!("No orphaned models found. Models directory is clean.");
        return Ok(());
    }

    // Calculate total size
    let total_size: u64 = cleanable.iter().map(|m| m.size).sum();
    let total_mb = total_size as f64 / (1024.0 * 1024.0);

    // Show what will be deleted
    println!("Found {} orphaned model(s):", cleanable.len());
    println!();

    for model in &cleanable {
        let size_mb = model.size as f64 / (1024.0 * 1024.0);
        println!("  {} ({:.1} MB) - {}", model.filename, size_mb, model.reason_str());
    }

    println!();
    println!("Total: {:.1} MB to be freed", total_mb);
    println!();

    if dry_run {
        println!("Dry run: No files were deleted.");
        return Ok(());
    }

    // Confirm deletion unless --force
    if !force {
        println!("Delete these files? [y/N] ");
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;

        if !line.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Delete files and update manifest
    let mut deleted_count = 0;
    let mut deleted_size: u64 = 0;

    for model in &cleanable {
        match fs::remove_file(&model.path) {
            Ok(()) => {
                // Remove from manifest if tracked
                manifest.remove_model(&model.filename);
                deleted_count += 1;
                deleted_size += model.size;
                println!("Deleted: {}", model.filename);
            }
            Err(e) => {
                eprintln!("Failed to delete {}: {}", model.filename, e);
            }
        }
    }

    // Save updated manifest
    manifest.save()?;

    println!();
    println!(
        "Cleaned {} file(s), freed {:.1} MB",
        deleted_count,
        deleted_size as f64 / (1024.0 * 1024.0)
    );

    Ok(())
}
