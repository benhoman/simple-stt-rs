use anyhow::Result;
use std::path::Path;
use tokio::sync::mpsc::Sender as TokioSender;
use tracing::info; // New: Import TokioSender

use crate::config::{Config, WhisperConfig};
use crate::stt::api::ApiSttBackend;
use crate::stt::local::LocalSttBackend;

mod api;
mod local;

pub mod wav_utils;

/// Enum representing different STT backend implementations
pub enum SttBackend {
    Api(ApiSttBackend),
    Local(LocalSttBackend),
}

impl SttBackend {
    /// Prepare the backend for transcription (download models, etc.)
    pub async fn prepare(&mut self) -> Result<()> {
        match self {
            SttBackend::Api(_) => {
                // API backend doesn't need preparation
                Ok(())
            }
            SttBackend::Local(backend) => backend.prepare().await,
        }
    }

    /// Check if this backend is properly configured and ready
    pub fn is_configured(&self) -> bool {
        match self {
            SttBackend::Api(backend) => backend.is_configured(),
            SttBackend::Local(backend) => backend.is_configured(),
        }
    }

    /// Check if the backend is currently being prepared
    pub fn is_preparing(&self) -> bool {
        match self {
            SttBackend::Api(_) => false, // API backend is always ready
            SttBackend::Local(backend) => backend.is_preparing(),
        }
    }

    /// Get preparation error if any
    pub fn preparation_failed(&self) -> Option<&str> {
        match self {
            SttBackend::Api(_) => None,
            SttBackend::Local(backend) => backend.preparation_failed(),
        }
    }

    /// Get the model name being used
    pub fn model(&self) -> &str {
        match self {
            SttBackend::Api(backend) => backend.model(),
            SttBackend::Local(backend) => backend.model(),
        }
    }

    /// Transcribe an audio file
    pub async fn transcribe<P: AsRef<Path>>(
        &self,
        audio_path: P,
        log_tx: Option<TokioSender<String>>,
    ) -> Result<Option<String>> {
        match self {
            SttBackend::Api(backend) => backend.transcribe(audio_path, log_tx).await,
            SttBackend::Local(backend) => backend.transcribe(audio_path, log_tx).await,
        }
    }
}

pub struct SttProcessor {
    backend: SttBackend,
    config: WhisperConfig,
}

impl SttProcessor {
    /// Create a new SttProcessor without preparing the backend
    pub fn new(config: &Config) -> Result<Self> {
        let backend = match config.whisper.backend.as_str() {
            "api" => {
                info!("Using OpenAI Whisper API backend");
                SttBackend::Api(ApiSttBackend::new(config)?)
            }
            "local" => {
                info!("Using local Whisper backend");
                SttBackend::Local(LocalSttBackend::new(config)?)
            }
            backend => {
                return Err(anyhow::anyhow!("Unknown STT backend: {}", backend));
            }
        };

        Ok(Self {
            backend,
            config: config.whisper.clone(),
        })
    }

    /// Prepare the backend for transcription (download models, etc.)
    /// This can be called in parallel with audio recording
    pub async fn prepare(&mut self) -> Result<()> {
        self.backend.prepare().await
    }

    /// Transcribe audio file using the configured backend
    pub async fn transcribe<P: AsRef<Path>>(
        &self,
        audio_path: P,
        log_tx: Option<TokioSender<String>>,
    ) -> Result<Option<String>> {
        self.backend.transcribe(audio_path, log_tx).await
    }

    /// Check if the backend is configured and ready
    pub fn is_configured(&self) -> bool {
        self.backend.is_configured()
    }

    /// Check if the backend is currently being prepared
    pub fn is_preparing(&self) -> bool {
        self.backend.is_preparing()
    }

    /// Get preparation error if any
    pub fn preparation_failed(&self) -> Option<&str> {
        self.backend.preparation_failed()
    }

    /// Get the configured model name
    pub fn model(&self) -> &str {
        self.backend.model()
    }

    /// Get the backend type
    pub fn backend_type(&self) -> &str {
        &self.config.backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_stt_processor_creation_api() {
        let mut config = Config::default();
        config.whisper.backend = "api".to_string();
        let processor = SttProcessor::new(&config);
        assert!(processor.is_ok());
    }

    #[tokio::test]
    async fn test_stt_processor_creation_local() {
        let mut config = Config::default();
        config.whisper.backend = "local".to_string();
        let processor = SttProcessor::new(&config);
        // Should succeed now but backend may not be configured without model
        assert!(processor.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_backend() {
        let mut config = Config::default();
        config.whisper.backend = "unknown".to_string();
        let processor = SttProcessor::new(&config);
        assert!(processor.is_err());
    }
}
