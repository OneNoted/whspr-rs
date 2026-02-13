use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::{Result, WhsprError};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub audio: AudioConfig,
    pub whisper: WhisperConfig,
    pub inject: InjectConfig,
    pub feedback: FeedbackConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub device: String,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WhisperConfig {
    pub model_path: String,
    pub language: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct InjectConfig {
    pub method: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FeedbackConfig {
    pub enabled: bool,
    pub start_sound: String,
    pub stop_sound: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            whisper: WhisperConfig::default(),
            inject: InjectConfig::default(),
            feedback: FeedbackConfig::default(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device: String::new(),
            sample_rate: 16000,
        }
    }
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model_path: "~/.local/share/whspr-rs/ggml-large-v3-turbo.bin".into(),
            language: "en".into(),
        }
    }
}

impl Default for InjectConfig {
    fn default() -> Self {
        Self {
            method: "clipboard".into(),
        }
    }
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_sound: String::new(),
            stop_sound: String::new(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = match path {
            Some(p) => p.to_path_buf(),
            None => default_config_path(),
        };

        if !config_path.exists() {
            tracing::info!("no config file found at {}, using defaults", config_path.display());
            return Ok(Config::default());
        }

        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| WhsprError::Config(format!("failed to read {}: {e}", config_path.display())))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| WhsprError::Config(format!("failed to parse {}: {e}", config_path.display())))?;

        Ok(config)
    }

    pub fn resolved_model_path(&self) -> PathBuf {
        let expanded = shellexpand::tilde(&self.whisper.model_path);
        PathBuf::from(expanded.as_ref())
    }
}

fn default_config_path() -> PathBuf {
    dirs_path("config").join("whspr-rs").join("config.toml")
}

fn dirs_path(kind: &str) -> PathBuf {
    match kind {
        "config" => {
            if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
                PathBuf::from(dir)
            } else if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(".config")
            } else {
                PathBuf::from("/tmp")
            }
        }
        _ => PathBuf::from("/tmp"),
    }
}
