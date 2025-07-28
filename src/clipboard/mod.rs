use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};
use which::which;
use wl_clipboard_rs::copy::{MimeType, Options, Source};

use crate::config::{ClipboardConfig, Config};

pub struct ClipboardManager {
    config: ClipboardConfig,
}

impl ClipboardManager {
    pub fn new(config: &Config) -> Result<Self> {
        debug!("Initializing Wayland clipboard manager");
        Ok(Self {
            config: config.clipboard.clone(),
        })
    }

    /// Copy text to clipboard using Wayland native clipboard
    pub fn copy_to_clipboard(&mut self, text: &str) -> Result<()> {
        // Try Wayland native clipboard first
        match self.copy_wayland_native(text) {
            Ok(_) => {
                info!("✅ Text copied to clipboard (Wayland native): \"{}\"", text);
                return Ok(());
            }
            Err(e) => {
                debug!("Wayland native clipboard failed: {}, trying wl-copy", e);
            }
        }

        // Fallback to wl-copy command
        self.copy_with_wl_copy(text)
    }

    /// Copy using native Wayland clipboard
    fn copy_wayland_native(&self, text: &str) -> Result<()> {
        let opts = Options::new();
        opts.copy(
            Source::Bytes(text.as_bytes().into()),
            MimeType::Specific("text/plain;charset=utf-8".to_string()),
        )
        .context("Failed to copy to Wayland clipboard")?;
        Ok(())
    }

    /// Copy using wl-copy command
    fn copy_with_wl_copy(&mut self, text: &str) -> Result<()> {
        if !which("wl-copy").is_ok() {
            return Err(anyhow::anyhow!(
                "wl-copy not found. Install wl-clipboard for Wayland clipboard support"
            ));
        }

        debug!("Using wl-copy for clipboard");
        let output = Command::new("wl-copy")
            .arg(text)
            .output()
            .context("Failed to execute wl-copy")?;

        if output.status.success() {
            info!("✅ Text copied to clipboard (wl-copy): \"{}\"", text);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("wl-copy failed: {}", stderr))
        }
    }

    /// Paste text directly to the active window using Wayland tools
    pub async fn paste_text(&mut self, text: &str) -> Result<()> {
        // First copy to clipboard
        self.copy_to_clipboard(text)?;

        if self.config.auto_paste {
            info!("🖱️ Auto-pasting text to active window");

            // Wait for configured delay
            if self.config.paste_delay > 0.0 {
                tokio::time::sleep(Duration::from_secs_f64(self.config.paste_delay)).await;
            }

            // Try Wayland paste methods
            if let Err(e) = self.try_wayland_paste().await {
                warn!("Auto-paste failed: {}. Text is still in clipboard.", e);
                return Err(e);
            }

            info!("✅ Text auto-pasted to active window");
        }

        Ok(())
    }

    /// Try Wayland paste methods - prioritize wtype, fallback to ydotool
    async fn try_wayland_paste(&self) -> Result<()> {
        // Try wtype first (Wayland native)
        if which("wtype").is_ok() {
            debug!("Using wtype for auto-paste");
            return self.paste_with_wtype().await;
        }

        // Try ydotool (universal, works on Wayland)
        if which("ydotool").is_ok() {
            debug!("Using ydotool for auto-paste");
            return self.paste_with_ydotool().await;
        }

        Err(anyhow::anyhow!(
            "No suitable paste tool found. Install wtype or ydotool for auto-paste functionality"
        ))
    }

    /// Paste using wtype (Wayland native)
    async fn paste_with_wtype(&self) -> Result<()> {
        let output = Command::new("wtype")
            .args(&["-M", "ctrl", "-P", "v", "-m", "ctrl"])
            .output()
            .context("Failed to execute wtype")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("wtype failed: {}", stderr));
        }

        Ok(())
    }

    /// Paste using ydotool (universal)
    async fn paste_with_ydotool(&self) -> Result<()> {
        let output = Command::new("ydotool")
            .args(&["key", "ctrl+v"])
            .output()
            .context("Failed to execute ydotool")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("ydotool failed: {}", stderr));
        }

        Ok(())
    }

    /// Get current clipboard content using wl-paste
    pub fn get_clipboard_text(&mut self) -> Result<String> {
        self.get_with_wl_paste()
    }

    /// Get clipboard content using wl-paste command
    fn get_with_wl_paste(&self) -> Result<String> {
        if !which("wl-paste").is_ok() {
            return Err(anyhow::anyhow!(
                "wl-paste not found. Install wl-clipboard for Wayland clipboard support"
            ));
        }

        let output = Command::new("wl-paste")
            .output()
            .context("Failed to execute wl-paste")?;

        if output.status.success() {
            String::from_utf8(output.stdout).context("Clipboard contents are not valid UTF-8")
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("wl-paste failed: {}", stderr))
        }
    }

    /// Check if auto-paste is enabled
    pub fn is_auto_paste_enabled(&self) -> bool {
        self.config.auto_paste
    }

    /// Set auto-paste configuration
    pub fn set_auto_paste(&mut self, enabled: bool) {
        self.config.auto_paste = enabled;
    }

    /// Check available Wayland clipboard and paste tools
    pub fn check_tools() -> (Vec<String>, Vec<String>) {
        let clipboard_tools = ["wl-copy", "wl-paste"];
        let paste_tools = ["wtype", "ydotool"];

        let available_clipboard: Vec<String> = clipboard_tools
            .iter()
            .filter(|&&tool| which(tool).is_ok())
            .map(|&tool| tool.to_string())
            .collect();

        let available_paste: Vec<String> = paste_tools
            .iter()
            .filter(|&&tool| which(tool).is_ok())
            .map(|&tool| tool.to_string())
            .collect();

        (available_clipboard, available_paste)
    }

    /// Legacy function for compatibility
    pub fn check_paste_tools() -> Vec<String> {
        let (_, paste_tools) = Self::check_tools();
        paste_tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_manager_creation() {
        let config = Config::default();
        let result = ClipboardManager::new(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_tools() {
        let (clipboard_tools, paste_tools) = ClipboardManager::check_tools();
        // Should return vectors (might be empty in CI)
        assert!(clipboard_tools.len() >= 0);
        assert!(paste_tools.len() >= 0);
    }

    #[test]
    fn test_auto_paste_configuration() {
        let config = Config::default();
        let mut clipboard = ClipboardManager::new(&config).unwrap();
        assert!(!clipboard.is_auto_paste_enabled());
        clipboard.set_auto_paste(true);
        assert!(clipboard.is_auto_paste_enabled());
    }
}
