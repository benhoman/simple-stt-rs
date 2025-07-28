# Simple STT RS

A high-performance, Wayland-native speech-to-text CLI client written in Rust for modern Linux desktops. Designed specifically for **Wayland compositors** (like Hyprland) with **PipeWire audio support**, featuring automatic silence detection, LLM text refinement, and seamless clipboard integration. **Local transcription by default** - works immediately without any API keys!

## Features

- **🎤 Smart Audio Recording**: Real-time recording with PipeWire integration and configurable silence detection
- **🔇 Auto-Stop**: Automatically stops recording after detecting silence (configurable duration)
- **🎯 Auto-Tuning**: Built-in threshold calibration for optimal silence detection
- **🏠 Local Transcription**: Offline Whisper models for privacy & speed (default)
- **☁️ Cloud Option**: OpenAI Whisper API support *(optional)*
- **✨ Text Refinement**: Optional LLM-powered text cleanup and formatting
- **📝 Multiple Profiles**: Different processing profiles (email, slack, todo, etc.)
- **📋 Wayland Clipboard**: Native wl-clipboard integration for Wayland compositors
- **🖥️ Auto-Paste**: Direct text input via wtype (Wayland) or ydotool
- **📤 Flexible Output**: Copy to clipboard or output to stdout
- **⚡ Fast Performance**: Rust-based for minimal overhead
- **🛠️ XDG Compliant**: Configuration stored in proper XDG directories
- **📊 Comprehensive Logging**: Detailed logs to console and file
- **🔧 Graceful Degradation**: Always works - even without models or API keys!

## System Requirements

- **Wayland compositor** (Hyprland, Sway, GNOME, KDE, etc.)
- **PipeWire** for audio (standard on modern Linux)
- **wl-clipboard** for clipboard operations (`wl-copy`, `wl-paste`)
- **wtype** or **ydotool** for auto-paste functionality *(optional)*

## Usage Modes

The application adapts based on your configuration:

### 🏠 **Local Mode** (Default - No setup required!)
Record → Transcribe Locally → Clipboard/Stdout
```bash
simple-stt           # Copy to clipboard
simple-stt --stdout  # Output to stdout
```

### ☁️ **Cloud Mode** (OpenAI API)  
Record → Cloud Transcribe → Clipboard/Stdout
```bash
# Switch to cloud in config.yaml:
# whisper:
#   backend: api
export OPENAI_API_KEY="your-key-here"
simple-stt --stdout  # Output to stdout
```

### 🚀 **Full Mode** (Local + LLM Refinement)
Record → Transcribe Locally → Refine with LLM → Clipboard/Stdout
```bash
export OPENAI_API_KEY="your-key-here"  # For LLM refinement only
simple-stt --profile email --stdout
```

### 🎤 **Audio-Only Mode** (Fallback)
Record → Save audio file
```bash
# If models aren't available and no API key
```

## Installation

### Prerequisites

#### Linux (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install pipewire-audio wl-clipboard
# For auto-paste functionality (optional):
sudo apt install wtype        # Wayland native typing
# OR:
sudo apt install ydotool      # Universal input tool
```

#### Linux (Fedora)
```bash
sudo dnf install pipewire wl-clipboard
# For auto-paste functionality (optional):
sudo dnf install wtype        # Wayland native typing  
# OR:
sudo dnf install ydotool      # Universal input tool
```

#### Arch Linux
```bash
sudo pacman -S pipewire wl-clipboard
# For auto-paste functionality (optional):
yay -S wtype              # Wayland native typing
# OR:
sudo pacman -S ydotool    # Universal input tool
```

### Build from Source

```bash
git clone https://github.com/benhoman/simple-stt-rs
cd simple-stt-rs
cargo build --release
```

The binary will be available at `target/release/simple-stt`.

## Quick Start

### 1. Install and Run
```bash
cargo build --release
./target/release/simple-stt
```
**That's it!** The app will:
- Create a default configuration 
- Show you where the Whisper model will be downloaded
- Work immediately once you speak

### 2. Check Your Setup
```bash
simple-stt --check-config
```

### 3. First Recording
```bash
simple-stt
# Speak into your microphone
# App stops automatically after 2 seconds of silence
# Text copied to clipboard!
```

## Configuration

### Default Setup (Local Mode)

By default, the app is configured for **local transcription**:
- Backend: `local` (offline Whisper models)
- Model: `tiny.en` (~39MB, fast and accurate for English)
- Auto-download: `enabled`
- No API keys required!

### Switching to Cloud Mode

Edit `~/.config/simple-stt/config.yaml`:

```yaml
whisper:
  backend: api          # Switch to cloud
  api_key: null         # Set via OPENAI_API_KEY env var
  model: whisper-1      # API model name
  language: null
