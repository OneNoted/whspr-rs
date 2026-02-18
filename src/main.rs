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

struct PidLock {
    path: PathBuf,
    _file: std::fs::File,
}

impl Drop for PidLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn pid_file_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("whspr-rs.pid")
}

fn read_pid_from_lock(path: &Path) -> Option<u32> {
    let contents = std::fs::read_to_string(path).ok()?;
    contents.trim().parse().ok()
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn pid_belongs_to_whspr(pid: u32) -> bool {
    if !process_exists(pid) {
        return false;
    }

    let current_exe = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok());
    let target_exe = std::fs::canonicalize(format!("/proc/{pid}/exe")).ok();

    if let (Some(current), Some(target)) = (current_exe.as_ref(), target_exe.as_ref()) {
        if current == target {
            return true;
        }
    }

    let current_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "whspr-rs".into());
    let cmdline = match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let Some(first_arg) = cmdline.split(|b| *b == 0).next() else {
        return false;
    };
    if first_arg.is_empty() {
        return false;
    }
    let first_arg = String::from_utf8_lossy(first_arg);
    Path::new(first_arg.as_ref())
        .file_name()
        .map(|name| name.to_string_lossy() == current_name)
        .unwrap_or(false)
}

fn try_acquire_pid_lock(path: &Path) -> std::io::Result<PidLock> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(file, "{}", std::process::id())?;

    Ok(PidLock {
        path: path.to_path_buf(),
        _file: file,
    })
}

fn signal_existing_instance(path: &Path) -> crate::error::Result<bool> {
    let Some(pid) = read_pid_from_lock(path) else {
        tracing::warn!("stale pid lock at {}, removing", path.display());
        let _ = std::fs::remove_file(path);
        return Ok(false);
    };

    if !pid_belongs_to_whspr(pid) {
        tracing::warn!(
            "pid lock at {} points to non-whspr process ({pid}), removing",
            path.display()
        );
        let _ = std::fs::remove_file(path);
        return Ok(false);
    }

    tracing::info!("sending toggle signal to running instance (pid {pid})");
    let ret = unsafe { libc::kill(pid as i32, libc::SIGUSR1) };
    if ret == 0 {
        return Ok(true);
    }

    let err = std::io::Error::last_os_error();
    tracing::warn!("failed to signal pid {pid}: {err}");
    if err.raw_os_error() == Some(libc::ESRCH) {
        let _ = std::fs::remove_file(path);
        return Ok(false);
    }

    Err(err.into())
}

fn acquire_or_signal_lock() -> crate::error::Result<Option<PidLock>> {
    let path = pid_file_path();

    for _ in 0..2 {
        match try_acquire_pid_lock(&path) {
            Ok(lock) => return Ok(Some(lock)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if signal_existing_instance(&path)? {
                    return Ok(None);
                }
            }
            Err(e) => return Err(e.into()),
        }
    }

    Err(crate::error::WhsprError::Config(format!(
        "failed to acquire pid lock at {}",
        path.display()
    )))
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
    let Some(_pid_lock) = acquire_or_signal_lock()? else {
        return Ok(());
    };

    tracing::info!("whspr-rs v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = Config::load(cli.config.as_deref())?;
    tracing::debug!("config loaded: {config:?}");

    app::run(config).await
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
