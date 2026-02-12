use std::path::PathBuf;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;

use crate::config::{self, data_dir, default_config_path, update_config_model_path};
use crate::error::{Result, WhsprError};

pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub size: &'static str,
    pub description: &'static str,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        name: "large-v3-turbo",
        filename: "ggml-large-v3-turbo.bin",
        size: "1.6 GB",
        description: "Best balance of speed and accuracy (recommended)",
    },
    ModelInfo {
        name: "large-v3-turbo-q5_0",
        filename: "ggml-large-v3-turbo-q5_0.bin",
        size: "574 MB",
        description: "Quantized turbo, smaller and slightly less accurate",
    },
    ModelInfo {
        name: "large-v3",
        filename: "ggml-large-v3.bin",
        size: "3.1 GB",
        description: "Most accurate, significantly slower",
    },
    ModelInfo {
        name: "large-v3-q5_0",
        filename: "ggml-large-v3-q5_0.bin",
        size: "1.1 GB",
        description: "Quantized large, good accuracy/size tradeoff",
    },
    ModelInfo {
        name: "medium",
        filename: "ggml-medium.bin",
        size: "1.5 GB",
        description: "Medium model",
    },
    ModelInfo {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size: "1.5 GB",
        description: "Medium model, English only",
    },
    ModelInfo {
        name: "small",
        filename: "ggml-small.bin",
        size: "488 MB",
        description: "Small model, fast",
    },
    ModelInfo {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size: "488 MB",
        description: "Small model, English only",
    },
    ModelInfo {
        name: "base",
        filename: "ggml-base.bin",
        size: "148 MB",
        description: "Base model, very fast",
    },
    ModelInfo {
        name: "base.en",
        filename: "ggml-base.en.bin",
        size: "148 MB",
        description: "Base model, English only",
    },
    ModelInfo {
        name: "tiny",
        filename: "ggml-tiny.bin",
        size: "78 MB",
        description: "Tiny model, fastest, least accurate",
    },
    ModelInfo {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size: "78 MB",
        description: "Tiny model, English only",
    },
];

pub fn find_model(name: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.name == name)
}

fn model_path(filename: &str) -> PathBuf {
    data_dir().join(filename)
}

fn active_model_path() -> Option<String> {
    let config_path = default_config_path();
    if !config_path.exists() {
        return None;
    }
    let contents = std::fs::read_to_string(&config_path).ok()?;
    let config: config::Config = toml::from_str(&contents).ok()?;
    Some(config.whisper.model_path)
}

fn model_status(info: &ModelInfo, active_resolved: Option<&std::path::Path>) -> &'static str {
    let path = model_path(info.filename);
    let is_active = active_resolved == Some(path.as_path());
    let is_local = path.exists();

    match (is_active, is_local) {
        (true, _) => "active",
        (_, true) => "local",
        _ => "remote",
    }
}

pub fn list_models() {
    let active_resolved =
        active_model_path().map(|p| std::path::PathBuf::from(config::expand_tilde(&p)));
    println!(
        "{:<22} {:>8}  {:<8}  DESCRIPTION",
        "MODEL", "SIZE", "STATUS"
    );
    println!("{}", "-".repeat(80));
    for m in MODELS {
        let status = model_status(m, active_resolved.as_deref());
        let marker = match status {
            "active" => "* ",
            _ => "  ",
        };
        println!(
            "{}{:<20} {:>8}  {:<8}  {}",
            marker, m.name, m.size, status, m.description
        );
    }
}

pub async fn download_model(name: &str) -> Result<PathBuf> {
    let info = find_model(name).ok_or_else(|| {
        let available: Vec<&str> = MODELS.iter().map(|m| m.name).collect();
        WhsprError::Download(format!(
            "unknown model '{}'. Available: {}",
            name,
            available.join(", ")
        ))
    })?;

    let dest = model_path(info.filename);
    let part_path = dest.with_extension("bin.part");

    if dest.exists() {
        println!("Model '{}' already downloaded at {}", name, dest.display());
        return Ok(dest);
    }

    // Ensure data directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WhsprError::Download(format!("failed to create data directory: {e}")))?;
    }

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        info.filename
    );

    println!("Downloading {} ({})...", info.name, info.size);

    let client = reqwest::Client::new();

    // Check for partial download to support resume
    let existing_len = if part_path.exists() {
        std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut request = client.get(&url);
    if existing_len > 0 {
        println!("Resuming from {} bytes...", existing_len);
        request = request.header("Range", format!("bytes={}-", existing_len));
    }

    let response = request
        .send()
        .await
        .map_err(|e| WhsprError::Download(format!("failed to start download: {e}")))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
    {
        return Err(WhsprError::Download(format!(
            "download failed with HTTP {}",
            response.status()
        )));
    }

    let total_size = if existing_len > 0 {
        // For range requests, content-length is remaining bytes
        response
            .content_length()
            .map(|cl| cl + existing_len)
            .unwrap_or(0)
    } else {
        response.content_length().unwrap_or(0)
    };

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_position(existing_len);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part_path)
        .await
        .map_err(|e| WhsprError::Download(format!("failed to open file: {e}")))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| WhsprError::Download(format!("download interrupted: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| WhsprError::Download(format!("failed to write: {e}")))?;
        pb.inc(chunk.len() as u64);
    }

    file.flush()
        .await
        .map_err(|e| WhsprError::Download(format!("failed to flush: {e}")))?;
    drop(file);

    pb.finish_with_message("done");

    // Atomic rename
    std::fs::rename(&part_path, &dest)
        .map_err(|e| WhsprError::Download(format!("failed to finalize download: {e}")))?;

    println!("Saved to {}", dest.display());
    Ok(dest)
}

pub fn select_model(name: &str) -> Result<()> {
    let info =
        find_model(name).ok_or_else(|| WhsprError::Download(format!("unknown model '{name}'")))?;

    let dest = model_path(info.filename);
    if !dest.exists() {
        return Err(WhsprError::Download(format!(
            "model '{}' is not downloaded yet. Run: whspr-rs model download {}",
            name, name
        )));
    }

    let config_path = default_config_path();
    let model_path_str = format!("~/.local/share/whspr-rs/{}", info.filename);

    if config_path.exists() {
        update_config_model_path(&config_path, &model_path_str)?;
    } else {
        config::write_default_config(&config_path, &model_path_str)?;
    }

    println!("Selected model '{}' as active.", name);
    println!("Config updated: {}", config_path.display());
    Ok(())
}
