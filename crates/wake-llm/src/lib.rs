//! Local LLM summarization for wake terminal recorder
//!
//! This crate provides command output summarization using a local Phi-3 Mini model.
//! The model is downloaded on first use from HuggingFace.
//!
//! # Features
//!
//! - `llm` - Enable actual LLM inference (requires mistralrs dependencies)
//! - `cuda` - Enable CUDA support for GPU acceleration
//! - `metal` - Enable Metal support for Apple Silicon
//!
//! Without the `llm` feature, only model downloading is available.

mod download;
mod model;
mod summarize;

pub use download::DownloadProgress;
pub use model::ModelStatus;
pub use summarize::SummarizeError;

use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Default model to use for summarization
pub const DEFAULT_MODEL: &str = "Phi-3-mini-4k-instruct-q4.gguf";
pub const DEFAULT_MODEL_URL: &str = "https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf/resolve/main/Phi-3-mini-4k-instruct-q4.gguf";
pub const DEFAULT_MODEL_SIZE: u64 = 2_300_000_000; // ~2.3GB

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
    #[error("LLM feature not enabled - compile with --features llm")]
    FeatureNotEnabled,
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

    /// Check if LLM inference is available (feature enabled)
    pub fn llm_enabled() -> bool {
        cfg!(feature = "llm")
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
        Ok(())
    }

    /// Download the model without progress (for simple use)
    pub async fn download_model(&self) -> Result<(), LlmError> {
        self.ensure_model(|_| {}).await
    }

    /// Load the model into memory (lazy - called automatically by summarize if needed)
    pub async fn load_model(&self) -> Result<(), LlmError> {
        if !Self::llm_enabled() {
            return Err(LlmError::FeatureNotEnabled);
        }

        if !self.model_available() {
            return Err(LlmError::ModelNotAvailable(
                "Model not downloaded. Call ensure_model() first.".to_string(),
            ));
        }

        let mut model_guard = self.model.write().await;
        if model_guard.is_none() {
            let loaded = model::Model::load(&self.model_path).await?;
            *model_guard = Some(loaded);
        }
        Ok(())
    }

    /// Summarize command output
    ///
    /// If the model is not loaded, it will be loaded first.
    /// If the model is not downloaded, returns an error.
    pub async fn summarize(&self, command: &str, output: &str) -> Result<String, LlmError> {
        if !Self::llm_enabled() {
            return Err(LlmError::FeatureNotEnabled);
        }

        // Ensure model is loaded
        self.load_model().await?;

        let model_guard = self.model.read().await;
        let model = model_guard.as_ref().ok_or_else(|| {
            LlmError::ModelNotAvailable("Model failed to load".to_string())
        })?;

        let summary = summarize::summarize(model, command, output).await?;
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
