use std::io::Cursor;

use rodio::{Decoder, OutputStream, Sink};

use crate::error::{Result, WhsprError};

// Bundled sounds (embedded at compile time)
const START_SOUND: &[u8] = include_bytes!("../sounds/start.wav");
const STOP_SOUND: &[u8] = include_bytes!("../sounds/stop.wav");

#[derive(Clone)]
pub struct FeedbackPlayer {
    enabled: bool,
    start_sound_path: Option<String>,
    stop_sound_path: Option<String>,
}

impl FeedbackPlayer {
    pub fn new(enabled: bool, start_sound: &str, stop_sound: &str) -> Self {
        Self {
            enabled,
            start_sound_path: if start_sound.is_empty() {
                None
            } else {
                Some(start_sound.to_string())
            },
            stop_sound_path: if stop_sound.is_empty() {
                None
            } else {
                Some(stop_sound.to_string())
            },
        }
    }

    pub fn play_start(&self) {
        if !self.enabled {
            return;
        }
        let path = self.start_sound_path.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = play_sound(path.as_deref(), START_SOUND) {
                tracing::warn!("failed to play start sound: {e}");
            }
        });
    }

    pub fn play_stop(&self) {
        if !self.enabled {
            return;
        }
        let path = self.stop_sound_path.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = play_sound(path.as_deref(), STOP_SOUND) {
                tracing::warn!("failed to play stop sound: {e}");
            }
        });
    }
}

fn play_sound(custom_path: Option<&str>, bundled: &'static [u8]) -> Result<()> {
    let (_stream, handle) = OutputStream::try_default()
        .map_err(|e| WhsprError::Feedback(format!("failed to open audio output: {e}")))?;

    let sink = Sink::try_new(&handle)
        .map_err(|e| WhsprError::Feedback(format!("failed to create sink: {e}")))?;

    if let Some(path) = custom_path {
        let file = std::fs::File::open(path)
            .map_err(|e| WhsprError::Feedback(format!("failed to open sound file: {e}")))?;
        let reader = std::io::BufReader::new(file);
        let source = Decoder::new(reader)
            .map_err(|e| WhsprError::Feedback(format!("failed to decode sound file: {e}")))?;
        sink.append(source);
    } else {
        let cursor = Cursor::new(bundled);
        let source = Decoder::new(cursor)
            .map_err(|e| WhsprError::Feedback(format!("failed to decode bundled sound: {e}")))?;
        sink.append(source);
    }

    sink.sleep_until_end();
    Ok(())
}
