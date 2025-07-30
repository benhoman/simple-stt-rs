use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, StreamConfig};
use std::sync::mpsc::Sender;
use tracing::{info, warn};

use crate::config::{AudioConfig, Config};

pub struct AudioRecorder {
    config: AudioConfig,
    device: Device,
    stream: Option<cpal::Stream>,
}

pub struct AudioData {
    pub samples: Vec<f32>,
    pub level: f32,
}

impl AudioRecorder {
    pub fn new(config: &Config) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No input device available")?;
        info!("Using audio device: {}", device.name().unwrap_or_default());

        Ok(Self {
            config: config.audio.clone(),
            device,
            stream: None,
        })
    }

    pub fn device_name(&self) -> String {
        self.device.name().unwrap_or_else(|e| {
            warn!("Failed to get device name: {}", e);
            "Unknown Device".to_string()
        })
    }

    pub fn start_recording(&mut self, audio_tx: Sender<AudioData>) -> Result<()> {
        // Stop any existing stream
        self.stop_recording();

        let config = StreamConfig {
            channels: self.config.channels,
            sample_rate: SampleRate(self.config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(self.config.chunk_size as u32),
        };

        let stream = self.device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let level = calculate_rms(data);
                if audio_tx
                    .send(AudioData {
                        samples: data.to_vec(),
                        level,
                    })
                    .is_err()
                {
                    warn!("Failed to send audio data to TUI");
                }
            },
            |err| {
                warn!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop_recording(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.pause().ok();
        }
    }
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt() * 100.0
}
