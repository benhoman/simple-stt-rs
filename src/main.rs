use anyhow::{Context, Result};
use clap::{Arg, Command};
use simple_stt_rs::{audio::AudioRecorder, clipboard::ClipboardManager, config::Config, stt::SttProcessor, ui::UiManager};
use std::process;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod llm;
use llm::LlmRefiner;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("❌ Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    let matches = Command::new("simple-stt")
        .version("0.1.0")
        .about("A Wayland-native speech-to-text CLI client with silence detection")
        .long_about("A Rust-based speech-to-text client for Wayland compositors (like Hyprland) that records audio, \
                     transcribes with Whisper (local or API), refines with LLM (optional), and outputs to clipboard or stdout. \
                     All features gracefully degrade when not configured.")
        .arg(
            Arg::new("tune")
                .long("tune")
                .help("Tune the silence threshold for your microphone and environment")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tune-interactive")
                .long("tune-interactive")
                .help("Interactive tuning - test different thresholds with immediate feedback")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("profile")
                .short('p')
                .long("profile")
                .value_name("PROFILE")
                .help("Use a specific LLM profile (e.g., general, todo, email, slack)")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("list-profiles")
                .long("list-profiles")
                .help("List all available LLM profiles")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("check-config")
                .long("check-config")
                .help("Check configuration status and show what features are available")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("stdout")
                .long("stdout")
                .help("Output transcribed text to stdout instead of copying to clipboard")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    // Setup logging
    setup_logging(matches.get_flag("verbose"))?;

    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;

    if matches.get_flag("check-config") {
        check_configuration(&config)?;
        return Ok(());
    }

    if matches.get_flag("list-profiles") {
        list_profiles(&config)?;
        return Ok(());
    }

    if matches.get_flag("tune") {
        tune_threshold(&config).await?;
        return Ok(());
    }

    if matches.get_flag("tune-interactive") {
        tune_threshold_interactive(&config).await?;
        return Ok(());
    }

    // Run speech-to-text
    let profile = matches.get_one::<String>("profile").map(|s| s.as_str());
    let use_stdout = matches.get_flag("stdout");
    run_stt(&config, profile, use_stdout).await
}

fn setup_logging(verbose: bool) -> Result<()> {
    let log_level = if verbose { "debug" } else { "info" };
    
    // Create log directory
    let log_dir = dirs::home_dir()
        .context("Could not determine home directory")?
        .join(".local")
        .join("share")
        .join("simple-stt");
    
    std::fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    // File appender
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "simple-stt.log");

    // Console filter
    let console_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .context("Failed to set log level")?;

    // File filter (always info or higher)
    let file_filter = EnvFilter::try_new("info").context("Failed to set file log level")?;

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(console_filter),
        )
        .with(
            fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_filter(file_filter),
        )
        .init();

    Ok(())
}

async fn tune_threshold(config: &Config) -> Result<()> {
    info!("Starting silence threshold tuning");
    println!("🎯 Tuning silence threshold for optimal detection...");

    let mut audio_recorder = AudioRecorder::new(config)
        .context("Failed to initialize audio recorder")?;

    let optimal_threshold = audio_recorder
        .tune_silence_threshold(12)
        .await
        .context("Failed to tune silence threshold")?;

    if let Some(threshold) = optimal_threshold {
        let mut updated_config = config.clone();
        updated_config.update_silence_threshold(threshold)
            .context("Failed to save updated configuration")?;

        println!("✅ Updated config with new threshold: {:.1}", threshold);
        println!("You can now use the STT system with optimized settings!");
        info!("Silence threshold tuning completed: {:.1}", threshold);
    } else {
        println!("❌ Tuning failed - could not determine optimal threshold");
        warn!("Silence threshold tuning failed");
    }

    Ok(())
}

