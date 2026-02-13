use std::process::{Command, Stdio};
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

use crate::error::{Result, WhsprError};

pub struct TextInjector;

impl TextInjector {
    pub fn new() -> Self {
        Self
    }

    pub async fn inject(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            tracing::warn!("empty text, nothing to inject");
            return Ok(());
        }

        // Create uinput device early so it registers with the compositor
        // while wl-copy + clipboard delay run in parallel
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::KEY_LEFTCTRL);
        keys.insert(KeyCode::KEY_LEFTSHIFT);
        keys.insert(KeyCode::KEY_V);

        let mut device = VirtualDevice::builder()
            .map_err(|e| WhsprError::Injection(format!("uinput: {e}")))?
            .name("whspr-rs-keyboard")
            .with_keys(&keys)
            .map_err(|e| WhsprError::Injection(format!("uinput keys: {e}")))?
            .build()
            .map_err(|e| WhsprError::Injection(format!("uinput build: {e}")))?;

        // Set text-only clipboard via wl-copy (stdin pipe, plain text MIME only)
        let mut wl_copy = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| WhsprError::Injection(format!("failed to spawn wl-copy: {e}")))?;

        if let Some(mut stdin) = wl_copy.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| WhsprError::Injection(format!("wl-copy stdin write: {e}")))?;
        }

        let status = wl_copy
            .wait()
            .map_err(|e| WhsprError::Injection(format!("wl-copy wait: {e}")))?;
        if !status.success() {
            return Err(WhsprError::Injection(format!(
                "wl-copy exited with {}",
                status
            )));
        }

        // Wait for compositor to process the clipboard offer.
        // The uinput device was created above, so it has already been
        // registering during the wl-copy write — no separate 60ms wait needed.
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Ctrl down, Shift down, V press+release, Shift up, Ctrl up
        device
            .emit(&[
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 1),
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTSHIFT.0, 1),
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_V.0, 1),
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_V.0, 0),
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTSHIFT.0, 0),
                InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 0),
            ])
            .map_err(|e| WhsprError::Injection(format!("paste keystroke: {e}")))?;

        tracing::info!("injected {} chars via wl-copy + Ctrl+Shift+V", text.len());
        Ok(())
    }
}
