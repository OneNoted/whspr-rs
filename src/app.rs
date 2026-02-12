#![allow(unused_assignments)]

use tokio::sync::mpsc;

use crate::audio::AudioRecorder;
use crate::config::{Config, HotkeyMode};
use crate::error::Result;
use crate::feedback::FeedbackPlayer;
use crate::hotkey::{HotkeyEvent, HotkeyMonitor};
use crate::inject::TextInjector;
use crate::transcribe::TranscriptionBackend;

#[derive(Debug, Clone, Copy, PartialEq)]
enum AppState {
    Idle,
    Recording,
    Transcribing,
    Injecting,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppState::Idle => write!(f, "idle"),
            AppState::Recording => write!(f, "recording"),
            AppState::Transcribing => write!(f, "transcribing"),
            AppState::Injecting => write!(f, "injecting"),
        }
    }
}

pub struct App {
    config: Config,
    backend: Box<dyn TranscriptionBackend>,
}

impl App {
    pub fn new(config: Config, backend: Box<dyn TranscriptionBackend>) -> Self {
        Self { config, backend }
    }

    pub async fn run(self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Result<()> {
        let feedback = FeedbackPlayer::new(
            self.config.feedback.enabled,
            &self.config.feedback.start_sound,
            &self.config.feedback.stop_sound,
        );

        let injector = TextInjector::new();

        let (hotkey_tx, mut hotkey_rx) = mpsc::channel::<HotkeyEvent>(32);

        let hotkey_monitor = HotkeyMonitor::new(&self.config.hotkey)?;
        let hotkey_mode = self.config.hotkey.mode.clone();

        // Spawn hotkey monitoring task
        tokio::spawn(async move {
            if let Err(e) = hotkey_monitor.run(hotkey_tx).await {
                tracing::error!("hotkey monitor error: {e}");
            }
        });

        let mut state = AppState::Idle;
        let mut recorder = AudioRecorder::new(&self.config.audio);
        let sample_rate = self.config.audio.sample_rate;

        tracing::info!("whspr-rs ready, waiting for hotkey...");

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("shutdown signal received");
                        break;
                    }
                }

                event = hotkey_rx.recv() => {
                    let Some(event) = event else {
                        tracing::error!("hotkey channel closed");
                        break;
                    };

                    match (&state, &event, &hotkey_mode) {
                        // Toggle mode: press starts, press again stops
                        (AppState::Idle, HotkeyEvent::Pressed, HotkeyMode::Toggle) => {
                            tracing::info!("state: idle -> recording");
                            state = AppState::Recording;
                            feedback.play_start();

                            if let Err(e) = recorder.start() {
                                tracing::error!("failed to start recording: {e}");
                                state = AppState::Idle;
                                continue;
                            }
                        }

                        (AppState::Recording, HotkeyEvent::Pressed, HotkeyMode::Toggle) => {
                            tracing::info!("state: recording -> transcribing");
                            state = AppState::Transcribing;
                            feedback.play_stop();

                            let audio = match recorder.stop() {
                                Ok(a) => a,
                                Err(e) => {
                                    tracing::error!("failed to stop recording: {e}");
                                    state = AppState::Idle;
                                    continue;
                                }
                            };

                            tracing::info!("transcribing {} samples...", audio.len());

                            match self.backend.transcribe(&audio, sample_rate).await {
                                Ok(text) if text.is_empty() => {
                                    tracing::warn!("transcription returned empty text");
                                    state = AppState::Idle;
                                }
                                Ok(text) => {
                                    tracing::info!("state: transcribing -> injecting");
                                    state = AppState::Injecting;

                                    match injector.inject(&text).await {
                                        Ok(()) => {
                                            tracing::info!("text injected successfully");
                                        }
                                        Err(e) => {
                                            tracing::error!("injection failed: {e}");
                                        }
                                    }

                                    state = AppState::Idle;
                                    tracing::info!("state: injecting -> idle");
                                }
                                Err(e) => {
                                    tracing::error!("transcription failed: {e}");
                                    state = AppState::Idle;
                                }
                            }
                        }

                        // Push-to-talk mode: hold to record, release to stop
                        (AppState::Idle, HotkeyEvent::Pressed, HotkeyMode::PushToTalk) => {
                            tracing::info!("state: idle -> recording (push-to-talk)");
                            state = AppState::Recording;
                            feedback.play_start();

                            if let Err(e) = recorder.start() {
                                tracing::error!("failed to start recording: {e}");
                                state = AppState::Idle;
                                continue;
                            }
                        }

                        (AppState::Recording, HotkeyEvent::Released, HotkeyMode::PushToTalk) => {
                            tracing::info!("state: recording -> transcribing (push-to-talk)");
                            state = AppState::Transcribing;
                            feedback.play_stop();

                            let audio = match recorder.stop() {
                                Ok(a) => a,
                                Err(e) => {
                                    tracing::error!("failed to stop recording: {e}");
                                    state = AppState::Idle;
                                    continue;
                                }
                            };

                            tracing::info!("transcribing {} samples...", audio.len());

                            match self.backend.transcribe(&audio, sample_rate).await {
                                Ok(text) if text.is_empty() => {
                                    tracing::warn!("transcription returned empty text");
                                    state = AppState::Idle;
                                }
                                Ok(text) => {
                                    tracing::info!("state: transcribing -> injecting");
                                    state = AppState::Injecting;

                                    match injector.inject(&text).await {
                                        Ok(()) => {
                                            tracing::info!("text injected successfully");
                                        }
                                        Err(e) => {
                                            tracing::error!("injection failed: {e}");
                                        }
                                    }

                                    state = AppState::Idle;
                                    tracing::info!("state: injecting -> idle");
                                }
                                Err(e) => {
                                    tracing::error!("transcription failed: {e}");
                                    state = AppState::Idle;
                                }
                            }
                        }

                        // Ignore irrelevant events for current state
                        (s, e, _) => {
                            tracing::debug!("ignoring event {e:?} in state {s}");
                        }
                    }
                }
            }
        }

        tracing::info!("app shutting down");
        Ok(())
    }
}
