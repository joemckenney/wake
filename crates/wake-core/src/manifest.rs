use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("Could not determine home directory")]
    NoHomeDir,
    #[error("Failed to read manifest file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse manifest file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Failed to serialize manifest: {0}")]
    SerializeError(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub filename: String,
    pub wake_version: String,
    pub sha256: String,
    pub downloaded_at: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelManifest {
    #[serde(default)]
    pub models: HashMap<String, ModelEntry>,
}

impl ModelManifest {
    /// Load manifest from ~/.wake/models/manifest.toml
    pub fn load() -> Result<Self, ManifestError> {
        let path = Self::manifest_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let manifest: ModelManifest = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Save manifest to ~/.wake/models/manifest.toml
    pub fn save(&self) -> Result<(), ManifestError> {
        let path = Self::manifest_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Get the path to the manifest file
    pub fn manifest_path() -> Result<PathBuf, ManifestError> {
        dirs::home_dir()
            .map(|h| h.join(".wake").join("models").join("manifest.toml"))
            .ok_or(ManifestError::NoHomeDir)
    }

    /// Get the models directory path
    pub fn models_dir() -> Result<PathBuf, ManifestError> {
        dirs::home_dir()
            .map(|h| h.join(".wake").join("models"))
            .ok_or(ManifestError::NoHomeDir)
    }

    /// Add or update a model entry
    pub fn add_model(&mut self, entry: ModelEntry) {
        self.models.insert(entry.filename.clone(), entry);
    }

    /// Remove a model entry by filename
    pub fn remove_model(&mut self, filename: &str) -> Option<ModelEntry> {
        self.models.remove(filename)
    }

    /// Check if a model exists in the manifest
    pub fn has_model(&self, filename: &str) -> bool {
        self.models.contains_key(filename)
    }

    /// Get a model entry by filename
    pub fn get_model(&self, filename: &str) -> Option<&ModelEntry> {
        self.models.get(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_entry_serialization() {
        let entry = ModelEntry {
            filename: "test-model.gguf".to_string(),
            wake_version: "0.5.0".to_string(),
            sha256: "abc123".to_string(),
            downloaded_at: "2024-01-01T00:00:00Z".to_string(),
            url: "https://example.com/model.gguf".to_string(),
        };

        let mut manifest = ModelManifest::default();
        manifest.add_model(entry.clone());

        let toml_str = toml::to_string_pretty(&manifest).unwrap();
        let parsed: ModelManifest = toml::from_str(&toml_str).unwrap();

        assert!(parsed.has_model("test-model.gguf"));
        let parsed_entry = parsed.get_model("test-model.gguf").unwrap();
        assert_eq!(parsed_entry.filename, entry.filename);
        assert_eq!(parsed_entry.wake_version, entry.wake_version);
        assert_eq!(parsed_entry.sha256, entry.sha256);
    }

    #[test]
    fn test_manifest_add_remove() {
        let mut manifest = ModelManifest::default();

        let entry = ModelEntry {
            filename: "model.gguf".to_string(),
            wake_version: "0.5.0".to_string(),
            sha256: "hash".to_string(),
            downloaded_at: "2024-01-01T00:00:00Z".to_string(),
            url: "https://example.com/model.gguf".to_string(),
        };

        manifest.add_model(entry);
        assert!(manifest.has_model("model.gguf"));

        let removed = manifest.remove_model("model.gguf");
        assert!(removed.is_some());
        assert!(!manifest.has_model("model.gguf"));
    }

    #[test]
    fn test_empty_manifest_parse() {
        let toml_str = "";
        let manifest: ModelManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.models.is_empty());
    }

    #[test]
    fn test_manifest_with_models_parse() {
        let toml_str = r#"
[models.test-model]
filename = "test-model.gguf"
wake_version = "0.5.0"
sha256 = "abc123"
downloaded_at = "2024-01-01T00:00:00Z"
url = "https://example.com/model.gguf"
"#;
        let manifest: ModelManifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.has_model("test-model"));
    }
}
