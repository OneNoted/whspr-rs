use std::collections::HashSet;
use std::path::PathBuf;

use evdev::{Device, EventType, Key};
use tokio::sync::mpsc;

use crate::config::HotkeyConfig;
use crate::error::{Result, WhsprError};

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

pub struct HotkeyMonitor {
    target_keys: HashSet<Key>,
}

impl HotkeyMonitor {
    pub fn new(config: &HotkeyConfig) -> Result<Self> {
        let mut target_keys = HashSet::new();

        for key_name in &config.keys {
            let key = parse_key_name(key_name).ok_or_else(|| {
                WhsprError::Hotkey(format!("unknown key name: {key_name}"))
            })?;
            target_keys.insert(key);
        }

        tracing::info!("hotkey monitor configured for keys: {:?}", config.keys);

        Ok(Self { target_keys })
    }

    pub async fn run(self, tx: mpsc::Sender<HotkeyEvent>) -> Result<()> {
        let device = find_keyboard_device()?;
        let device_name = device
            .name()
            .unwrap_or("unknown")
            .to_string();
        tracing::info!("monitoring keyboard: {device_name}");

        let mut stream = device
            .into_event_stream()
            .map_err(|e| WhsprError::Hotkey(format!("failed to create event stream: {e}")))?;

        let mut held_keys: HashSet<Key> = HashSet::new();
        let mut combo_active = false;

        loop {
            let event = stream
                .next_event()
                .await
                .map_err(|e| WhsprError::Hotkey(format!("event read error: {e}")))?;

            if event.event_type() != EventType::KEY {
                continue;
            }

            let key = Key::new(event.code());
            let value = event.value(); // 0=release, 1=press, 2=repeat

            if !self.target_keys.contains(&key) {
                continue;
            }

            match value {
                1 => {
                    // Key press
                    held_keys.insert(key);

                    if !combo_active && held_keys.is_superset(&self.target_keys) {
                        combo_active = true;
                        tracing::debug!("hotkey combo pressed");
                        if tx.send(HotkeyEvent::Pressed).await.is_err() {
                            break;
                        }
                    }
                }
                0 => {
                    // Key release
                    held_keys.remove(&key);

                    if combo_active && !held_keys.is_superset(&self.target_keys) {
                        combo_active = false;
                        tracing::debug!("hotkey combo released");
                        if tx.send(HotkeyEvent::Released).await.is_err() {
                            break;
                        }
                    }
                }
                _ => {} // ignore repeats
            }
        }

        Ok(())
    }
}

fn find_keyboard_device() -> Result<Device> {
    let mut devices: Vec<(PathBuf, Device)> = evdev::enumerate()
        .filter(|(_path, dev)| {
            dev.supported_keys()
                .map(|keys| {
                    // Must have common keyboard keys to be a real keyboard
                    keys.contains(Key::KEY_A)
                        && keys.contains(Key::KEY_Z)
                        && keys.contains(Key::KEY_LEFTMETA)
                })
                .unwrap_or(false)
        })
        .collect();

    if devices.is_empty() {
        return Err(WhsprError::Hotkey(
            "no keyboard device found (do you have permission to read /dev/input?)".into(),
        ));
    }

    // Sort by path to get a deterministic result, prefer event0-style devices
    devices.sort_by(|(a, _), (b, _)| a.cmp(b));

    let (path, device) = devices.into_iter().next().unwrap();
    let name = device.name().unwrap_or("unknown").to_string();
    tracing::info!("selected keyboard device: {} ({})", name, path.display());

    Ok(device)
}

fn parse_key_name(name: &str) -> Option<Key> {
    // Support both "KEY_LEFTMETA" and "LEFTMETA" formats
    let name_upper = name.to_uppercase();
    let prefixed = if name_upper.starts_with("KEY_") {
        name_upper.clone()
    } else {
        format!("KEY_{name_upper}")
    };

    match prefixed.as_str() {
        // Modifiers
        "KEY_LEFTMETA" => Some(Key::KEY_LEFTMETA),
        "KEY_RIGHTMETA" => Some(Key::KEY_RIGHTMETA),
        "KEY_LEFTSHIFT" => Some(Key::KEY_LEFTSHIFT),
        "KEY_RIGHTSHIFT" => Some(Key::KEY_RIGHTSHIFT),
        "KEY_LEFTCTRL" => Some(Key::KEY_LEFTCTRL),
        "KEY_RIGHTCTRL" => Some(Key::KEY_RIGHTCTRL),
        "KEY_LEFTALT" => Some(Key::KEY_LEFTALT),
        "KEY_RIGHTALT" => Some(Key::KEY_RIGHTALT),

        // Function keys
        "KEY_F1" => Some(Key::KEY_F1),
        "KEY_F2" => Some(Key::KEY_F2),
        "KEY_F3" => Some(Key::KEY_F3),
        "KEY_F4" => Some(Key::KEY_F4),
        "KEY_F5" => Some(Key::KEY_F5),
        "KEY_F6" => Some(Key::KEY_F6),
        "KEY_F7" => Some(Key::KEY_F7),
        "KEY_F8" => Some(Key::KEY_F8),
        "KEY_F9" => Some(Key::KEY_F9),
        "KEY_F10" => Some(Key::KEY_F10),
        "KEY_F11" => Some(Key::KEY_F11),
        "KEY_F12" => Some(Key::KEY_F12),

        // Letters
        "KEY_A" => Some(Key::KEY_A),
        "KEY_B" => Some(Key::KEY_B),
        "KEY_C" => Some(Key::KEY_C),
        "KEY_D" => Some(Key::KEY_D),
        "KEY_E" => Some(Key::KEY_E),
        "KEY_F" => Some(Key::KEY_F),
        "KEY_G" => Some(Key::KEY_G),
        "KEY_H" => Some(Key::KEY_H),
        "KEY_I" => Some(Key::KEY_I),
        "KEY_J" => Some(Key::KEY_J),
        "KEY_K" => Some(Key::KEY_K),
        "KEY_L" => Some(Key::KEY_L),
        "KEY_M" => Some(Key::KEY_M),
        "KEY_N" => Some(Key::KEY_N),
        "KEY_O" => Some(Key::KEY_O),
        "KEY_P" => Some(Key::KEY_P),
        "KEY_Q" => Some(Key::KEY_Q),
        "KEY_R" => Some(Key::KEY_R),
        "KEY_S" => Some(Key::KEY_S),
        "KEY_T" => Some(Key::KEY_T),
        "KEY_U" => Some(Key::KEY_U),
        "KEY_V" => Some(Key::KEY_V),
        "KEY_W" => Some(Key::KEY_W),
        "KEY_X" => Some(Key::KEY_X),
        "KEY_Y" => Some(Key::KEY_Y),
        "KEY_Z" => Some(Key::KEY_Z),

        // Special
        "KEY_SPACE" => Some(Key::KEY_SPACE),
        "KEY_ENTER" => Some(Key::KEY_ENTER),
        "KEY_ESC" => Some(Key::KEY_ESC),
        "KEY_TAB" => Some(Key::KEY_TAB),
        "KEY_CAPSLOCK" => Some(Key::KEY_CAPSLOCK),

        _ => None,
    }
}
