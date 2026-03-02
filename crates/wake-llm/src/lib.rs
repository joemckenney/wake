#![allow(deprecated)]
//! Local LLM summarization for wake terminal recorder
//!
//! This crate provides command output summarization using a local Qwen3-0.6B model
//! via llama.cpp. The model is downloaded on first use from HuggingFace.
//!
//! # Features
//!
//! - `cuda` - Enable CUDA support for GPU acceleration
//! - `metal` - Enable Metal support for Apple Silicon
//!
//! LLM inference is always available - the small model runs efficiently on CPU.

mod download;
mod model;
mod summarize;

pub use download::DownloadProgress;
pub use model::ModelStatus;
pub use summarize::SummarizeError;

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use wake_core::{ModelEntry, ModelManifest};

/// Default model to use for summarization (Qwen3-0.6B - small and fast)
pub const DEFAULT_MODEL: &str = "Qwen3-0.6B-Q4_K_M.gguf";
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf";
pub const DEFAULT_MODEL_SIZE: u64 = 397_000_000; // ~397MB

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),
    #[error("Download error: {0}")]
    Download(#[from] download::DownloadError),
    #[error("Model error: {0}")]
    Model(#[from] model::ModelError),
    #[error("Summarization error: {0}")]
    Summarize(#[from] summarize::SummarizeError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Manifest error: {0}")]
    Manifest(#[from] wake_core::ManifestError),
}

/// Compute SHA256 hash of a file
pub fn compute_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Main interface for LLM operations
pub struct WakeLlm {
    model_path: PathBuf,
    model_url: String,
    model: Arc<RwLock<Option<model::Model>>>,
}

impl WakeLlm {
    /// Create a new WakeLlm instance with default model settings
    pub fn new() -> Self {
        let model_dir = dirs::home_dir()
            .map(|h| h.join(".wake").join("models"))
            .unwrap_or_else(|| PathBuf::from(".wake/models"));

        Self {
            model_path: model_dir.join(DEFAULT_MODEL),
            model_url: DEFAULT_MODEL_URL.to_string(),
            model: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if the model file exists on disk
    pub fn model_available(&self) -> bool {
        self.model_path.exists()
    }

    /// Get the path where the model is/will be stored
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }

    /// Get the expected model size in bytes
    pub fn model_size(&self) -> u64 {
        DEFAULT_MODEL_SIZE
    }

    /// Get the current model status
    pub async fn status(&self) -> ModelStatus {
        if !self.model_available() {
            return ModelStatus::NotDownloaded;
        }

        let model = self.model.read().await;
        if model.is_some() {
            ModelStatus::Loaded
        } else {
            ModelStatus::Downloaded
        }
    }

    /// Ensure the model is downloaded, with progress callback
    pub async fn ensure_model<F>(&self, progress_callback: F) -> Result<(), LlmError>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        if self.model_available() {
            return Ok(());
        }

        // Create models directory
        if let Some(parent) = self.model_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        download::download_model(&self.model_url, &self.model_path, progress_callback).await?;

        // Record in manifest
        self.record_in_manifest()?;

        Ok(())
    }

    /// Record the current model in the manifest
    fn record_in_manifest(&self) -> Result<(), LlmError> {
        let mut manifest = ModelManifest::load()?;

        let sha256 = compute_sha256(&self.model_path)?;
        let downloaded_at = chrono::Utc::now().to_rfc3339();

        let entry = ModelEntry {
            filename: DEFAULT_MODEL.to_string(),
            wake_version: env!("CARGO_PKG_VERSION").to_string(),
            sha256,
            downloaded_at,
            url: self.model_url.clone(),
        };

        manifest.add_model(entry);
        manifest.save()?;

        Ok(())
    }

    /// Download the model without progress (for simple use)
    pub async fn download_model(&self) -> Result<(), LlmError> {
        self.ensure_model(|_| {}).await
    }

    /// Load the model into memory (lazy - called automatically by summarize if needed)
    pub async fn load_model(&self) -> Result<(), LlmError> {
        if !self.model_available() {
            return Err(LlmError::ModelNotAvailable(
                "Model not downloaded. Call ensure_model() first.".to_string(),
            ));
        }

        let mut model_guard = self.model.write().await;
        if model_guard.is_none() {
            let model_path = self.model_path.clone();
            // Load model in blocking task since llama.cpp is synchronous
            let loaded = tokio::task::spawn_blocking(move || model::Model::load(&model_path))
                .await
                .map_err(|e| LlmError::ModelNotAvailable(e.to_string()))??;
            *model_guard = Some(loaded);
        }
        Ok(())
    }

    /// Summarize command output
    ///
    /// If the model is not loaded, it will be loaded first.
    /// If the model is not downloaded, returns an error.
    pub async fn summarize(&self, command: &str, output: &str) -> Result<String, LlmError> {
        // Ensure model is loaded
        self.load_model().await?;

        let model_guard = self.model.read().await;
        let model = model_guard
            .as_ref()
            .ok_or_else(|| LlmError::ModelNotAvailable("Model failed to load".to_string()))?;

        let summary = summarize::summarize(model, command, output)?;
        Ok(summary)
    }

    /// Unload the model from memory to free resources
    pub async fn unload_model(&self) {
        let mut model_guard = self.model.write().await;
        *model_guard = None;
    }
}

impl Default for WakeLlm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_status_not_downloaded() {
        let llm = WakeLlm {
            model_path: PathBuf::from("/nonexistent/path/model.gguf"),
            model_url: DEFAULT_MODEL_URL.to_string(),
            model: Arc::new(RwLock::new(None)),
        };
        let status = llm.status().await;
        assert_eq!(status, ModelStatus::NotDownloaded);
    }

    #[tokio::test]
    async fn test_load_model_without_download() {
        let llm = WakeLlm {
            model_path: PathBuf::from("/nonexistent/model.gguf"),
            model_url: DEFAULT_MODEL_URL.to_string(),
            model: Arc::new(RwLock::new(None)),
        };

        let result = llm.load_model().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_summarize_without_model() {
        let llm = WakeLlm {
            model_path: PathBuf::from("/nonexistent/model.gguf"),
            model_url: DEFAULT_MODEL_URL.to_string(),
            model: Arc::new(RwLock::new(None)),
        };

        let result = llm.summarize("echo hello", "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unload_model() {
        let llm = WakeLlm::new();
        llm.unload_model().await;
    }
}
