use crate::audio::AudioRecorder;
use crate::config::Config;
use crate::error::Result;
use crate::feedback::FeedbackPlayer;
use crate::inject::TextInjector;
use crate::transcribe::{TranscriptionBackend, WhisperLocal};

pub async fn run(config: Config) -> Result<()> {
    let feedback = FeedbackPlayer::new(
        config.feedback.enabled,
        &config.feedback.start_sound,
        &config.feedback.stop_sound,
    );

    // Start recording immediately
    let mut recorder = AudioRecorder::new(&config.audio);
    recorder.start()?;
    feedback.play_start();
    tracing::info!("recording... (run whspr-rs again to stop)");

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
            recorder.stop()?;
            return Ok(());
        }
        _ = sigterm.recv() => {
            tracing::info!("terminated, cancelling");
            recorder.stop()?;
            return Ok(());
        }
    }

    // Stop recording
    feedback.play_stop();
    let audio = recorder.stop()?;
    let sample_rate = config.audio.sample_rate;

    tracing::info!("transcribing {} samples...", audio.len());

    // Load model and transcribe
    let model_path = config.resolved_model_path();
    let backend = WhisperLocal::new(&config.whisper, &model_path)?;

    let text = backend.transcribe(&audio, sample_rate).await?;

    if text.is_empty() {
        tracing::warn!("transcription returned empty text");
        return Ok(());
    }

    // Inject text
    tracing::info!("injecting: {text:?}");
    let injector = TextInjector::new();
    injector.inject(&text).await?;

    tracing::info!("done");
    Ok(())
}
