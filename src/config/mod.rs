use anyhow::{Context, Result};
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};

const APP_NAME: &str = "simple-stt";
const CONFIG_FILE: &str = "config.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_size: usize,
    pub silence_threshold: f32,
    pub silence_duration: f64,
    pub max_recording_time: f64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            chunk_size: 2048,
            silence_threshold: 15.0,
            silence_duration: 2.0,
            max_recording_time: 120.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    pub backend: String, // "api" or "local"
    pub api_key: Option<String>,
    pub model: String,
    pub language: Option<String>,
    pub timeout: u64,

    // Local-specific options
    pub model_path: Option<String>,
    pub download_models: bool,
    pub device: String, // "auto", "cpu", "cuda"
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            backend: "local".to_string(), // Default to local - better UX, no API keys needed
            api_key: None,
            model: "tiny.en".to_string(), // Use local model name for local backend
            language: Some("en".to_string()), // Set default language for better accuracy
            timeout: 60,
            model_path: None, // Will use default cache directory
            download_models: true,
            device: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProfile {
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub default_profile: String,
    pub profiles: HashMap<String, LlmProfile>,
    pub api_key: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();

        profiles.insert(
            "general".to_string(),
            LlmProfile {
                name: "General Text Cleanup".to_string(),
                prompt: "Please clean up and format this transcribed text, fixing any grammar issues and making it more readable. It is extremely important to maintain the original meaning and not add any additional information:".to_string(),
            },
        );

        profiles.insert(
            "todo".to_string(),
            LlmProfile {
                name: "Todo/Task".to_string(),
                prompt: "Convert this speech into a clear, actionable todo item or task description. Make it specific, concise, and action-oriented. Use bullet points (markdown format) if multiple tasks are mentioned:".to_string(),
            },
        );

        profiles.insert(
            "email".to_string(),
            LlmProfile {
                name: "Email Format".to_string(),
                prompt: "Format this transcribed text as a professional email. Fix grammar, structure sentences properly, and ensure appropriate tone:".to_string(),
            },
        );

        profiles.insert(
            "slack".to_string(),
            LlmProfile {
                name: "Slack Message".to_string(),
                prompt: "Format this transcribed text as a clear, concise Slack message. Keep it casual but professional, fix any grammar issues:".to_string(),
            },
        );

        Self {
            provider: "openai".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            max_tokens: 500,
            default_profile: "general".to_string(),
            profiles,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardConfig {
    pub auto_paste: bool,
    pub paste_delay: f64,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            auto_paste: false,
            paste_delay: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub enabled: bool,
    pub position_x: u32,
    pub position_y: u32,
    pub auto_hide_delay: f64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position_x: 50,
            position_y: 50,
            auto_hide_delay: 3.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub audio: AudioConfig,
    pub whisper: WhisperConfig,
    pub llm: LlmConfig,
    pub clipboard: ClipboardConfig,
    pub ui: UiConfig,
}

impl Config {
    /// Load configuration from XDG config directory
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            info!(
                "Configuration file not found, creating default: {:?}",
                config_path
            );
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {config_path:?}"))?;

        let mut config: Self =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse YAML configuration")?;

        // Override with environment variables
        config.apply_env_overrides();

        debug!("Configuration loaded from: {:?}", config_path);
        Ok(config)
    }

    /// Save configuration to XDG config directory
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {parent:?}"))?;
        }

        let content =
            serde_yaml::to_string(self).with_context(|| "Failed to serialize configuration")?;

        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {config_path:?}"))?;

        debug!("Configuration saved to: {:?}", config_path);
        Ok(())
    }

    /// Get the configuration file path using XDG config directory
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = config_dir().context("Could not determine config directory")?;

        Ok(config_dir.join(APP_NAME).join(CONFIG_FILE))
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            self.whisper.api_key = Some(api_key.clone());
            self.llm.api_key = Some(api_key);
            debug!("Using OPENAI_API_KEY from environment");
        }

        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            if self.llm.provider == "anthropic" {
                self.llm.api_key = Some(api_key);
                debug!("Using ANTHROPIC_API_KEY from environment");
            }
        }
    }

    /// Get a nested configuration value by dot notation
    pub fn get_nested(&self, key: &str) -> Option<String> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["audio", "silence_threshold"] => Some(self.audio.silence_threshold.to_string()),
            ["audio", "silence_duration"] => Some(self.audio.silence_duration.to_string()),
            ["audio", "max_recording_time"] => Some(self.audio.max_recording_time.to_string()),
            ["whisper", "model"] => Some(self.whisper.model.clone()),
            ["llm", "provider"] => Some(self.llm.provider.clone()),
            ["llm", "model"] => Some(self.llm.model.clone()),
            ["llm", "default_profile"] => Some(self.llm.default_profile.clone()),
            ["clipboard", "auto_paste"] => Some(self.clipboard.auto_paste.to_string()),
            _ => None,
        }
    }

    /// Update the silence threshold and save
    pub fn update_silence_threshold(&mut self, threshold: f32) -> Result<()> {
        self.audio.silence_threshold = threshold;
        self.save()
    }
}
