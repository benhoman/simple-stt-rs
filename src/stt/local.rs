use anyhow::{Context, Result};
use hf_hub::api::tokio::Api;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::config::{Config, WhisperConfig};

pub struct LocalSttBackend {
    config: WhisperConfig,
    context: Option<WhisperContext>,
    preparation_status: PreparationStatus,
}

#[derive(Debug, Clone)]
enum PreparationStatus {
    NotStarted,
    InProgress,
    Ready,
    Failed(String),
}

impl LocalSttBackend {
    /// Create a new LocalSttBackend instance without loading the model
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            config: config.whisper.clone(),
            context: None,
            preparation_status: PreparationStatus::NotStarted,
        })
    }

    /// Prepare the backend by downloading and loading the model
    pub async fn prepare(&mut self) -> Result<()> {
        if matches!(self.preparation_status, PreparationStatus::Ready) {
            return Ok(()); // Already prepared
        }

        self.preparation_status = PreparationStatus::InProgress;
        info!("🔄 Preparing local Whisper backend...");

        let model_path = get_model_path(&self.config)?;

        // Check if model exists
        if !model_path.exists() {
            if self.config.download_models {
                info!("Whisper model not found at {:?}", model_path);
                info!("🔄 Downloading Whisper model: {}", self.config.model);

                // Create model directory if it doesn't exist
                if let Some(parent) = model_path.parent() {
                    std::fs::create_dir_all(parent).context("Failed to create model directory")?;
                }

                // Download the model
                download_model(&self.config.model, &model_path)
                    .await
                    .with_context(|| format!("Failed to download model: {}", self.config.model))?;

                info!("✅ Model downloaded successfully: {:?}", model_path);
            } else {
                let error_msg = format!(
                    "Whisper model not found at {model_path:?} and download_models is disabled"
                );
                warn!("{}", error_msg);
                self.preparation_status = PreparationStatus::Failed(error_msg.clone());
                return Err(anyhow::anyhow!(error_msg));
            }
        }

        info!("Loading Whisper model from: {:?}", model_path);

        // Load the model (this can be slow, so we do it during preparation)
        let ctx_params = WhisperContextParameters::default();

        match WhisperContext::new_with_params(model_path.to_string_lossy().as_ref(), ctx_params) {
            Ok(context) => {
                info!("✅ Whisper model loaded successfully");
                self.context = Some(context);
                self.preparation_status = PreparationStatus::Ready;
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to load Whisper model: {e}");
                warn!("{}", error_msg);
                warn!("Local backend will be unavailable");
                info!("💡 Try downloading the model manually or check the file path");
                self.preparation_status = PreparationStatus::Failed(error_msg.clone());
                Err(anyhow::anyhow!(error_msg))
            }
        }
    }

    /// Check if the backend is ready for transcription
    pub fn is_configured(&self) -> bool {
        matches!(self.preparation_status, PreparationStatus::Ready) && self.context.is_some()
    }

    /// Check if the backend is currently being prepared
    pub fn is_preparing(&self) -> bool {
        matches!(self.preparation_status, PreparationStatus::InProgress)
    }

    /// Check if preparation failed
    pub fn preparation_failed(&self) -> Option<&str> {
        match &self.preparation_status {
            PreparationStatus::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub async fn transcribe<P: AsRef<Path>>(&self, audio_path: P) -> Result<Option<String>> {
        let audio_path = audio_path.as_ref();

        if !audio_path.exists() {
            return Err(anyhow::anyhow!("Audio file not found: {:?}", audio_path));
        }

        let context = match &self.context {
            Some(ctx) => ctx,
            None => {
                return Err(anyhow::anyhow!(
                    "Local transcription not available - model not loaded. Check logs for details."
                ));
            }
        };

        info!("🔄 Transcribing audio file locally: {:?}", audio_path);

        // Convert audio to required format (16kHz mono f32)
        let audio_data = load_audio_file(audio_path).await?;

        if audio_data.is_empty() {
            warn!("Audio file appears to be empty or invalid");
            return Ok(None);
        }

        debug!("Audio data loaded: {} samples", audio_data.len());

        // Use the prepared context directly (no need for spawn_blocking since context is already loaded)
        let language = self.config.language.clone();

        // Setup transcription parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        if let Some(ref lang) = language {
            params.set_language(Some(lang));
        }

        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true); // Disable context from previous transcriptions
        params.set_single_segment(false); // Allow multiple segments

        debug!("Running Whisper transcription...");

        // Run transcription using the prepared context
        let mut state = context
            .create_state()
            .context("Failed to create whisper state")?;
        state
            .full(params, &audio_data)
            .context("Failed to run Whisper transcription")?;

        // Extract text using the state
        let num_segments = state
            .full_n_segments()
            .context("Failed to get number of segments")?;

        debug!("Transcription completed: {} segments", num_segments);

        let mut result = String::new();
        for i in 0..num_segments {
            let segment = state
                .full_get_segment_text(i)
                .context("Failed to get segment text")?;

            debug!("Raw segment {}: \"{}\"", i, segment);

            // Filter out Whisper special tokens and unwanted content
            let cleaned_segment = clean_whisper_output(&segment);
            if !cleaned_segment.is_empty() {
                result.push_str(&cleaned_segment);
                debug!("Added cleaned segment {}: \"{}\"", i, cleaned_segment);
            } else {
                debug!("Filtered out segment {}: \"{}\"", i, segment);
            }
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
}

/// Download a Whisper model from Hugging Face
async fn download_model(model_name: &str, model_path: &Path) -> Result<()> {
    info!("📥 Downloading {} from Hugging Face...", model_name);

    // Initialize Hugging Face API
    let api = Api::new()?;
    let repo = api.model("ggerganov/whisper.cpp".to_string());

    // Model filename on Hugging Face
    let filename = format!("ggml-{model_name}.bin");

    info!("🌐 Fetching model file: {}", filename);

    // Download the model file
    let model_file = repo
        .get(&filename)
        .await
        .with_context(|| format!("Failed to download model file: {filename}"))?;

    // Copy the downloaded file to the target location
    debug!("💾 Saving model to: {:?}", model_path);
    tokio::fs::copy(&model_file, &model_path)
        .await
        .context("Failed to save model file")?;

    // Verify the file was downloaded correctly
    let metadata = tokio::fs::metadata(&model_path)
        .await
        .context("Failed to verify downloaded model")?;

    info!(
        "✅ Model downloaded successfully: {:.1} MB",
        metadata.len() as f64 / 1024.0 / 1024.0
    );

    Ok(())
}

/// Get the path where the model should be located
fn get_model_path(config: &WhisperConfig) -> Result<PathBuf> {
    if let Some(ref path) = config.model_path {
        let expanded = shellexpand::tilde(path);
        Ok(PathBuf::from(expanded.as_ref()))
    } else {
        // Default model path in cache directory
        let cache_dir = dirs::cache_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
            .unwrap_or_else(std::env::temp_dir);

        let model_dir = cache_dir.join("simple-stt").join("models");
        let model_file = format!("ggml-{}.bin", config.model);

        Ok(model_dir.join(model_file))
    }
}

/// Load and convert audio file to the format required by Whisper (16kHz mono f32)
async fn load_audio_file<P: AsRef<Path>>(audio_path: P) -> Result<Vec<f32>> {
    let audio_path = audio_path.as_ref();

    debug!("Loading audio file: {:?}", audio_path);

    // Use hound to read the WAV file
    let reader = hound::WavReader::open(audio_path).context("Failed to open audio file")?;

    let spec = reader.spec();
    debug!("Audio spec: {:?}", spec);

    // Read samples based on the bit depth
    let samples: Result<Vec<f32>, _> = match spec.bits_per_sample {
        16 => reader
            .into_samples::<i16>()
            .map(|s| s.map(|sample| sample as f32 / 32768.0))
            .collect(),
        32 => {
            if spec.sample_format == hound::SampleFormat::Float {
                reader.into_samples::<f32>().collect()
            } else {
                reader
                    .into_samples::<i32>()
                    .map(|s| s.map(|sample| sample as f32 / 2147483648.0))
                    .collect()
            }
        }
        24 => {
            // 24-bit samples are stored as i32 but only use 24 bits
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|sample| (sample >> 8) as f32 / 8388608.0))
                .collect()
        }
        8 => {
            // Convert 8-bit unsigned to signed first
            reader
                .into_samples::<i8>()
                .map(|s| s.map(|sample| sample as f32 / 128.0))
                .collect()
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported bit depth: {} bits",
                spec.bits_per_sample
            ));
        }
    };

    let mut samples = samples.context("Failed to read audio samples")?;

    debug!("Read {} samples", samples.len());

    // Convert stereo to mono if necessary
    if spec.channels == 2 {
        debug!("Converting stereo to mono");
        samples = samples
            .chunks(2)
            .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
            .collect();
    } else if spec.channels != 1 {
        return Err(anyhow::anyhow!(
            "Unsupported number of channels: {}",
            spec.channels
        ));
    }

    // Resample to 16kHz if necessary
    if spec.sample_rate != 16000 {
        debug!("Resampling from {} Hz to 16000 Hz", spec.sample_rate);
        samples = resample_audio(samples, spec.sample_rate, 16000)?;
    }

    debug!("Final audio: {} samples at 16kHz mono", samples.len());

    Ok(samples)
}

