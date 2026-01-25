//! Model download with progress tracking

use futures_util::StreamExt;
use reqwest::Client;
use std::path::Path;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Download failed: {0}")]
    Failed(String),
}

/// Progress information during download
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<f64>,
}

impl DownloadProgress {
    fn new(downloaded: u64, total: Option<u64>) -> Self {
        let percent = total.map(|t| (downloaded as f64 / t as f64) * 100.0);
        Self { downloaded, total, percent }
    }
}

/// Download the model from a URL to a file path with progress callback
pub async fn download_model<F>(
    url: &str,
    dest: &Path,
    progress_callback: F,
) -> Result<(), DownloadError>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    let client = Client::builder()
        .user_agent("wake-llm/0.1")
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(DownloadError::Failed(format!(
            "HTTP {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown error")
        )));
    }

    let total_size = response.content_length();

    // Create temp file for download
    let temp_path = dest.with_extension("tmp");
    let mut file = File::create(&temp_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        progress_callback(DownloadProgress::new(downloaded, total_size));
    }

    file.flush().await?;
    drop(file);

    // Rename temp file to final destination
    tokio::fs::rename(&temp_path, dest).await?;

    Ok(())
}

/// Check if a partial download exists and get its size
pub async fn partial_download_size(dest: &Path) -> Option<u64> {
    let temp_path = dest.with_extension("tmp");
    tokio::fs::metadata(&temp_path).await.ok().map(|m| m.len())
}

/// Remove a partial download if it exists
pub async fn remove_partial_download(dest: &Path) -> Result<(), DownloadError> {
    let temp_path = dest.with_extension("tmp");
    if temp_path.exists() {
        tokio::fs::remove_file(&temp_path).await?;
    }
    Ok(())
}