```

### Local Models Available

- `tiny.en` (~39MB) - **Default**, fast, good for English
- `base.en` (~74MB) - Better accuracy, still fast
- `small.en` (~244MB) - High accuracy, moderate speed
- `medium.en` (~769MB) - Very high accuracy
- `large` (~1550MB) - Best accuracy, supports all languages

Change model in config:
```yaml
whisper:
  backend: local
  model: base.en        # Upgrade to better model
```

### Full Configuration File

```yaml
audio:
  sample_rate: 16000
  channels: 1
  chunk_size: 2048
  silence_threshold: 15.0
  silence_duration: 2.0
  max_recording_time: 120.0

whisper:
  backend: local        # "local" or "api"
  api_key: null         # Set via environment or here
  model: tiny.en        # Local: tiny.en, base.en, etc. | API: whisper-1
  language: en          # Language hint (null for auto-detect)
  timeout: 60
  model_path: null      # Custom model path (optional)
  download_models: true # Auto-download models
  device: auto          # "auto", "cpu", "cuda"

llm:
  provider: openai
  model: gpt-3.5-turbo
  max_tokens: 500
  default_profile: general
  api_key: null         # Uses same OpenAI key by default
  profiles:
    general:
      name: General Text Cleanup
      prompt: "Please clean up and format this transcribed text..."
    todo:
      name: Todo/Task
      prompt: "Convert this speech into a clear, actionable todo item..."
    email:
      name: Email Format
      prompt: "Format this transcribed text as a professional email..."
    slack:
      name: Slack Message
      prompt: "Format this transcribed text as a clear, concise Slack message..."

clipboard:
  auto_paste: false
  paste_delay: 0.1

ui:
  enabled: true
  position_x: 50
  position_y: 50
  auto_hide_delay: 3.0
```

## Usage

### Basic Usage

```bash
# Start recording immediately (local transcription)
simple-stt

# Check what features are available
simple-stt --check-config

# Use verbose logging
simple-stt -v

# Use a specific profile for text refinement (requires API key)
simple-stt --profile todo
simple-stt --profile email
simple-stt --profile slack
```

### First-Time Setup

1. **Check your configuration**:
   ```bash
   simple-stt --check-config
   ```

2. **Tune silence detection** (recommended):
   ```bash
   simple-stt --tune
   ```
   Follow the prompts to calibrate optimal silence detection for your microphone and environment.

3. **List available profiles**:
   ```bash
   simple-stt --list-profiles
   ```

## CLI Options

- `simple-stt` - Record and transcribe (copy to clipboard)
- `simple-stt --stdout` - Record and transcribe (output to stdout)  
- `simple-stt --check-config` - Check configuration status and available features
- `simple-stt --tune` - Calibrate silence detection threshold
- `simple-stt --list-profiles` - Show available LLM profiles
- `simple-stt --profile <name>` - Use specific processing profile
- `simple-stt --verbose` - Enable debug logging

### LLM Profiles

The application includes several built-in profiles for different use cases:

- **general**: General text cleanup and formatting
- **todo**: Convert speech to actionable todo items
- **email**: Format as professional email text
- **slack**: Format as casual but professional Slack messages

You can add custom profiles by editing the configuration file.

## Workflow

### Default Flow (Local Transcription)
1. **Start**: Run `simple-stt` command
2. **Record**: Speak into your microphone  
3. **Auto-Stop**: Recording stops automatically after detecting silence
4. **Transcribe**: Audio is processed locally with Whisper
5. **Output**: Result is copied to clipboard

### With LLM Refinement
1. **Start**: Run `simple-stt --profile email` 
2. **Record**: Speak into your microphone
3. **Auto-Stop**: Recording stops automatically after detecting silence
4. **Transcribe**: Audio is processed locally with Whisper
5. **Refine**: Text is processed through selected LLM profile
6. **Output**: Refined result is copied to clipboard

## Keybinding Integration

### Hyprland

Add to your `~/.config/hypr/hyprland.conf`:

```conf
# Basic STT (local transcription)
bind = ALT SHIFT, R, exec, simple-stt

