use std::process::{Child, Command};

use crate::audio::AudioRecorder;
use crate::config::Config;
use crate::error::{Result, WhsprError};
use crate::feedback::FeedbackPlayer;
use crate::inject::TextInjector;
use crate::transcribe::{TranscriptionBackend, WhisperLocal};

pub async fn run(config: Config) -> Result<()> {
    let feedback = FeedbackPlayer::new(
        config.feedback.enabled,
        &config.feedback.start_sound,
        &config.feedback.stop_sound,
    );

    // Play start sound first (blocking), then start recording so the sound
    // doesn't leak into the mic.
    feedback.play_start();
    let mut recorder = AudioRecorder::new(&config.audio);
    recorder.start()?;
    let mut osd = spawn_osd();
    tracing::info!("recording... (run whspr-rs again to stop)");

    // Preload whisper model in background while recording
    let whisper_config = config.whisper.clone();
    let model_path = config.resolved_model_path();
    let model_handle =
        tokio::task::spawn_blocking(move || WhisperLocal::new(&whisper_config, &model_path));

    // Wait for SIGUSR1 (second invocation) or SIGINT/SIGTERM
    let mut sigusr1 = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1())
        .expect("failed to register SIGUSR1 handler");
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");

    tokio::select! {
        _ = sigusr1.recv() => {
            tracing::info!("toggle signal received, stopping recording");
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("interrupted, cancelling");
            kill_osd(&mut osd);
            recorder.stop()?;
            return Ok(());
        }
        _ = sigterm.recv() => {
            tracing::info!("terminated, cancelling");
            kill_osd(&mut osd);
            recorder.stop()?;
            return Ok(());
        }
    }

    // Stop recording before playing feedback so the stop sound doesn't
    // leak into the mic.
    kill_osd(&mut osd);
    let audio = recorder.stop()?;
    feedback.play_stop();
    let sample_rate = config.audio.sample_rate;

    tracing::info!("transcribing {} samples...", audio.len());

    // Await preloaded model (instant if it finished during recording)
    let backend = model_handle
        .await
        .map_err(|e| WhsprError::Transcription(format!("model loading task failed: {e}")))??;

    let text = tokio::task::spawn_blocking(move || backend.transcribe(&audio, sample_rate))
        .await
        .map_err(|e| WhsprError::Transcription(format!("task panicked: {e}")))??;

    if text.is_empty() {
        tracing::warn!("transcription returned empty text");
        // When the RMS/duration gates skip transcription, the process would
        // exit almost immediately after play_stop().  PipeWire may still be
        // draining the stop sound's last buffer; exiting while it's "warm"
        // causes an audible click as the OS closes our audio file descriptors.
        // With speech, transcription takes seconds — providing natural drain time.
        std::thread::sleep(std::time::Duration::from_millis(150));
        return Ok(());
    }

    // Inject text
    tracing::info!("injecting: {text:?}");
    let injector = TextInjector::new();
    injector.inject(&text).await?;

    tracing::info!("done");
    Ok(())
}

fn spawn_osd() -> Option<Child> {
    // Look for whspr-osd next to our own binary first, then fall back to PATH
    let osd_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|dir| dir.join("whspr-osd")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "whspr-osd".into());

    match Command::new(&osd_path).spawn() {
        Ok(child) => {
            tracing::debug!("spawned whspr-osd (pid {})", child.id());
            Some(child)
        }
        Err(e) => {
            tracing::warn!("failed to spawn whspr-osd from {}: {e}", osd_path.display());
            None
        }
    }
}

fn kill_osd(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        let pid = c.id();
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let _ = c.wait();
        tracing::debug!("whspr-osd (pid {pid}) terminated");
    }
}