async fn tune_threshold_interactive(config: &Config) -> Result<()> {
    use std::io::{self, Write};

    println!("🎯 Interactive Threshold Tuning");
    println!("===============================");
    println!("This will help you find the perfect threshold through testing!");
    println!();

    // First, do the automatic tuning to get baseline suggestions
    let mut audio_recorder = AudioRecorder::new(config)
        .context("Failed to initialize audio recorder")?;

    println!("🔄 Step 1: Automatic calibration...");
    let _optimal_threshold = audio_recorder
        .tune_silence_threshold(12)
        .await
        .context("Failed to run initial tuning")?;

    println!();
    println!("🧪 Step 2: Interactive testing");
    println!("We'll test different thresholds with quick 10-second recordings.");
    println!("Type the threshold to test, or 'done' when you find one that works.");
    println!();

    let mut test_config = config.clone();
    let mut successful_threshold: Option<f32> = None;

    loop {
        print!("🎯 Enter threshold to test (or 'done' to finish): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "done" {
            break;
        }

        let threshold: f32 = match input.parse() {
            Ok(t) => t,
            Err(_) => {
                println!("❌ Invalid number. Please enter a decimal number like 2.5");
                continue;
            }
        };

        // Update test config with new threshold
        test_config.audio.silence_threshold = threshold;
        let mut test_recorder = AudioRecorder::new(&test_config)?;

        println!("🎤 Testing threshold {:.1} - speak for ~5 seconds, then pause...", threshold);
        println!("⏱️  Recording will start in 3 seconds...");
        
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Quick test recording
        match test_recorder.record_until_silence().await {
            Ok(Some(audio_file)) => {
                println!("✅ Recording completed! How did it feel?");
                println!("   - Did it stop at the right time? (y/n)");
                print!("   - Your feedback: ");
                io::stdout().flush().unwrap();

                let mut feedback = String::new();
                io::stdin().read_line(&mut feedback)?;
                
                if feedback.trim().to_lowercase().starts_with('y') {
                    successful_threshold = Some(threshold);
                    println!("🎉 Great! Threshold {:.1} marked as working!", threshold);
                } else {
                    println!("👍 Noted. Try a different value:");
                    println!("   - If it cut you off → try a LOWER number (like {:.1})", threshold * 0.8);
                    println!("   - If it didn't stop → try a HIGHER number (like {:.1})", threshold * 1.2);
                }

                // Clean up test file
                let _ = std::fs::remove_file(audio_file);
                println!();
            }
            Ok(None) => {
                println!("❌ No audio recorded (silence detected immediately)");
                println!("   - Try a LOWER threshold (like {:.1})", threshold * 0.7);
                println!();
            }
            Err(e) => {
                println!("❌ Test recording failed: {}", e);
                println!("Try a different threshold.");
                println!();
            }
        }
    }

    // Apply the successful threshold if found
    if let Some(threshold) = successful_threshold {
        let mut updated_config = config.clone();
        updated_config.update_silence_threshold(threshold)
            .context("Failed to save updated configuration")?;

        println!("🎯 Applied successful threshold: {:.1}", threshold);
        println!("✅ Configuration saved! Ready to use STT system.");
    } else {
        println!("💡 No threshold was marked as successful.");
        println!("   You can manually edit ~/.config/simple-stt/config.yaml");
        println!("   or run 'simple-stt --tune' for automatic suggestions.");
    }

    Ok(())
}

fn list_profiles(config: &Config) -> Result<()> {
    println!("📝 Available LLM profiles:");
    
    for (profile_id, profile_data) in &config.llm.profiles {
        println!("  • {}: {}", profile_id, profile_data.name);
    }
    
    println!("\n🎯 Default profile: {}", config.llm.default_profile);
    
    Ok(())
}

