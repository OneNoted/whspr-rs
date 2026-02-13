mod app;
mod audio;
mod config;
mod error;
mod feedback;
mod inject;
mod transcribe;

use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[derive(Parser, Debug)]
#[command(name = "whspr-rs", version, about = "Speech-to-text dictation tool for Wayland")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Increase log verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn pid_file_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".into());
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = match cli.verbose {
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

    // Check if an instance is already recording
    if let Some(pid) = read_running_pid() {
        tracing::info!("sending toggle signal to running instance (pid {pid})");
        unsafe {
            libc::kill(pid as i32, libc::SIGUSR1);
        }
        return Ok(());
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

    result.map_err(Into::into)
}
