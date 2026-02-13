use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig};

use crate::config::AudioConfig;
use crate::error::{Result, WhsprError};

pub struct AudioRecorder {
    config: AudioConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
}

impl AudioRecorder {
    pub fn new(config: &AudioConfig) -> Self {
        Self {
            config: config.clone(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();

        let device = if self.config.device.is_empty() {
            host.default_input_device()
                .ok_or_else(|| WhsprError::Audio("no default input device found".into()))?
        } else {
            host.input_devices()
                .map_err(|e| WhsprError::Audio(format!("failed to enumerate input devices: {e}")))?
                .find(|d| {
                    d.name()
                        .map(|n| n.contains(&self.config.device))
                        .unwrap_or(false)
                })
                .ok_or_else(|| {
                    WhsprError::Audio(format!(
                        "input device '{}' not found",
                        self.config.device
                    ))
                })?
        };

        let device_name = device.name().unwrap_or_else(|_| "unknown".into());
        tracing::info!("using input device: {device_name}");

        let stream_config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(self.config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Check if the device supports our desired config, fall back to supported config
        let supported = device
            .supported_input_configs()
            .map_err(|e| WhsprError::Audio(format!("failed to get supported configs: {e}")))?;

        let mut found_matching = false;
        for cfg in supported {
            if cfg.channels() == 1
                && cfg.min_sample_rate().0 <= self.config.sample_rate
                && cfg.max_sample_rate().0 >= self.config.sample_rate
                && cfg.sample_format() == SampleFormat::F32
            {
                found_matching = true;
                break;
            }
        }

        if !found_matching {
            tracing::warn!(
                "device may not natively support mono {}Hz f32, will attempt anyway",
                self.config.sample_rate
            );
        }

        let buffer = Arc::clone(&self.buffer);
        buffer.lock().expect("audio buffer lock").clear();

        let err_fn = |err: cpal::StreamError| {
            tracing::error!("audio stream error: {err}");
        };

        let stream = device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| WhsprError::Audio(format!("failed to build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| WhsprError::Audio(format!("failed to start audio stream: {e}")))?;

        self.stream = Some(stream);
        tracing::info!("audio recording started");
        Ok(())
    }

    pub fn stop(&mut self) -> Result<Vec<f32>> {
        // Drop the stream to stop recording
        self.stream.take();

        let buffer = std::mem::take(
            &mut *self
                .buffer
                .lock()
                .map_err(|_| WhsprError::Audio("audio buffer lock poisoned".into()))?,
        );
        tracing::info!("audio recording stopped, captured {} samples", buffer.len());

        if buffer.is_empty() {
            return Err(WhsprError::Audio("no audio data captured".into()));
        }

        Ok(buffer)
    }
}