async fn run_stt(config: &Config, profile: Option<&str>, use_stdout: bool) -> Result<()> {
    info!("Starting STT process with profile: {:?}", profile);

    // Initialize managers
    let mut ui_manager = UiManager::new(&config);
    let mut audio_recorder = AudioRecorder::new(&config)?;
    let mut stt_processor = SttProcessor::new(&config)?;
    let llm_refiner = LlmRefiner::new(&config)?;
    let mut clipboard_manager = ClipboardManager::new(&config)?;

    // Check LLM configuration
    let llm_configured = llm_refiner.is_configured();

    if !llm_configured {
        println!("⚠️  LLM not configured - will skip text refinement");
        warn!("LLM not configured, will skip text refinement");
    }

    // Start UI
    ui_manager.start()?;
    ui_manager.start_recording(profile);

    // Start STT preparation in parallel with recording
    let preparation_task = tokio::spawn(async move {
        match stt_processor.prepare().await {
            Ok(()) => Ok(stt_processor),
            Err(e) => Err(e),
        }
    });

    // Record audio
    let audio_file = audio_recorder
        .record_until_silence()
        .await
        .context("Failed to record audio")?;

    ui_manager.stop_recording();

    // Wait for STT preparation to complete
    ui_manager.set_status("⏳ Preparing transcription...", "#ffaa00");
    let stt_processor = match preparation_task.await {
        Ok(Ok(processor)) => processor,
        Ok(Err(e)) => {
            warn!("STT preparation failed: {}", e);
            println!("❌ STT preparation failed: {}", e);
            println!("🎤 Audio was recorded successfully but transcription is unavailable");
            ui_manager.set_error("STT preparation failed");
            return Ok(());
        }
        Err(e) => {
            warn!("STT preparation task failed: {}", e);
            println!("❌ STT preparation task failed: {}", e);
            ui_manager.set_error("STT preparation task failed");
            return Ok(());
        }
    };

    let audio_file = match audio_file {
        Some(file) => file,
        None => {
            ui_manager.set_warning("No audio recorded");
            return Ok(());
        }
    };

    // Check if STT processor is now configured after preparation
    if !stt_processor.is_configured() {
        println!("🎤 Audio recorded successfully!");
        println!("📁 Audio file saved to: {:?}", audio_file);
        if let Some(error) = stt_processor.preparation_failed() {
            println!("❌ STT preparation failed: {}", error);
        } else {
            println!("💡 STT backend not configured properly");
        }
        ui_manager.set_status("✅ Audio recorded (transcription unavailable)", "#ffaa00");
        
        // Clean up and exit
        std::fs::remove_file(&audio_file).ok();
        return Ok(());
    }

    // Transcribe audio
    ui_manager.set_transcribing();
    let text = match stt_processor.transcribe(&audio_file).await {
        Ok(Some(text)) => text,
        Ok(None) => {
            ui_manager.set_warning("No speech detected in audio");
            return Ok(());
        }
        Err(e) => {
            warn!("Transcription failed: {}", e);
            println!("❌ Transcription failed: {}", e);
            println!("🎤 Audio was recorded successfully but couldn't be transcribed");
            ui_manager.set_error("Transcription failed");
            return Ok(());
        }
    };

    info!("Transcribed text: \"{}\"", text);

    // Refine text with LLM (optional)
    let final_text = if llm_configured {
        ui_manager.set_refining(profile);
        
        match llm_refiner.refine_text(&text, profile).await {
            Ok(Some(refined)) => {
                info!("Text refined successfully");
                refined
            }
            Ok(None) => {
                warn!("LLM returned empty response, using original text");
                text
            }
            Err(e) => {
                warn!("LLM refinement failed: {}, using original text", e);
                println!("⚠️  Text refinement failed, using original transcription");
                text
            }
        }
    } else {
        info!("LLM not configured, using original transcription");
        text
    };

    info!("Final text: \"{}\"", final_text);

    // Handle output - stdout or clipboard/paste
    if use_stdout {
        // Output to stdout
        println!("{}", final_text);
        info!("STT process completed successfully - output to stdout");
    } else {
        // Handle clipboard/paste
        match if config.clipboard.auto_paste {
            clipboard_manager.paste_text(&final_text).await
        } else {
            clipboard_manager.copy_to_clipboard(&final_text).map(|_| ())
        } {
            Ok(_) => {
                ui_manager.set_completed(!config.clipboard.auto_paste);
                println!("✅ Processing complete!");
                info!("STT process completed successfully");
            }
            Err(e) => {
                warn!("Clipboard operation failed: {}", e);
                println!("⚠️  Transcription successful but clipboard operation failed:");
                println!("📝 Transcribed text: \"{}\"", final_text);
                ui_manager.set_status("⚠️ Transcription done, clipboard failed", "#ffaa00");
            }
        }
    }

    // Auto-hide delay
    if ui_manager.is_enabled() && config.ui.auto_hide_delay > 0.0 {
        sleep(Duration::from_secs_f64(config.ui.auto_hide_delay)).await;
    }

    Ok(())
}

