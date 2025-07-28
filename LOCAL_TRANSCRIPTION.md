# Local Transcription Implementation Guide

This document explains how to add local transcription support to simple-stt-rs.

## Current Status

✅ **Architecture Ready**: The codebase now supports multiple STT backends via an enum-based approach
✅ **Configuration Ready**: Backend selection and local-specific config options are implemented
❌ **Local Backend**: Not yet implemented (currently returns an error)

## Implementation Options

### Option 1: whisper-rs (Recommended)
Uses Rust bindings for the whisper.cpp library.

**Pros:**
- Mature, well-tested
- Good performance (uses whisper.cpp optimizations)
- Supports quantized models

**Cons:**
- Requires C++ compilation
- External dependency

**Dependencies to add:**
```toml
[features]
local = ["whisper-rs"]

[dependencies]
whisper-rs = { version = "0.12", optional = true }
```

### Option 2: candle-whisper
Pure Rust implementation using the Candle ML framework.

**Pros:**
- Pure Rust (no C++ dependencies)
- Uses modern ML framework
- Good integration with Hugging Face models

**Cons:**
- Newer, less mature
- Larger binary size
- More complex dependency tree

**Dependencies to add:**
```toml
[features]
local = ["candle-core", "candle-nn", "candle-transformers", "hf-hub", "tokenizers"]

[dependencies]
candle-core = { version = "0.3", optional = true }
candle-nn = { version = "0.3", optional = true }
candle-transformers = { version = "0.3", optional = true }
hf-hub = { version = "0.3", optional = true, features = ["tokio"] }
tokenizers = { version = "0.15", optional = true }
```

### Option 3: ONNX Runtime
Uses ONNX models with the ort crate.

**Pros:**
- Lightweight
- Good performance
- Pre-converted models available

**Cons:**
- Requires ONNX model conversion
- Less flexible

## Implementation Steps

### 1. Add Local Backend Dependencies

Choose one of the options above and add to `Cargo.toml`:

```toml
[features]
default = ["api"]
api = ["reqwest"]
local = ["whisper-rs"]  # or chosen implementation

[dependencies]
whisper-rs = { version = "0.12", optional = true }
```

### 2. Create Local Backend Implementation

Create `src/stt/local.rs`:

```rust
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

#[cfg(feature = "local")]
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

use crate::config::{Config, WhisperConfig};

pub struct LocalSttBackend {
    config: WhisperConfig,
    #[cfg(feature = "local")]
    context: WhisperContext,
}

impl LocalSttBackend {
    pub fn new(config: &Config) -> Result<Self> {
        #[cfg(feature = "local")]
        {
            let model_path = config.whisper.model_path
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| {
                    // Default model path in cache directory
                    dirs::cache_dir()
                        .unwrap_or_else(|| std::env::temp_dir())
                        .join("simple-stt")
                        .join("models")
                        .join(format!("{}.bin", config.whisper.model))
                        .to_string_lossy()
                        .to_string()
                });

            // Download model if needed
            if config.whisper.download_models && !std::path::Path::new(&model_path).exists() {
                info!("Downloading Whisper model: {}", config.whisper.model);
                download_model(&config.whisper.model, &model_path)?;
            }

            let ctx_params = WhisperContextParameters::default();
            let context = WhisperContext::new_with_params(&model_path, ctx_params)
                .context("Failed to load Whisper model")?;

            Ok(Self {
                config: config.whisper.clone(),
                context,
            })
        }

        #[cfg(not(feature = "local"))]
        {
            Err(anyhow::anyhow!(
                "Local backend not available. Build with --features local"
            ))
        }
    }

    pub fn is_configured(&self) -> bool {
        true // Local backend doesn't need external API keys
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub async fn transcribe<P: AsRef<Path>>(&self, audio_path: P) -> Result<Option<String>> {
        #[cfg(feature = "local")]
        {
            let audio_path = audio_path.as_ref();

            if !audio_path.exists() {
                return Err(anyhow::anyhow!("Audio file not found: {:?}", audio_path));
            }

            info!("🔄 Transcribing audio file locally: {:?}", audio_path);

            // Convert audio to required format (16kHz mono f32)
            let audio_data = load_audio_file(audio_path)?;

            // Setup transcription parameters
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

            if let Some(ref language) = self.config.language {
                params.set_language(Some(language));
            }

            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            // Run transcription
            self.context
                .full(params, &audio_data)
                .context("Failed to run transcription")?;

            // Extract text
            let num_segments = self.context.full_n_segments()
                .context("Failed to get number of segments")?;

            let mut result = String::new();
            for i in 0..num_segments {
                let segment = self.context.full_get_segment_text(i)
                    .context("Failed to get segment text")?;
                result.push_str(&segment);
            }

            let text = result.trim().to_string();

            if text.is_empty() {
                info!("❌ No speech detected in audio");
                Ok(None)
            } else {
                info!("✅ Local transcription successful: \"{}\"", text);
                Ok(Some(text))
            }
        }

        #[cfg(not(feature = "local"))]
        {
            let _ = audio_path;
            Err(anyhow::anyhow!(
                "Local backend not available. Build with --features local"
            ))
        }
    }
}

#[cfg(feature = "local")]
fn download_model(model_name: &str, model_path: &str) -> Result<()> {
    // Implement model downloading from Hugging Face or other sources
    // This would download the appropriate .bin file for the model
    todo!("Implement model downloading")
}

#[cfg(feature = "local")]
fn load_audio_file<P: AsRef<Path>>(audio_path: P) -> Result<Vec<f32>> {
    // Load and convert audio file to 16kHz mono f32 format
    // This would use hound or similar to read the WAV file
    todo!("Implement audio file loading and conversion")
}
```

