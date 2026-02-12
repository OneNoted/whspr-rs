use std::path::Path;

use async_trait::async_trait;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::config::WhisperConfig;
use crate::error::{Result, WhsprError};

#[async_trait]
pub trait TranscriptionBackend: Send + Sync {
    async fn transcribe(&self, audio: &[f32], sample_rate: u32) -> Result<String>;
}

pub struct WhisperLocal {
    ctx: WhisperContext,
    language: String,
}

impl WhisperLocal {
    pub fn new(config: &WhisperConfig, model_path: &Path) -> Result<Self> {
        if !model_path.exists() {
            return Err(WhsprError::Transcription(format!(
                "model file not found: {}",
                model_path.display()
            )));
        }

        tracing::info!("loading whisper model from {}", model_path.display());

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            WhisperContextParameters::default(),
        )
        .map_err(|e| WhsprError::Transcription(format!("failed to load whisper model: {e}")))?;

        tracing::info!("whisper model loaded successfully");

        Ok(Self {
            ctx,
            language: config.language.clone(),
        })
    }
}

#[async_trait]
impl TranscriptionBackend for WhisperLocal {
    async fn transcribe(&self, audio: &[f32], _sample_rate: u32) -> Result<String> {
        let audio = audio.to_vec();
        let language = self.language.clone();

        // whisper-rs is CPU-bound, so we get a reference to the context
        // and run in a blocking task. Since WhisperContext isn't Send,
        // we need to run it on the current thread via spawn_blocking workaround.
        // Actually, WhisperContext from whisper-rs implements Send, so we can
        // use a pointer trick. Let's just do the transcription inline since
        // the caller can wrap in spawn_blocking.

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_language(Some(&language));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        // Use 4 threads for transcription
        params.set_n_threads(4);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| WhsprError::Transcription(format!("failed to create whisper state: {e}")))?;

        state
            .full(params, &audio)
            .map_err(|e| WhsprError::Transcription(format!("transcription failed: {e}")))?;

        let num_segments = state.full_n_segments().map_err(|e| {
            WhsprError::Transcription(format!("failed to get segment count: {e}"))
        })?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        let text = text.trim().to_string();
        tracing::info!("transcription result: {text:?}");

        Ok(text)
    }
}