fn check_configuration(config: &Config) -> Result<()> {
    println!("🔧 Configuration Status");
    println!("=======================");
    
    // Check STT configuration
    let stt_configured = match config.whisper.backend.as_str() {
        "api" => config.whisper.api_key.is_some(),
        "local" => {
            // For local backend, check if model exists or can be auto-downloaded
            match get_model_path(&config.whisper) {
                Ok(model_path) => model_path.exists() || config.whisper.download_models,
                Err(_) => false,
            }
        },
        _ => false,
    };
    
    if stt_configured {
        println!("✅ Speech-to-Text: Configured");
        println!("   Backend: {} ({})", 
            config.whisper.backend,
            match config.whisper.backend.as_str() {
                "api" => "OpenAI Whisper API",
                "local" => "Local Whisper models",
                _ => "Unknown"
            }
        );
        println!("   Model: {}", config.whisper.model);
        if let Some(lang) = &config.whisper.language {
            println!("   Language: {}", lang);
        }
        if config.whisper.backend == "local" {
            println!("   Device: {}", config.whisper.device);
            if let Some(path) = &config.whisper.model_path {
                println!("   Model Path: {}", path);
            } else {
                match get_model_path(&config.whisper) {
                    Ok(default_path) => {
                        println!("   Model Path: {:?} (default)", default_path);
                        if default_path.exists() {
                            println!("   Model Status: ✅ Available");
                        } else if config.whisper.download_models {
                            println!("   Model Status: ⚠️ Will be downloaded on first use");
                            println!("   Download URL: https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin", config.whisper.model);
                        } else {
                            println!("   Model Status: ❌ Not found");
                        }
                    }
                    Err(_) => println!("   Model Path: ❌ Invalid"),
                }
            }
        }
    } else {
        println!("❌ Speech-to-Text: Not configured");
        match config.whisper.backend.as_str() {
            "api" => println!("   💡 Set OPENAI_API_KEY environment variable to enable"),
            "local" => {
                println!("   📥 Local model not found");
                if config.whisper.download_models {
                    println!("   💡 Will be auto-downloaded on first use");
                } else {
                    println!("   💡 Download model manually or enable auto-download");
                }
                if let Ok(model_path) = get_model_path(&config.whisper) {
                    println!("   📁 Expected location: {:?}", model_path);
                    println!("   🌐 Download from: https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin", config.whisper.model);
                }
            },
            _ => println!("   ❌ Unknown backend: {}", config.whisper.backend),
        }
    }
    
    // Check LLM configuration  
    let llm_configured = config.llm.api_key.is_some();
    if llm_configured {
        println!("✅ LLM Text Refinement: Configured");
        println!("   Provider: {}", config.llm.provider);
        println!("   Model: {}", config.llm.model);
        println!("   Default Profile: {}", config.llm.default_profile);
        println!("   Available Profiles: {}", config.llm.profiles.len());
    } else {
        println!("❌ LLM Text Refinement: Not configured");
        match config.llm.provider.as_str() {
            "openai" => println!("   💡 Set OPENAI_API_KEY environment variable to enable"),
            "anthropic" => println!("   💡 Set ANTHROPIC_API_KEY environment variable to enable"),
            _ => println!("   💡 Configure API key for {} provider", config.llm.provider),
        }
    }
    
    // Check audio configuration
    println!("✅ Audio Recording: Always available");
    println!("   Sample Rate: {} Hz", config.audio.sample_rate);
    println!("   Silence Threshold: {:.1}", config.audio.silence_threshold);
    println!("   Silence Duration: {:.1}s", config.audio.silence_duration);
    
    // Check clipboard configuration
    println!("✅ Clipboard: Always available");
    if config.clipboard.auto_paste {
        println!("   Auto-paste: Enabled");
        let paste_tools = ClipboardManager::check_paste_tools();
        if paste_tools.is_empty() {
            println!("   ⚠️  No paste tools found (install wtype or ydotool for Wayland)");
        } else {
            println!("   Paste tools: {}", paste_tools.join(", "));
        }
    } else {
        println!("   Auto-paste: Disabled (will copy to clipboard)");
    }

    let (clipboard_tools, _) = ClipboardManager::check_tools();
    if clipboard_tools.is_empty() {
        println!("   ⚠️  No clipboard tools found (install wl-copy/wl-paste for Wayland)");
    } else {
        println!("   Clipboard tools: {}", clipboard_tools.join(", "));
    }

    println!();
    println!("📖 Usage modes:");
    if stt_configured && llm_configured {
        println!("   🚀 Full mode: Record → Transcribe → Refine → Clipboard/Stdout");
    } else if stt_configured {
        println!("   📝 Transcription mode: Record → Transcribe → Clipboard/Stdout");
    } else {
        println!("   🎤 Audio-only mode: Record → Save audio file");
    }

    println!("   📺 Output options: --stdout (stdout) or default (clipboard)");

    println!("\n📁 Config file: {:?}", Config::config_path()?);
    println!("🌊 This application is designed for Wayland compositors (like Hyprland)");
    
    Ok(())
}

/// Get the path where the model should be located (duplicate from local.rs for checking)
fn get_model_path(config: &simple_stt_rs::config::WhisperConfig) -> Result<std::path::PathBuf> {
    use std::path::PathBuf;
    
    if let Some(ref path) = config.model_path {
        let expanded = shellexpand::tilde(path);
        Ok(PathBuf::from(expanded.as_ref()))
    } else {
        // Default model path in cache directory
        let cache_dir = dirs::cache_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
            .unwrap_or_else(|| std::env::temp_dir());
            
        let model_dir = cache_dir.join("simple-stt").join("models");
        let model_file = format!("ggml-{}.bin", config.model);
        
        Ok(model_dir.join(model_file))
    }
}
