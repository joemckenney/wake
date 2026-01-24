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

impl Config {
    /// Load config with precedence: env var > config file > defaults
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Self::load_from_file().unwrap_or_default();

        // Environment variable override
        if let Ok(days_str) = std::env::var("WAKE_RETENTION_DAYS") {
            if let Ok(days) = days_str.parse::<u32>() {
                config.retention.days = days;
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
}