# Profile-specific shortcuts (requires API key for LLM refinement)
bind = ALT SHIFT, T, exec, simple-stt --profile todo
bind = ALT SHIFT, E, exec, simple-stt --profile email
bind = ALT SHIFT, S, exec, simple-stt --profile slack
```

### i3/sway

Add to your config:

```conf
# Basic STT (local transcription)
bindsym $mod+Shift+r exec simple-stt

# Profile-specific shortcuts (requires API key for LLM refinement)
bindsym $mod+Shift+t exec simple-stt --profile todo
bindsym $mod+Shift+e exec simple-stt --profile email
bindsym $mod+Shift+s exec simple-stt --profile slack
```

## Auto-Paste Setup

For auto-paste functionality, install one of these tools:

- **X11**: `xdotool` (most common)
- **Wayland**: `wtype` or `ydotool`

Enable auto-paste in config:

```yaml
clipboard:
  auto_paste: true
  paste_delay: 0.1
```

## Troubleshooting

### Getting Started
- **First run**: App works immediately! No setup needed for local transcription
- **Model download**: On first use, the app will download the Whisper model (~39MB for tiny.en)
- **Check setup**: Run `simple-stt --check-config` to see status

### Audio Issues

- **No microphone detected**: Check `arecord -l` to list available devices
- **Permission denied**: Add user to `audio` group: `sudo usermod -a -G audio $USER`
- **ALSA warnings**: These are usually harmless but can be reduced with proper ALSA configuration

### Local Transcription Issues

- **Model not downloading**: Check internet connection and disk space
- **Slow transcription**: Try a smaller model (tiny.en vs base.en)
- **Poor accuracy**: Upgrade to a larger model (base.en, small.en, medium.en)
- **Out of memory**: Use a smaller model or close other applications

### Cloud API Issues

- **Authentication errors**: Verify your OpenAI API key is correct
- **Rate limits**: Check your OpenAI account usage and limits
- **Network timeouts**: Check internet connection and firewall settings

### Silence Detection

- **Recording doesn't stop**: Run `simple-stt --tune` to calibrate threshold
- **Stops too early**: Increase `silence_duration` in config
- **Doesn't detect speech**: Decrease `silence_threshold` in config

### Auto-Paste Issues

- **Not pasting**: Install `xdotool` (X11) or `wtype` (Wayland)
- **Wrong window**: Ensure target window is focused before running

## Model Storage

Local models are stored in:
- **Linux**: `~/.cache/simple-stt/models/`
- **Model files**: `ggml-{model-name}.bin` (e.g., `ggml-tiny.en.bin`)

You can manually download models if needed:
```bash
mkdir -p ~/.cache/simple-stt/models
wget https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin -O ~/.cache/simple-stt/models/ggml-tiny.en.bin
```

## Logging

Logs are written to:
- Console: Real-time status and errors
- File: `~/.local/share/simple-stt/simple-stt.log` (rotated daily)

Use `-v` flag for verbose debug logging.

## Performance Comparison

| Backend | Speed | Privacy | Cost | Accuracy | Setup |
|---------|--------|---------|------|----------|-------|
| **Local (tiny.en)** | ⚡⚡⚡ | 🔒🔒🔒 | Free | ⭐⭐⭐ | None |
| **Local (base.en)** | ⚡⚡ | 🔒🔒🔒 | Free | ⭐⭐⭐⭐ | None |
| **Cloud (API)** | ⚡ | ⭐ | $$ | ⭐⭐⭐⭐⭐ | API Key |

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running

```bash
cargo run -- --help
```

## Architecture

The application is organized into several modules:

- **config**: XDG-compliant configuration management with local/cloud backend selection
- **audio**: Real-time recording with PipeWire integration and silence detection
- **stt**: Pluggable STT backends (local Whisper models + OpenAI API)
- **llm**: LLM text refinement (OpenAI/Anthropic)
- **clipboard**: Wayland-native clipboard with wl-clipboard integration
- **ui**: Console-based status display

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

MIT License - see LICENSE file for details.

## Acknowledgments

- Inspired by the Python `simple-stt` implementation
- Built with modern Wayland and PipeWire integration for the Linux desktop
- Uses local Whisper models via whisper-rs for privacy and speed
- Wayland clipboard support via wl-clipboard-rs
- OpenAI Whisper for both local and cloud speech recognition 