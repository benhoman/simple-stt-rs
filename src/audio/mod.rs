use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleRate, StreamConfig};
use hound::{WavSpec, WavWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tracing::{debug, info, warn};

use crate::config::{AudioConfig, Config};

#[derive(Debug, Clone)]
pub struct AudioSample {
    pub data: Vec<f32>,
    pub timestamp: Instant,
}

pub struct AudioRecorder {
    config: AudioConfig,
    host: Host,
    device: Device,
    sample_rate: u32,
    channels: u16,
    temp_file: Option<PathBuf>,
}

impl AudioRecorder {
    pub fn new(config: &Config) -> Result<Self> {
        let host = cpal::default_host();
        
        let device = host
            .default_input_device()
            .context("No input device available")?;

        info!("Using audio device: {}", device.name().unwrap_or_default());

        let supported_configs_range = device
            .supported_input_configs()
            .context("Error while querying input configs")?;

        debug!("Supported configs:");
        for config_range in supported_configs_range {
            debug!("  {:?}", config_range);
        }

        // Find a suitable config
        let supported_config = device
            .supported_input_configs()
            .context("Error while querying input configs")?
            .find(|config_range| {
                config_range.channels() == config.audio.channels
                    && config_range.min_sample_rate() <= SampleRate(config.audio.sample_rate)
                    && config_range.max_sample_rate() >= SampleRate(config.audio.sample_rate)
            })
            .or_else(|| {
                device
                    .supported_input_configs()
                    .ok()?
                    .next()
            })
            .context("No supported audio configuration found")?;

        let sample_rate = config.audio.sample_rate.min(supported_config.max_sample_rate().0)
            .max(supported_config.min_sample_rate().0);
        
        let channels = supported_config.channels().min(config.audio.channels);

        info!("Audio config: {} Hz, {} channels", sample_rate, channels);

        Ok(Self {
            config: config.audio.clone(),
            host,
            device,
            sample_rate,
            channels,
            temp_file: None,
        })
    }

    /// Record audio until silence is detected
    pub async fn record_until_silence(&mut self) -> Result<Option<PathBuf>> {
        let config = StreamConfig {
            channels: self.channels,
            sample_rate: SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(self.config.chunk_size as u32),
        };

        info!("🎤 Starting recording... speak now, will stop after silence");

        // Create temporary file for recording
        let temp_file = NamedTempFile::new()
            .context("Failed to create temporary file")?;
        let temp_path = temp_file.path().with_extension("wav");
        
        // Create WAV writer
        let spec = WavSpec {
            channels: self.channels,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = Arc::new(Mutex::new(
            WavWriter::create(&temp_path, spec)
                .context("Failed to create WAV writer")?
        ));

        // Shared state
        let recording = Arc::new(AtomicBool::new(true));
        let recording_clone = recording.clone();
        let writer_clone = writer.clone();

        // Audio analysis state
        let silence_start = Arc::new(Mutex::new(None::<Instant>));
        let silence_start_clone = silence_start.clone();

        let silence_threshold = self.config.silence_threshold;
        let silence_duration = Duration::from_secs_f64(self.config.silence_duration);
        let max_recording_time = Duration::from_secs_f64(self.config.max_recording_time);

        let start_time = Instant::now();

        // Error channel for stream errors
        let (error_tx1, error_rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        let error_tx2 = error_tx1.clone();

        // Build the input stream
        let stream = self.device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let now = Instant::now();
                
                // Check max recording time
                if now.duration_since(start_time) > max_recording_time {
                    warn!("⏰ Maximum recording time reached, stopping...");
                    recording_clone.store(false, Ordering::Relaxed);
                    return;
                }

                // Calculate RMS volume
                let rms = calculate_rms(data);
                
                // Check for silence
                let mut silence_start_guard = silence_start_clone.lock().unwrap();
                if rms < silence_threshold {
                    if silence_start_guard.is_none() {
                        *silence_start_guard = Some(now);
                        debug!("Silence detected, started timer");
                    } else if let Some(silence_start_time) = *silence_start_guard {
                        if now.duration_since(silence_start_time) > silence_duration {
                            info!("🔇 Silence detected for required duration, stopping recording");
                            recording_clone.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                } else {
                    if silence_start_guard.is_some() {
                        debug!("Speech resumed, resetting silence timer");
                    }
                    *silence_start_guard = None;
                }

                // Convert f32 samples to i16 and write to file
                let samples_i16: Vec<i16> = data.iter()
                    .map(|&sample| (sample * i16::MAX as f32) as i16)
                    .collect();

                if let Ok(mut writer_guard) = writer_clone.lock() {
                    for &sample in &samples_i16 {
                        if let Err(e) = writer_guard.write_sample(sample) {
                            let _ = error_tx1.send(format!("Failed to write audio sample: {}", e));
                            recording_clone.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                } else {
                    let _ = error_tx1.send("Failed to acquire writer lock".to_string());
                    recording_clone.store(false, Ordering::Relaxed);
                }
            },
            move |err| {
                warn!("Audio stream error: {}", err);
                let _ = error_tx2.send(format!("Audio stream error: {}", err));
            },
            None,
        ).context("Failed to build input stream")?;

        // Start the stream
        stream.play().context("Failed to start audio stream")?;

        // Wait for recording to complete
        while recording.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Check for stream errors
            if let Ok(error) = error_rx.try_recv() {
                return Err(anyhow::anyhow!("Audio recording error: {}", error));
            }
        }

        // Stop the stream and finalize the file
        drop(stream);
        
        // Finalize the WAV file by dropping the writer
        drop(writer);

        // Check if we recorded any meaningful audio
        let file_size = std::fs::metadata(&temp_path)
            .context("Failed to get file metadata")?
            .len();

        if file_size < 1000 { // Less than 1KB probably means no audio
            warn!("❌ No meaningful audio recorded");
            std::fs::remove_file(&temp_path).ok();
            return Ok(None);
        }

        // Keep reference to temp file
        self.temp_file = Some(temp_path.clone());
        
        info!("✅ Recording completed: {:.2} KB", file_size as f64 / 1024.0);
        Ok(Some(temp_path))
    }

    /// Tune the silence threshold by analyzing ambient noise and speech
    pub async fn tune_silence_threshold(&mut self, duration_seconds: u64) -> Result<Option<f32>> {
        info!("🎯 Starting silence threshold tuning for {} seconds", duration_seconds);
        println!("🎯 Starting silence threshold tuning...");
        println!("This will record for {} seconds. Follow the prompts below:", duration_seconds);
        println!();

        let config = StreamConfig {
            channels: self.channels,
            sample_rate: SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(self.config.chunk_size as u32),
        };

        // Collect volume samples
        let volumes = Arc::new(Mutex::new(Vec::new()));
        let volumes_clone = volumes.clone();

        let recording = Arc::new(AtomicBool::new(true));
        let recording_clone = recording.clone();

        let silence_time = 3; // First 3 seconds for silence
        let speech_time = duration_seconds - silence_time; // Remaining time for speech

        let start_time = Instant::now();

        println!("🔇 First, stay SILENT for {} seconds...", silence_time);

        // Build the input stream for tuning
        let stream = self.device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let now = Instant::now();
                let elapsed = now.duration_since(start_time);
                
                if elapsed.as_secs() >= duration_seconds {
                    recording_clone.store(false, Ordering::Relaxed);
                    return;
                }

                let rms = calculate_rms(data);
                
                if let Ok(mut volumes_guard) = volumes_clone.lock() {
                    volumes_guard.push((rms, elapsed));
                }
            },
            move |err| {
                warn!("Audio stream error during tuning: {}", err);
            },
            None,
        ).context("Failed to build input stream for tuning")?;

        stream.play().context("Failed to start tuning stream")?;

        // Monitor and provide feedback
        let mut last_feedback = Instant::now();
        let mut speech_prompted = false;

        while recording.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(250)).await;
            
            let elapsed = Instant::now().duration_since(start_time);
            
            // Provide feedback every second
            if Instant::now().duration_since(last_feedback) >= Duration::from_secs(1) {
                if elapsed.as_secs() < silence_time {
                    let remaining = silence_time - elapsed.as_secs();
                    println!("🔇 Stay silent... {}s remaining", remaining);
                } else if !speech_prompted {
                    println!("🗣️  Now SPEAK CLEARLY for {} seconds... (say anything, read text, etc.)", speech_time);
                    speech_prompted = true;
                } else {
                    let remaining = duration_seconds - elapsed.as_secs();
                    if remaining > 0 {
                        println!("🗣️  Keep talking... {}s remaining", remaining);
                    }
                }
                last_feedback = Instant::now();
            }
        }

        drop(stream);

        // Analyze the collected data
        let volumes_data = volumes.lock().unwrap().clone();
        
        if volumes_data.len() < 10 {
            warn!("❌ Not enough data collected for tuning");
            return Ok(None);
        }

        let silence_volumes: Vec<f32> = volumes_data
            .iter()
            .filter(|(_, elapsed)| elapsed.as_secs() < silence_time)
            .map(|(volume, _)| *volume)
            .collect();

        let speech_volumes: Vec<f32> = volumes_data
            .iter()
            .filter(|(_, elapsed)| elapsed.as_secs() >= silence_time)
            .map(|(volume, _)| *volume)
            .collect();

        if silence_volumes.is_empty() || speech_volumes.is_empty() {
            warn!("❌ Insufficient silence or speech data");
            return Ok(None);
        }

        // Calculate statistics
        let avg_silence = silence_volumes.iter().sum::<f32>() / silence_volumes.len() as f32;
        let max_silence = silence_volumes.iter().fold(0.0f32, |a, &b| a.max(b));
        let p95_silence = percentile(&silence_volumes, 0.95);
        
        let avg_speech = speech_volumes.iter().sum::<f32>() / speech_volumes.len() as f32;
        let min_speech = speech_volumes.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let p10_speech = percentile(&speech_volumes, 0.10);

        println!();
        println!("📊 Analysis Results:");
        println!("   Average silence volume: {:.1}", avg_silence);
        println!("   Maximum silence volume: {:.1}", max_silence);
        println!("   95th percentile silence: {:.1}", p95_silence);
        println!("   Average speech volume: {:.1}", avg_speech);
        println!("   Minimum speech volume: {:.1}", min_speech);
        println!("   10th percentile speech: {:.1}", p10_speech);

        // Improved algorithm: Calculate multiple thresholds
        let gap = p10_speech - max_silence;
        
        if gap > 0.5 {
            // Good separation between silence and speech
            let conservative = max_silence + gap * 0.2;  // Close to silence
            let balanced = max_silence + gap * 0.5;      // Middle ground  
            let aggressive = max_silence + gap * 0.8;    // Close to speech
            
            println!();
            println!("🎯 Threshold Suggestions (try in order):");
            println!("   🟢 Balanced: {:.1}     (recommended - stops on natural pauses)", balanced);
            println!("   🔵 Conservative: {:.1}  (stops quickly, may cut off speech)", conservative);
            println!("   🟡 Aggressive: {:.1}   (allows long pauses, slower to stop)", aggressive);
            println!();
            println!("💡 Start with the balanced setting. If it cuts you off, try aggressive.");
            println!("   If it doesn't stop, try conservative.");
            
            // Use balanced as the auto-applied setting
            let optimal_threshold = balanced;
            println!("✅ Auto-applying balanced threshold: {:.1}", optimal_threshold);
            
            Ok(Some(optimal_threshold))
        } else {
            // Poor separation - provide wider range
            let base = (max_silence + p10_speech) / 2.0;
            let conservative = base * 0.7;
            let balanced = base;
            let aggressive = base * 1.4;
            
            println!();
            println!("⚠️  Overlapping silence/speech levels detected!");
            println!("🎯 Threshold Suggestions (experiment needed):");
            println!("   🟢 Balanced: {:.1}     (starting point)", balanced);
            println!("   🔵 Conservative: {:.1}  (try if balanced doesn't stop)", conservative); 
            println!("   🟡 Aggressive: {:.1}   (try if balanced cuts you off)", aggressive);
            println!();
            println!("💡 Your microphone setup may need adjustment or try --tune again in a quieter environment.");
            
            let optimal_threshold = balanced;
            println!("✅ Auto-applying balanced threshold: {:.1}", optimal_threshold);
            
            Ok(Some(optimal_threshold))
        }
    }

    /// Clean up temporary files
    pub fn cleanup(&mut self) {
        if let Some(temp_path) = &self.temp_file {
            if temp_path.exists() {
                if let Err(e) = std::fs::remove_file(temp_path) {
                    warn!("Failed to clean up temporary file: {}", e);
                } else {
                    debug!("Cleaned up temporary file: {:?}", temp_path);
                }
            }
            self.temp_file = None;
        }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Calculate RMS (Root Mean Square) value for audio samples
fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let sum_squares: f32 = samples.iter().map(|&sample| sample * sample).sum();
    (sum_squares / samples.len() as f32).sqrt() * 1000.0 // Scale for easier threshold values
}

/// Calculate percentile of a dataset
fn percentile(data: &[f32], p: f64) -> f32 {
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    
    let index = (p * (sorted.len() - 1) as f64) as usize;
    sorted[index.min(sorted.len() - 1)]
} 