use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Could not determine home directory")]
    NoHomeDir,
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub summarization: SummarizationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    /// Number of days to retain data (default: 21)
    #[serde(default = "default_retention_days")]
    pub days: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self { days: default_retention_days() }
    }
}

fn default_retention_days() -> u32 {
    21
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    /// Max output size in MB (default: 5)
    #[serde(default = "default_max_output_mb")]
    pub max_mb: u32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { max_mb: default_max_output_mb() }
    }
}

fn default_max_output_mb() -> u32 {
    5
}

impl OutputConfig {
    /// Returns max output size in bytes
    pub fn max_bytes(&self) -> usize {
        self.max_mb as usize * 1024 * 1024
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SummarizationConfig {
    /// Enable LLM summarization of command outputs (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Minimum output size in bytes to trigger summarization (default: 1024)
    #[serde(default = "default_min_bytes")]
    pub min_bytes: u32,
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_bytes: default_min_bytes(),
        }
    }
}

fn default_min_bytes() -> u32 {
    1024
}

impl SummarizationConfig {
    /// Returns min output size in bytes as usize
    pub fn min_bytes_usize(&self) -> usize {
        self.min_bytes as usize
    }
}

impl Config {
    /// Load config with precedence: env var > config file > defaults
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Self::load_from_file().unwrap_or_default();

        // Environment variable overrides
        if let Ok(days_str) = std::env::var("WAKE_RETENTION_DAYS") {
            if let Ok(days) = days_str.parse::<u32>() {
                config.retention.days = days;
            }
        }

        if let Ok(mb_str) = std::env::var("WAKE_MAX_OUTPUT_MB") {
            if let Ok(mb) = mb_str.parse::<u32>() {
                config.output.max_mb = mb;
            }
        }

        Ok(config)
    }

    fn load_from_file() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    fn config_path() -> Result<PathBuf, ConfigError> {
        dirs::home_dir().map(|h| h.join(".wake").join("config.toml")).ok_or(ConfigError::NoHomeDir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_default_retention() {
        let config = Config::default();
        assert_eq!(config.retention.days, 21);
    }

    #[test]
    fn test_env_var_override() {
        // Save original value if set
        let original = env::var("WAKE_RETENTION_DAYS").ok();

        env::set_var("WAKE_RETENTION_DAYS", "7");
        let config = Config::load().unwrap();
        assert_eq!(config.retention.days, 7);

        // Restore original value
        match original {
            Some(val) => env::set_var("WAKE_RETENTION_DAYS", val),
            None => env::remove_var("WAKE_RETENTION_DAYS"),
        }
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_str = r#"
[retention]
days = 14
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retention.days, 14);
    }

    #[test]
    fn test_parse_empty_config() {
        let toml_str = "";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retention.days, 21); // default
    }

    #[test]
    fn test_parse_partial_config() {
        let toml_str = r#"
[retention]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retention.days, 21); // default when days not specified
    }

    #[test]
    fn test_default_output() {
        let config = Config::default();
        assert_eq!(config.output.max_mb, 5);
        assert_eq!(config.output.max_bytes(), 5 * 1024 * 1024);
    }

    #[test]
    fn test_output_env_var_override() {
        let original = env::var("WAKE_MAX_OUTPUT_MB").ok();

        env::set_var("WAKE_MAX_OUTPUT_MB", "10");
        let config = Config::load().unwrap();
        assert_eq!(config.output.max_mb, 10);

        match original {
            Some(val) => env::set_var("WAKE_MAX_OUTPUT_MB", val),
            None => env::remove_var("WAKE_MAX_OUTPUT_MB"),
        }
    }

    #[test]
    fn test_parse_output_config_toml() {
        let toml_str = r#"
[output]
max_mb = 20
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output.max_mb, 20);
        assert_eq!(config.output.max_bytes(), 20 * 1024 * 1024);
    }

    #[test]
    fn test_parse_full_config_toml() {
        let toml_str = r#"
[retention]
days = 30

[output]
max_mb = 10

[summarization]
enabled = true
min_bytes = 2048
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.retention.days, 30);
        assert_eq!(config.output.max_mb, 10);
        assert!(config.summarization.enabled);
        assert_eq!(config.summarization.min_bytes, 2048);
    }

    #[test]
    fn test_default_summarization() {
        let config = Config::default();
        assert!(!config.summarization.enabled);
        assert_eq!(config.summarization.min_bytes, 1024);
        assert_eq!(config.summarization.min_bytes_usize(), 1024);
    }

    #[test]
    fn test_parse_summarization_config_toml() {
        let toml_str = r#"
[summarization]
enabled = true
min_bytes = 512
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.summarization.enabled);
        assert_eq!(config.summarization.min_bytes, 512);
    }

    #[test]
    fn test_summarization_disabled_by_default() {
        let toml_str = "";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.summarization.enabled);
    }
}
