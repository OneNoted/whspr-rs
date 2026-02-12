use std::time::Duration;

use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, EventType, InputEvent, Key};
use wl_clipboard_rs::copy::{MimeType, Options, Source};

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

        // Step 1: Copy text to Wayland clipboard
        self.copy_to_clipboard(text)?;

        // Step 2: Small delay to let clipboard settle
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Step 3: Simulate Ctrl+V paste via uinput
        self.simulate_paste().await?;

        tracing::info!("text injected successfully ({} chars)", text.len());
        Ok(())
    }

    fn copy_to_clipboard(&self, text: &str) -> Result<()> {
        let opts = Options::new();
        opts.copy(
            Source::Bytes(text.as_bytes().into()),
            MimeType::Autodetect,
        )
        .map_err(|e| WhsprError::Injection(format!("failed to copy to clipboard: {e}")))?;

        tracing::debug!("text copied to clipboard");
        Ok(())
    }

    async fn simulate_paste(&self) -> Result<()> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::KEY_LEFTCTRL);
        keys.insert(Key::KEY_V);

        let mut device = VirtualDeviceBuilder::new()
            .map_err(|e| WhsprError::Injection(format!("failed to create virtual device: {e}")))?
            .name("whspr-rs-keyboard")
            .with_keys(&keys)
            .map_err(|e| WhsprError::Injection(format!("failed to set keys: {e}")))?
            .build()
            .map_err(|e| WhsprError::Injection(format!("failed to build virtual device: {e}")))?;

        // Small delay for uinput device to register
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Press Ctrl
        let ctrl_down = InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 1);
        device
            .emit(&[ctrl_down])
            .map_err(|e| WhsprError::Injection(format!("failed to press ctrl: {e}")))?;

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Press V
        let v_down = InputEvent::new(EventType::KEY, Key::KEY_V.code(), 1);
        device
            .emit(&[v_down])
            .map_err(|e| WhsprError::Injection(format!("failed to press v: {e}")))?;

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Release V
        let v_up = InputEvent::new(EventType::KEY, Key::KEY_V.code(), 0);
        device
            .emit(&[v_up])
            .map_err(|e| WhsprError::Injection(format!("failed to release v: {e}")))?;

        tokio::time::sleep(Duration::from_millis(10)).await;

        // Release Ctrl
        let ctrl_up = InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 0);
        device
            .emit(&[ctrl_up])
            .map_err(|e| WhsprError::Injection(format!("failed to release ctrl: {e}")))?;

        tracing::debug!("paste keystroke simulated");
        Ok(())
    }
}
