mod app;
mod audio;
mod config;
mod error;
mod feedback;
mod hotkey;
mod inject;
mod transcribe;

use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::app::App;
use crate::config::Config;
use crate::transcribe::WhisperLocal;

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

    tracing::info!("whspr-rs v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = Config::load(cli.config.as_deref())?;
    tracing::debug!("config loaded: {config:?}");

    // Initialize whisper backend
    let model_path = config.resolved_model_path();
    let backend = WhisperLocal::new(&config.whisper, &model_path)?;

    // Setup graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT");
            }
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM");
            }
        }

        let _ = shutdown_tx.send(true);
    });

    // Run the app
    let app = App::new(config, Box::new(backend));
    app.run(shutdown_rx).await?;

    Ok(())
}
