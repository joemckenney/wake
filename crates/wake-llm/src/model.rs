//! Model loading and inference using llama.cpp
//!
//! This module provides model loading and text generation using the llama-cpp-2 bindings.

use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Failed to load model: {0}")]
    LoadError(String),
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Model path does not exist: {0}")]
    PathNotFound(String),
    #[error("Backend initialization failed: {0}")]
    BackendError(String),
}

/// Model status
#[derive(Debug, Clone, PartialEq)]
pub enum ModelStatus {
    NotDownloaded,
    Downloaded,
    Loaded,
}

/// Loaded model ready for inference
pub struct Model {
    model: Arc<llama_cpp_2::llama_backend::LlamaBackend>,
    llama_model: llama_cpp_2::model::LlamaModel,
}

// Model needs to be Send + Sync for async usage
unsafe impl Send for Model {}
unsafe impl Sync for Model {}

impl Model {
    /// Load a GGUF model from disk
    pub fn load(model_path: &Path) -> Result<Self, ModelError> {
        use llama_cpp_2::llama_backend::LlamaBackend;
        use llama_cpp_2::model::params::LlamaModelParams;
        use llama_cpp_2::model::LlamaModel;

        if !model_path.exists() {
            return Err(ModelError::PathNotFound(model_path.display().to_string()));
        }

        // Initialize the llama.cpp backend with logging disabled
        let mut backend =
            LlamaBackend::init().map_err(|e| ModelError::BackendError(e.to_string()))?;
        backend.void_logs();

        // Configure model parameters
        let model_params = LlamaModelParams::default();

        // Load the model
        let llama_model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| ModelError::LoadError(e.to_string()))?;

        Ok(Self { model: Arc::new(backend), llama_model })
    }

    /// Run inference on the model with the given messages
    pub fn generate(&self, system_prompt: &str, user_message: &str) -> Result<String, ModelError> {
        use llama_cpp_2::context::params::LlamaContextParams;
        use llama_cpp_2::llama_batch::LlamaBatch;
        use llama_cpp_2::sampling::LlamaSampler;
        use std::num::NonZeroU32;

        // Build the prompt in chat format
        let prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system_prompt, user_message
        );

        // Create context parameters
        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(2048));

        // Create context from model
        let mut ctx = self
            .llama_model
            .new_context(&self.model, ctx_params)
            .map_err(|e| ModelError::InferenceError(e.to_string()))?;

        // Tokenize the prompt
        let tokens = self
            .llama_model
            .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
            .map_err(|e| ModelError::InferenceError(e.to_string()))?;

        // Create batch and add tokens
        let mut batch = LlamaBatch::new(2048, 1);
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| ModelError::InferenceError(e.to_string()))?;
        }

        // Process the prompt
        ctx.decode(&mut batch).map_err(|e| ModelError::InferenceError(e.to_string()))?;

        // Set up sampler for generation
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(42),
        ]);

        // Generate tokens
        let mut output = String::new();
        let max_tokens = 256;
        let mut n_cur = tokens.len() as i32;

        for _ in 0..max_tokens {
            // Sample next token
            let token = sampler.sample(&ctx, -1);

            // Check for end of generation
            if self.llama_model.is_eog_token(token) {
                break;
            }

            // Convert token to text
            let piece = self
                .llama_model
                .token_to_str(token, llama_cpp_2::model::Special::Tokenize)
                .map_err(|e| ModelError::InferenceError(e.to_string()))?;

            // Check for end-of-turn marker
            if piece.contains("<|im_end|>") {
                break;
            }

            output.push_str(&piece);

            // Prepare next batch
            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| ModelError::InferenceError(e.to_string()))?;
            n_cur += 1;

            // Decode
            ctx.decode(&mut batch).map_err(|e| ModelError::InferenceError(e.to_string()))?;
        }

        Ok(output.trim().to_string())
    }
}
