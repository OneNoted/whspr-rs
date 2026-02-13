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

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(true);
        ctx_params.flash_attn(true);
        tracing::info!("GPU acceleration enabled (flash_attn=true)");

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            ctx_params,
        )
        .map_err(|e| WhsprError::Transcription(format!("failed to load whisper model: {e}")))?;

        tracing::info!("whisper model loaded successfully");

        Ok(Self {
            ctx,
            language: config.language.clone(),
        })
    }
}

const CHUNK_DURATION_SECS: f64 = 30.0;
const OVERLAP_SECS: f64 = 1.0;

#[async_trait]
impl TranscriptionBackend for WhisperLocal {
    async fn transcribe(&self, audio: &[f32], sample_rate: u32) -> Result<String> {
        // Audio diagnostics
        let duration_secs = audio.len() as f64 / sample_rate as f64;
        let rms = (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt();
        tracing::info!(
            "audio: {:.1}s, {} samples, RMS={:.4}",
            duration_secs,
            audio.len(),
            rms
        );

        let chunk_size = (CHUNK_DURATION_SECS * sample_rate as f64) as usize;
        let overlap = (OVERLAP_SECS * sample_rate as f64) as usize;

        if audio.len() <= chunk_size {
            // Short audio: process directly
            self.transcribe_chunk(&audio)
        } else {
            // Long audio: split into overlapping chunks
            let mut results = Vec::new();
            let mut offset = 0;

            while offset < audio.len() {
                let end = (offset + chunk_size).min(audio.len());
                let chunk = &audio[offset..end];
                tracing::info!(
                    "processing chunk: {:.1}s - {:.1}s",
                    offset as f64 / sample_rate as f64,
                    end as f64 / sample_rate as f64
                );

                let text = self.transcribe_chunk(chunk)?;
                if !text.is_empty() {
                    results.push(text);
                }

                if end == audio.len() {
                    break;
                }
                offset = end - overlap;
            }

            let text = results.join(" ");
            tracing::info!("transcription result: {text:?}");
            Ok(text)
        }
    }
}

impl WhisperLocal {
    fn transcribe_chunk(&self, audio: &[f32]) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_language(Some(&self.language));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        params.set_n_threads(n_threads);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| WhsprError::Transcription(format!("failed to create whisper state: {e}")))?;

        state
            .full(params, audio)
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
        if !text.is_empty() {
            tracing::debug!("chunk transcription: {text:?}");
        }

        Ok(text)
    }
}
