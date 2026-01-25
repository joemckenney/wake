//! Model loading and inference
//!
//! This module provides model loading and inference when the `llm` feature is enabled.
//! Without the feature, stub implementations are provided.

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Failed to load model: {0}")]
    LoadError(String),
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Model path does not exist: {0}")]
    PathNotFound(String),
    #[error("LLM feature not enabled")]
    FeatureNotEnabled,
}

/// Model status
#[derive(Debug, Clone, PartialEq)]
pub enum ModelStatus {
    NotDownloaded,
    Downloaded,
    Loaded,
}

/// Loaded model ready for inference
#[cfg(feature = "llm")]
pub struct Model {
    inner: mistralrs::Model,
}

#[cfg(not(feature = "llm"))]
pub struct Model {
    _path: std::path::PathBuf,
}

impl Model {
    /// Load a GGUF model from disk
    #[cfg(feature = "llm")]
    pub async fn load(model_path: &Path) -> Result<Self, ModelError> {
        use mistralrs::GgufModelBuilder;

        if !model_path.exists() {
            return Err(ModelError::PathNotFound(model_path.display().to_string()));
        }

        // Get the parent directory and filename for the new API
        let parent = model_path.parent().unwrap_or(Path::new("."));
        let filename = model_path
            .file_name()
            .ok_or_else(|| ModelError::LoadError("Invalid model path".to_string()))?
            .to_string_lossy()
            .to_string();

        let inner = GgufModelBuilder::new(parent.display().to_string(), vec![filename])
            .with_logging()
            .build()
            .await
            .map_err(|e| ModelError::LoadError(e.to_string()))?;

        Ok(Self { inner })
    }

    #[cfg(not(feature = "llm"))]
    pub async fn load(model_path: &Path) -> Result<Self, ModelError> {
        if !model_path.exists() {
            return Err(ModelError::PathNotFound(model_path.display().to_string()));
        }
        // Return stub when feature not enabled
        Ok(Self { _path: model_path.to_path_buf() })
    }

    /// Run inference on the model with the given messages
    #[cfg(feature = "llm")]
    pub async fn generate(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, ModelError> {
        use mistralrs::TextMessages;

        let messages = TextMessages::new()
            .add_message(mistralrs::TextMessageRole::System, system_prompt)
            .add_message(mistralrs::TextMessageRole::User, user_message);

        let response = self
            .inner
            .send_chat_request(messages)
            .await
            .map_err(|e| ModelError::InferenceError(e.to_string()))?;

        // Extract the response text (content is now Option<String>)
        let content =
            response.choices.first().and_then(|c| c.message.content.clone()).unwrap_or_default();

        Ok(content.trim().to_string())
    }

    #[cfg(not(feature = "llm"))]
    pub async fn generate(
        &self,
        _system_prompt: &str,
        _user_message: &str,
    ) -> Result<String, ModelError> {
        Err(ModelError::FeatureNotEnabled)
    }
}
