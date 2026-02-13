# whspr-rs

Speech-to-text dictation for Wayland. Press a key to start recording, press it again to transcribe and paste.

Runs [whisper.cpp](https://github.com/ggerganov/whisper.cpp) locally — your audio never leaves your machine.

Inspired by [hyprwhspr](https://github.com/goodroot/hyprwhspr) by goodroot.

## How it works

1. Bind `whspr-rs` to a key in your compositor
2. First press starts recording (OSD overlay shows audio visualization)
3. Second press stops recording, transcribes with Whisper, and pastes via `Ctrl+Shift+V`

The two invocations communicate via PID file + `SIGUSR1` — no daemon, no IPC server.

## Requirements

- Rust 1.85+ (edition 2024)
- Linux with Wayland compositor
- `wl-copy` (from `wl-clipboard`)
- `uinput` access (for virtual keyboard paste)
- NVIDIA GPU + CUDA toolkit (optional, for GPU acceleration)

## Install

### From source

```sh
# With CUDA (recommended if you have an NVIDIA GPU)
cargo install --git https://github.com/OneNoted/whspr-rs

# Without CUDA
cargo install --git https://github.com/OneNoted/whspr-rs --no-default-features --features osd

# Without OSD overlay
cargo install --git https://github.com/OneNoted/whspr-rs --no-default-features --features cuda
```

### Setup

Run the interactive setup wizard to download a model and generate config:

```sh
whspr-rs setup
```

Or manage models manually:

```sh
whspr-rs model list          # show available models
whspr-rs model download large-v3-turbo
whspr-rs model select large-v3-turbo
```

## Compositor keybinding

### Hyprland

```conf
bind = SUPER ALT, D, exec, whspr-rs
```

### Sway

```conf
bindsym $mod+Alt+d exec whspr-rs
```

## Configuration

Config lives at `~/.config/whspr-rs/config.toml`. Generated automatically by `whspr-rs setup`, or copy from `config.example.toml`:

```toml
[audio]
device = ""            # empty = system default
sample_rate = 16000

[whisper]
model_path = "~/.local/share/whspr-rs/ggml-large-v3-turbo.bin"
language = "auto"      # or "en", "fr", "de", etc.

[feedback]
enabled = true
start_sound = ""       # empty = bundled sound
stop_sound = ""
```

## Models

| Model | Size | Speed | Notes |
|-------|------|-------|-------|
| large-v3-turbo | 1.6 GB | Fast | Best balance (recommended) |
| large-v3-turbo-q5_0 | 574 MB | Fast | Quantized, slightly less accurate |
| large-v3 | 3.1 GB | Slow | Most accurate |
| small / small.en | 488 MB | Very fast | Good for English-only |
| tiny / tiny.en | 78 MB | Instant | Least accurate |

Models are downloaded from [Hugging Face](https://huggingface.co/ggerganov/whisper.cpp) and stored in `~/.local/share/whspr-rs/`.

## uinput permissions

whspr-rs needs access to `/dev/uinput` for the virtual keyboard paste. Add your user to the `input` group:

```sh
sudo usermod -aG input $USER
```

Then log out and back in.

## Acknowledgements

This project is inspired by [hyprwhspr](https://github.com/goodroot/hyprwhspr) by [goodroot](https://github.com/goodroot), which provides native speech-to-text for Linux with support for multiple backends. whspr-rs is a from-scratch Rust reimplementation focused on local-only Whisper transcription with minimal dependencies.

## License

[MIT](LICENSE)