/// Simple linear resampling (not high quality, but sufficient for speech)
fn resample_audio(input: Vec<f32>, input_rate: u32, output_rate: u32) -> Result<Vec<f32>> {
    if input_rate == output_rate {
        return Ok(input);
    }

    let ratio = input_rate as f64 / output_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_index = (i as f64 * ratio) as usize;
        if src_index < input.len() {
            output.push(input[src_index]);
        } else {
            output.push(0.0);
        }
    }

    Ok(output)
}

/// Clean Whisper output by removing special tokens and unwanted markers
fn clean_whisper_output(text: &str) -> String {
    let text = text.trim();

    // List of Whisper special tokens to filter out
    let unwanted_tokens = [
        "[BLANK_AUDIO]",
        "[blank_audio]",
        "[MUSIC]",
        "[music]",
        "[NOISE]",
        "[noise]",
        "[SILENCE]",
        "[silence]",
        "[SPEAKING]",
        "[speaking]",
        "[SOUND]",
        "[sound]",
        "[BEEP]",
        "[beep]",
        "[APPLAUSE]",
        "[applause]",
        "[LAUGHTER]",
        "[laughter]",
        "[COUGH]",
        "[cough]",
        "(blank)",
        "(BLANK)",
        "(no audio)",
        "(NO AUDIO)",
        "inaudible",
        "INAUDIBLE",
    ];

    // Check if the entire segment is just a special token
    for token in &unwanted_tokens {
        if text.eq_ignore_ascii_case(token) {
            return String::new(); // Return empty string for pure special tokens
        }
    }

    // Remove special tokens that appear within text
    let mut cleaned = text.to_string();
    for token in &unwanted_tokens {
        // Remove exact matches (case insensitive)
        cleaned = cleaned.replace(token, "");
        cleaned = cleaned.replace(&token.to_lowercase(), "");
        cleaned = cleaned.replace(&token.to_uppercase(), "");
    }

    // Clean up extra whitespace and common artifacts
    cleaned = cleaned
        .replace("  ", " ") // Multiple spaces
        .replace(" ,", ",") // Space before comma
        .replace(" .", ".") // Space before period
        .replace(" ?", "?") // Space before question mark
        .replace(" !", "!") // Space before exclamation
        .trim() // Leading/trailing whitespace
        .to_string();

    // Filter out very short segments that are likely artifacts
    if cleaned.len() < 2 {
        return String::new();
    }

    cleaned
}
