use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info, warn};

use simple_stt_rs::config::{Config, LlmConfig, LlmProfile};

pub struct LlmRefiner {
    config: LlmConfig,
    client: reqwest::Client,
}

impl LlmRefiner {
    pub fn new(config: &Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            config: config.llm.clone(),
            client,
        })
    }

    /// Refine text using the configured LLM provider
    pub async fn refine_text(&self, text: &str, profile: Option<&str>) -> Result<Option<String>> {
        if !self.is_configured() {
            debug!("LLM not configured, returning original text");
            return Ok(Some(text.to_string()));
        }

        let profile_name = profile.unwrap_or(&self.config.default_profile);
        let profile_data = self.config.profiles.get(profile_name);

        let profile_data = match profile_data {
            Some(profile) => profile,
            None => {
                warn!("Profile '{}' not found, using original text", profile_name);
                return Ok(Some(text.to_string()));
            }
        };

        info!("🔄 Refining text with LLM using profile: {}", profile_name);
        debug!("Profile prompt: {}", profile_data.prompt);

        match self.config.provider.as_str() {
            "openai" => self.refine_with_openai(text, profile_data).await,
            "anthropic" => self.refine_with_anthropic(text, profile_data).await,
            provider => {
                warn!(
                    "Unsupported LLM provider '{}', using original text",
                    provider
                );
                Ok(Some(text.to_string()))
            }
        }
    }

    /// Refine text using OpenAI API
    async fn refine_with_openai(&self, text: &str, profile: &LlmProfile) -> Result<Option<String>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .context("OpenAI API key not configured")?;

        let payload = json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "system",
                    "content": profile.prompt
                },
                {
                    "role": "user",
                    "content": text
                }
            ],
            "max_tokens": self.config.max_tokens,
            "temperature": 0.3
        });

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .context("Failed to send OpenAI request")?;

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
            .context("Failed to parse OpenAI response")?;

        let refined_text = result
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .map(|s| s.trim().to_string())
            .context("No content found in OpenAI response")?;

        if refined_text.is_empty() {
            warn!("OpenAI returned empty response");
            Ok(None)
        } else {
            info!("✅ Text refined successfully: \"{}\"", refined_text);
            Ok(Some(refined_text))
        }
    }

    /// Refine text using Anthropic Claude API
    async fn refine_with_anthropic(
        &self,
        text: &str,
        profile: &LlmProfile,
    ) -> Result<Option<String>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .context("Anthropic API key not configured")?;

        let payload = json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "messages": [
                {
                    "role": "user",
                    "content": format!("{}\n\nText to process: {}", profile.prompt, text)
                }
            ]
        });

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(api_key)?);
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Anthropic request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Anthropic API request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let result: Value = response
            .json()
            .await
            .context("Failed to parse Anthropic response")?;

        let refined_text = result
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .map(|s| s.trim().to_string())
            .context("No content found in Anthropic response")?;

        if refined_text.is_empty() {
            warn!("Anthropic returned empty response");
            Ok(None)
        } else {
            info!("✅ Text refined successfully: \"{}\"", refined_text);
            Ok(Some(refined_text))
        }
    }

    /// Check if LLM is configured
    pub fn is_configured(&self) -> bool {
        self.config.api_key.is_some()
    }

    /// Get the configured provider
    pub fn provider(&self) -> &str {
        &self.config.provider
    }

    /// Get the configured model
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// List available profiles
    pub fn list_profiles(&self) -> &std::collections::HashMap<String, LlmProfile> {
        &self.config.profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_refiner_creation() {
        let config = Config::default();
        let refiner = LlmRefiner::new(&config);
        assert!(refiner.is_ok());
    }

    #[test]
    fn test_provider_getter() {
        let config = Config::default();
        let refiner = LlmRefiner::new(&config).unwrap();
        assert_eq!(refiner.provider(), "openai");
    }

    #[test]
    fn test_model_getter() {
        let config = Config::default();
        let refiner = LlmRefiner::new(&config).unwrap();
        assert_eq!(refiner.model(), "gpt-3.5-turbo");
    }

    #[test]
    fn test_is_configured() {
        let mut config = Config::default();
        let refiner = LlmRefiner::new(&config).unwrap();
        assert!(!refiner.is_configured());

        config.llm.api_key = Some("test-key".to_string());
        let refiner = LlmRefiner::new(&config).unwrap();
        assert!(refiner.is_configured());
    }

    #[test]
    fn test_list_profiles() {
        let config = Config::default();
        let refiner = LlmRefiner::new(&config).unwrap();
        let profiles = refiner.list_profiles();

        assert!(profiles.contains_key("general"));
        assert!(profiles.contains_key("todo"));
        assert!(profiles.contains_key("email"));
        assert!(profiles.contains_key("slack"));
    }
}
