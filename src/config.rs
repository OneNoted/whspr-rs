use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::error::{Result, WhsprError};

#[derive(Debug, Clone, Default, Deserialize)]
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InjectConfig {}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FeedbackConfig {
    pub enabled: bool,
    pub start_sound: String,
    pub stop_sound: String,
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
            language: "auto".into(),
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
        PathBuf::from(expand_tilde(&self.whisper.model_path))
    }
}

pub fn default_config_path() -> PathBuf {
    xdg_dir("config").join("whspr-rs").join("config.toml")
}

pub fn data_dir() -> PathBuf {
    xdg_dir("data").join("whspr-rs")
}

fn xdg_dir(kind: &str) -> PathBuf {
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
        "data" => {
            if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
                PathBuf::from(dir)
            } else if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(".local").join("share")
            } else {
                PathBuf::from("/tmp")
            }
        }
        _ => PathBuf::from("/tmp"),
    }
}

pub fn expand_tilde(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => {
            if let Ok(home) = std::env::var("HOME") {
                return format!("{home}/{rest}");
            }
        }
        None if path == "~" => {
            if let Ok(home) = std::env::var("HOME") {
                return home;
            }
        }
        _ => {}
    }
    path.to_string()
}

pub fn write_default_config(path: &Path, model_path: &str) -> Result<()> {
    let contents = format!(
        r#"# whspr-rs configuration
#
# Keybinding is handled by your compositor. Example for Hyprland:
#   bind = SUPER ALT, D, exec, whspr-rs
#
# First invocation starts recording, second invocation stops + transcribes + pastes.

[audio]
# Input device name (empty = system default)
device = ""
# Sample rate in Hz (whisper requires 16000)
sample_rate = 16000

[whisper]
# Path to ggml whisper model file
# Manage models with: whspr-rs model list / download / select
model_path = "{model_path}"
# Language code ("en", "fr", "de", etc.) or "auto" for auto-detect
language = "auto"

[feedback]
# Play sound feedback on start/stop
enabled = true
# Custom sound file paths (empty = use bundled sounds)
start_sound = ""
stop_sound = ""
"#
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WhsprError::Config(format!("failed to create config directory: {e}")))?;
    }

    std::fs::write(path, contents)
        .map_err(|e| WhsprError::Config(format!("failed to write config: {e}")))?;

    Ok(())
}

pub fn update_config_model_path(config_path: &Path, new_model_path: &str) -> Result<()> {
    let contents = std::fs::read_to_string(config_path)
        .map_err(|e| WhsprError::Config(format!("failed to read config: {e}")))?;

    let mut doc = contents
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| WhsprError::Config(format!("failed to parse config: {e}")))?;

    doc["whisper"]["model_path"] = toml_edit::value(new_model_path);

    std::fs::write(config_path, doc.to_string())
        .map_err(|e| WhsprError::Config(format!("failed to write config: {e}")))?;

    Ok(())
}
