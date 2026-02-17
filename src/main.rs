mod app;
mod audio;
mod cli;
mod config;
mod error;
mod feedback;
mod file_audio;
mod inject;
mod model;
mod setup;
mod transcribe;

use std::path::{Path, PathBuf};

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command, ModelAction};
use crate::config::Config;
use crate::transcribe::{TranscriptionBackend, WhisperLocal};

fn pid_file_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("whspr-rs.pid")
}

fn read_running_pid() -> Option<u32> {
    let path = pid_file_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    let pid: u32 = contents.trim().parse().ok()?;

    // Verify the process is actually running
    let proc_path = format!("/proc/{pid}");
    if std::path::Path::new(&proc_path).exists() {
        Some(pid)
    } else {
        // Stale PID file, clean up
        let _ = std::fs::remove_file(&path);
        None
    }
}

fn write_pid_file() -> std::io::Result<()> {
    let path = pid_file_path();
    std::fs::write(&path, std::process::id().to_string())
}

fn remove_pid_file() {
    let _ = std::fs::remove_file(pid_file_path());
}

fn init_tracing(verbose: u8) {
    let filter = match verbose {
        0 => "whspr_rs=info",
        1 => "whspr_rs=debug",
        _ => "whspr_rs=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .compact()
        .init();
}

async fn transcribe_file(
    cli: &Cli,
    file: &Path,
    output: Option<&Path>,
) -> crate::error::Result<()> {
    let config = Config::load(cli.config.as_deref())?;
    let model_path = config.resolved_model_path();

    tracing::info!("decoding audio file: {}", file.display());
    let samples = file_audio::decode_audio_file(file)?;

    let backend = tokio::task::spawn_blocking(move || {
        WhisperLocal::new(&config.whisper, &model_path)
    })
    .await
    .unwrap()?;

    let text = tokio::task::spawn_blocking(move || backend.transcribe(&samples, 16000))
        .await
        .unwrap()?;

    if let Some(out_path) = output {
        tokio::fs::write(out_path, &text).await?;
        tracing::info!("transcription written to {}", out_path.display());
    } else {
        println!("{text}");
    }

    Ok(())
}

async fn run_default(cli: &Cli) -> crate::error::Result<()> {
    // Check if an instance is already recording
    if let Some(pid) = read_running_pid() {
        tracing::info!("sending toggle signal to running instance (pid {pid})");
        let ret = unsafe { libc::kill(pid as i32, libc::SIGUSR1) };
        if ret != 0 {
            tracing::warn!(
                "failed to signal pid {pid}: {}",
                std::io::Error::last_os_error()
            );
            let _ = std::fs::remove_file(pid_file_path());
            // fall through to start new instance
        } else {
            return Ok(());
        }
    }

    tracing::info!("whspr-rs v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = Config::load(cli.config.as_deref())?;
    tracing::debug!("config loaded: {config:?}");

    // Write PID file so a second invocation can signal us
    write_pid_file()?;

    // Ensure PID file is cleaned up on exit
    let result = app::run(config).await;

    remove_pid_file();

    result
}

#[tokio::main]
async fn main() -> crate::error::Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose);

    match &cli.command {
        None => run_default(&cli).await,
        Some(Command::Setup) => setup::run_setup().await,
        Some(Command::Transcribe { file, output }) => {
            transcribe_file(&cli, file, output.as_deref()).await
        }
        Some(Command::Model { action }) => match action {
            ModelAction::List => {
                model::list_models();
                Ok(())
            }
            ModelAction::Download { name } => {
                model::download_model(name).await?;
                Ok(())
            }
            ModelAction::Select { name } => model::select_model(name),
        },
    }
}
