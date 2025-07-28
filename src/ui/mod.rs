use anyhow::Result;
use std::time::Instant;
use tracing::{debug, info};

use crate::config::{Config, UiConfig};

pub struct UiManager {
    config: UiConfig,
    start_time: Option<Instant>,
    current_status: String,
}

impl UiManager {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.ui.clone(),
            start_time: None,
            current_status: "Ready".to_string(),
        }
    }

    /// Start the UI and show initial status
    pub fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            debug!("UI disabled in configuration");
            return Ok(());
        }

        self.start_time = Some(Instant::now());
        self.set_status("🚀 Starting...", "#00aaff");
        info!("UI started");
        Ok(())
    }

    /// Set the current status message
    pub fn set_status(&mut self, message: &str, _color: &str) {
        self.current_status = message.to_string();

        if self.config.enabled {
            let elapsed = self
                .start_time
                .map(|start| start.elapsed().as_secs_f32())
                .unwrap_or(0.0);

            println!("[{:6.1}s] {}", elapsed, message);
        }
    }

    /// Update recording status
    pub fn start_recording(&mut self, profile: Option<&str>) {
        let message = if let Some(profile) = profile {
            format!("🎤 Recording (profile: {})...", profile)
        } else {
            "🎤 Recording...".to_string()
        };
        self.set_status(&message, "#ff6600");
    }

    /// Update when recording stops
    pub fn stop_recording(&mut self) {
        self.set_status("⏹️ Recording stopped", "#ffaa00");
    }

    /// Show model loading status
    pub fn set_model_loading(&mut self) {
        self.set_status("⏳ Loading speech recognition model...", "#ffaa00");
    }

    /// Show model ready status
    pub fn set_model_ready(&mut self) {
        self.set_status("✅ Model ready", "#00ff00");
    }

    /// Show transcription status
    pub fn set_transcribing(&mut self) {
        self.set_status("🔄 Transcribing audio...", "#00aaff");
    }

    /// Show refinement status
    pub fn set_refining(&mut self, profile: Option<&str>) {
        let message = if let Some(profile) = profile {
            format!("🔄 Refining text (profile: {})...", profile)
        } else {
            "🔄 Refining text...".to_string()
        };
        self.set_status(&message, "#00aaff");
    }

    /// Show completion status
    pub fn set_completed(&mut self, copied_to_clipboard: bool) {
        let message = if copied_to_clipboard {
            "✅ Text copied to clipboard!"
        } else {
            "✅ Text pasted to active window!"
        };
        self.set_status(message, "#00ff00");
    }

    /// Show error status
    pub fn set_error(&mut self, error: &str) {
        self.set_status(&format!("❌ {}", error), "#ff4444");
    }

    /// Show warning status
    pub fn set_warning(&mut self, warning: &str) {
        self.set_status(&format!("⚠️ {}", warning), "#ffaa00");
    }

    /// Clean up UI resources
    pub fn cleanup(&mut self) {
        if self.config.enabled && self.config.auto_hide_delay > 0.0 {
            debug!(
                "UI will auto-hide after {} seconds",
                self.config.auto_hide_delay
            );
        }
    }

    /// Check if UI is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the current status
    pub fn current_status(&self) -> &str {
        &self.current_status
    }

    /// Get elapsed time since start
    pub fn elapsed_time(&self) -> Option<f32> {
        self.start_time.map(|start| start.elapsed().as_secs_f32())
    }
}

impl Drop for UiManager {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_manager_creation() {
        let config = Config::default();
        let ui = UiManager::new(&config);
        assert!(ui.is_enabled());
        assert_eq!(ui.current_status(), "Ready");
    }

    #[test]
    fn test_ui_manager_disabled() {
        let mut config = Config::default();
        config.ui.enabled = false;
        let ui = UiManager::new(&config);
        assert!(!ui.is_enabled());
    }

    #[test]
    fn test_status_updates() {
        let config = Config::default();
        let mut ui = UiManager::new(&config);

        ui.set_status("Test", "#ffffff");
        assert_eq!(ui.current_status(), "Test");

        ui.start_recording(Some("test-profile"));
        assert!(ui.current_status().contains("test-profile"));
    }
}
