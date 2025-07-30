# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build Commands
- **Build**: `cargo build --release`
- **Debug build**: `cargo build`
- **Run**: `cargo run` or `./target/release/simple-stt`

### Development Commands
- **Lint**: `cargo clippy -- -D warnings`
- **Test**: `cargo test -- --test-threads=1`
- **Single test**: `cargo test -- --test-threads=1 --test <test_name>`
- **Format**: `cargo fmt`

### Application Commands
- **Check config**: `./target/release/simple-stt --check-config`
- **Tune silence detection**: `./target/release/simple-stt --tune`
- **List profiles**: `./target/release/simple-stt --list-profiles`
- **Run with profile**: `./target/release/simple-stt --profile <name>`

## Architecture

### Core Components

**Main Application Flow (src/main.rs:28-176)**
- TUI-based application using ratatui with real-time audio recording
- Multi-threaded architecture: main UI thread + audio recording thread + async STT processing
- Uses mpsc channels for thread communication and tokio channels for async tasks

**Module Structure (src/lib.rs)**
- `audio`: Real-time PipeWire/CPAL audio recording with silence detection
- `config`: XDG-compliant TOML configuration with environment variable overrides
- `stt`: Pluggable speech-to-text backends (API + local Whisper support)
- `clipboard`: Wayland-native clipboard integration via wl-clipboard-rs
- `tui`: Terminal user interface with real-time status display

### STT Backend Architecture (src/stt/mod.rs)
- **Enum-based backend system**: `SttBackend::Api` and `SttBackend::Local`
- **Pluggable design**: Easy to add new STT providers
- **Local backend**: Uses whisper-rs for offline transcription with auto-model downloading
- **API backend**: OpenAI Whisper API with configurable models
- **Async processing**: Non-blocking transcription with progress reporting

### Configuration System (src/config/mod.rs)
- **XDG compliance**: Config stored in `~/.config/simple-stt/config.toml`
- **Environment overrides**: OPENAI_API_KEY and ANTHROPIC_API_KEY
- **LLM profiles**: Built-in profiles for different use cases (email, slack, todo, general)
- **Auto-generation**: Creates default config on first run

### Audio Pipeline (src/audio/mod.rs)
- **Real-time recording**: CPAL-based audio capture with configurable sample rate/channels
- **RMS level calculation**: For visual feedback and silence detection
- **Thread-safe**: Uses mpsc channels to communicate with main thread

## Key Features

- **Dual STT backends**: Local Whisper models (default, privacy-focused) and OpenAI API
- **LLM text refinement**: Optional post-processing with OpenAI/Anthropic for different contexts
- **Wayland-native**: Uses wl-clipboard for clipboard operations
- **Auto-paste support**: Integration with wtype/ydotool for direct text input
- **Model management**: Auto-download and caching of Whisper models in ~/.cache/simple-stt/

## Development Notes

### Backend Selection Logic
- Default backend is "local" for better UX (no API keys needed)
- Backend switching via config: `whisper.backend = "api"` or `"local"`
- Local backend preparation is async and happens in parallel with UI startup

### Threading Model
- Main thread: TUI rendering and event handling
- Audio thread: Continuous recording via CPAL
- Async tasks: STT processing, model downloading, LLM requests

### Error Handling
- Uses anyhow for error handling throughout
- Graceful degradation: app works even without models or API keys
- Comprehensive logging to both console and file (~/.local/share/simple-stt/simple-stt.log)

## Local Model Support

The application supports local Whisper models via whisper-rs:
- Models stored in ~/.cache/simple-stt/models/
- Auto-download from Hugging Face on first use
- Supported models: tiny.en (39MB), base.en (74MB), small.en (244MB), medium.en (769MB), large (1550MB)
- Device selection: auto, cpu, cuda

## Configuration Profiles

Built-in LLM profiles for text refinement:
- `general`: General text cleanup and formatting
- `todo`: Convert speech to actionable todo items
- `email`: Professional email formatting
- `slack`: Casual but professional messaging

Custom profiles can be added by editing the config file.