### 3. Update Backend Enum

Update `src/stt/mod.rs`:

```rust
mod local;  // Uncomment this line
pub use local::LocalSttBackend;  // Add this line

pub enum SttBackend {
    Api(ApiSttBackend),
    Local(LocalSttBackend),  // Uncomment this line
}

impl SttBackend {
    pub fn is_configured(&self) -> bool {
        match self {
            SttBackend::Api(backend) => backend.is_configured(),
            SttBackend::Local(backend) => backend.is_configured(),  // Uncomment
        }
    }

    pub fn model(&self) -> &str {
        match self {
            SttBackend::Api(backend) => backend.model(),
            SttBackend::Local(backend) => backend.model(),  // Uncomment
        }
    }

    pub async fn transcribe<P: AsRef<Path>>(&self, audio_path: P) -> Result<Option<String>> {
        match self {
            SttBackend::Api(backend) => backend.transcribe(audio_path).await,
            SttBackend::Local(backend) => backend.transcribe(audio_path).await,  // Uncomment
        }
    }
}

impl SttProcessor {
    pub fn new(config: &Config) -> Result<Self> {
        let backend = match config.whisper.backend.as_str() {
            "api" => {
                info!("Using OpenAI Whisper API backend");
                SttBackend::Api(ApiSttBackend::new(config)?)
            }
            "local" => {
                info!("Using local Whisper backend");
                SttBackend::Local(LocalSttBackend::new(config)?)  // Update this line
            }
            // ... rest unchanged
        };
        // ... rest unchanged
    }
}
```

### 4. Update Configuration

The configuration structure is already ready! Users can set:

```yaml
whisper:
  backend: "local"  # Switch to local backend
  model: "tiny.en"  # Local model name
  model_path: null  # Auto-download to cache dir
  download_models: true
  device: "auto"    # "cpu", "cuda", or "auto"
  language: "en"
```

### 5. Build with Local Support

```bash
# Build with local transcription support
cargo build --features local

# Or set as default
cargo build --features default,local
```

## Available Models

Local models that could be supported:
- `tiny`, `tiny.en` - ~39 MB, fastest
- `base`, `base.en` - ~74 MB, good balance
- `small`, `small.en` - ~244 MB, better accuracy
- `medium`, `medium.en` - ~769 MB, high accuracy
- `large` - ~1550 MB, best accuracy

## Benefits of Local Transcription

✅ **Privacy**: Audio never leaves your machine
✅ **Speed**: No network latency
✅ **Offline**: Works without internet
✅ **Cost**: No API costs
✅ **Reliability**: No rate limits or API downtime

## Next Steps

1. Choose implementation approach (whisper-rs recommended)
2. Implement audio file loading and conversion
3. Add model downloading functionality
4. Test with different model sizes
5. Add device selection (CPU/GPU)
6. Optimize for performance

The architecture is ready - just need to implement the local backend!
