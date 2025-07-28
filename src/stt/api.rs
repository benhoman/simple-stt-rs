use anyhow::{Context, Result};
use reqwest::multipart;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::info;

use crate::config::{Config, WhisperConfig};

pub struct ApiSttBackend {
    config: WhisperConfig,
    client: reqwest::Client,
}

impl ApiSttBackend {
    pub fn new(config: &Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.whisper.timeout))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            config: config.whisper.clone(),
            client,
        })
    }

    pub fn is_configured(&self) -> bool {
        self.config.api_key.is_some()
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub async fn transcribe<P: AsRef<Path>>(&self, audio_path: P) -> Result<Option<String>> {
        let audio_path = audio_path.as_ref();

        if !audio_path.exists() {
            return Err(anyhow::anyhow!("Audio file not found: {:?}", audio_path));
        }

        let api_key = self.config.api_key
            .as_ref()
            .context("OpenAI API key not configured. Set OPENAI_API_KEY environment variable or configure in config file")?;

        info!(
            "🔄 Transcribing audio file with OpenAI API: {:?}",
            audio_path
        );

        // Read audio file
        let mut file = File::open(audio_path)
            .await
            .context("Failed to open audio file")?;

        let mut audio_data = Vec::new();
        file.read_to_end(&mut audio_data)
            .await
            .context("Failed to read audio file")?;

        // Prepare multipart form
        let part = multipart::Part::bytes(audio_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .context("Failed to set MIME type")?;

        let mut form = multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-1"); // Use API model name

        // Add language if specified
        if let Some(ref language) = self.config.language {
            form = form.text("language", language.clone());
        }

        // Make API request
        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await
            .context("Failed to send transcription request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "OpenAI API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let result: Value = response
            .json()
            .await
            .context("Failed to parse JSON response")?;

        let text = result
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .context("No text found in API response")?;

        if text.is_empty() {
            info!("❌ No speech detected in audio");
            Ok(None)
        } else {
            info!("✅ API transcription successful: \"{}\"", text);
            Ok(Some(text))
        }
    }
}
