use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs::cache_dir;
use ratatui::{prelude::*, Terminal};
use simple_stt_rs::{
    audio::{AudioData, AudioRecorder},
    clipboard::ClipboardManager,
    config::Config,
    stt::{wav_utils, SttProcessor},
    tui::{
        app::{App, AppState},
        events::handle_key_events,
        ui::draw,
    },
};
use std::io;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

async fn load_stt_processor(
    config: &Config,
    app: &Arc<Mutex<App>>,
    log_tx: &tokio_mpsc::Sender<String>,
) -> Result<Arc<tokio::sync::Mutex<SttProcessor>>> {
    {
        let mut app = app.lock().unwrap();
        app.model_status = format!("Loading {}...", config.whisper.model);
        app.state = AppState::LoadingModel;
    }

    let mut stt_processor = match SttProcessor::new(config) {
        Ok(processor) => processor,
        Err(e) => {
            let error_msg = format!("❌ Failed to create STT processor: {e}");
            {
                let mut app = app.lock().unwrap();
                app.model_status = error_msg.clone();
                app.state = AppState::Idle;
            }
            log_tx.send(error_msg).await.ok();
            return Err(e);
        }
    };

    match stt_processor.prepare().await {
        Ok(_) => {
            {
                let mut app = app.lock().unwrap();
                app.model_status = "✅ Model Ready".to_string();
                app.state = AppState::Idle;
            }
            log_tx
                .send(format!(
                    "Model {} loaded successfully",
                    config.whisper.model
                ))
                .await
                .ok();
        }
        Err(e) => {
            let error_msg = format!("❌ Error loading model: {e}");
            {
                let mut app = app.lock().unwrap();
                app.model_status = error_msg.clone();
                app.state = AppState::Idle;
            }
            log_tx.send(error_msg).await.ok();
            return Err(e);
        }
    }

    Ok(Arc::new(tokio::sync::Mutex::new(stt_processor)))
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;
    let config = Config::load()?;
    let device_name = cpal::default_host()
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(|| "Unknown Device".to_string());
    let app = Arc::new(Mutex::new(App::new(config.clone(), device_name)));
    let mut terminal = setup_terminal()?;
    let mut clipboard_manager = ClipboardManager::new(&app.lock().unwrap().config)?;

    let (audio_tx, audio_rx) = mpsc::channel::<AudioData>();
    let (stt_tx, mut stt_rx) = tokio_mpsc::channel::<String>(1);
    let (log_tx, mut log_rx) = tokio_mpsc::channel::<String>(10);
    let (stop_audio_tx, stop_audio_rx) = mpsc::channel::<()>();
    let (audio_stopped_tx, audio_stopped_rx) = mpsc::channel::<()>();
    let (start_audio_tx, start_audio_rx) = mpsc::channel::<()>();
    // --- STT Preparation ---
    let app_clone_for_stt = app.clone();
    let log_tx_clone_prepare = log_tx.clone();
    let stt_prepare_task = tokio::spawn(async move {
        let config = { app_clone_for_stt.lock().unwrap().config.clone() };
        (load_stt_processor(&config, &app_clone_for_stt, &log_tx_clone_prepare).await).ok()
    });

    // --- Audio Recording Thread ---
    let config_clone_for_audio = config.clone();
    let app_clone_for_audio = app.clone();
    let audio_stopped_tx_clone = audio_stopped_tx.clone();
    std::thread::spawn(move || {
        let mut audio_recorder: Option<AudioRecorder> = None;
        let mut recording_active = false;

        loop {
            // Check if application should exit
            if !app_clone_for_audio.lock().unwrap().running {
                if let Some(ref mut recorder) = audio_recorder {
                    recorder.stop_recording();
                }
                tracing::info!("Audio thread: Application shutting down, exiting audio thread");
                break;
            }

            // Check for start signal
            if start_audio_rx.try_recv().is_ok() && !recording_active {
                tracing::info!("Audio thread: Starting new recording session");

                // Clear any leftover stop signals from previous recording
                while stop_audio_rx.try_recv().is_ok() {
                    // Silently clear leftover signals
                }

                // Create a fresh audio recorder for each session
                match AudioRecorder::new(&config_clone_for_audio) {
                    Ok(mut recorder) => {
                        if let Err(e) = recorder.start_recording(audio_tx.clone()) {
                            tracing::error!("Audio thread: Failed to start recording: {}", e);
                        } else {
                            tracing::info!("Audio thread: Successfully started recording");
                            audio_recorder = Some(recorder);
                            recording_active = true;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Audio thread: Failed to create recorder: {}", e);
                    }
                }
            }

            // Check for stop signal
            if recording_active && stop_audio_rx.try_recv().is_ok() {
                tracing::info!("Audio thread: Received stop signal, ending recording session");
                if let Some(ref mut recorder) = audio_recorder {
                    recorder.stop_recording();
                }
                // Drop the recorder completely for next session
                audio_recorder = None;
                recording_active = false;
                audio_stopped_tx_clone.send(()).ok();
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let stt_processor_arc = match stt_prepare_task.await? {
        Some(processor) => processor,
        None => {
            tracing::error!("Failed to initialize STT processor");
            return Err(anyhow::anyhow!("STT processor initialization failed"));
        }
    };
    let mut recorded_audio: Vec<f32> = Vec::new();

    loop {
        let app_arc = app.clone(); // Store reference to Arc before locking
        let mut app = app.lock().unwrap();
        if !app.running {
            break;
        }

        terminal.draw(|frame| draw(frame, &app))?;
        handle_key_events(&mut app, stop_audio_tx.clone(), start_audio_tx.clone())?;

        // Process incoming log messages
        while let Ok(log_message) = log_rx.try_recv() {
            app.add_log_message(log_message);
        }

        // Handle model selection confirmation
        if app.model_change_requested {
            app.model_change_requested = false;
            let selected_model = app.get_selected_model().to_string();
            if selected_model != app.get_current_model() {
                // Update config and reload model
                app.config.whisper.model = selected_model.clone();
                app.model_status = format!("Loading {selected_model}...");
                app.state = AppState::LoadingModel;

                // Save config
                if let Err(e) = app.config.save() {
                    tracing::error!("Failed to save config: {}", e);
                }

                tracing::info!("Model changed to: {}, reloading...", selected_model);

                // Reload the STT processor with new model
                let app_clone_for_reload = app_arc.clone();
                let log_tx_clone_reload = log_tx.clone();
                let config_for_reload = app.config.clone();
                let stt_processor_clone = stt_processor_arc.clone();

                tokio::spawn(async move {
                    match load_stt_processor(
                        &config_for_reload,
                        &app_clone_for_reload,
                        &log_tx_clone_reload,
                    )
                    .await
                    {
                        Ok(new_processor) => {
                            // Replace the old processor with the new one
                            let new_processor_inner = Arc::try_unwrap(new_processor)
                                .map_err(|_| "Failed to unwrap Arc")
                                .unwrap()
                                .into_inner();
                            let mut old_processor = stt_processor_clone.lock().await;
                            *old_processor = new_processor_inner;
                            tracing::info!(
                                "✅ Model {} loaded successfully",
                                config_for_reload.whisper.model
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to reload model {}: {}",
                                config_for_reload.whisper.model,
                                e
                            );
                            let mut app = app_clone_for_reload.lock().unwrap();
                            app.model_status =
                                format!("❌ Failed to load {}", config_for_reload.whisper.model);
                            app.state = AppState::Idle;
                        }
                    }
                });
            } else {
                app.exit_model_selection();
            }
        }

        if app.state == AppState::Recording {
            if let Ok(data) = audio_rx.try_recv() {
                app.audio_level = data.level;

                // Update waveform for visualization (keep recent samples for display)
                const WAVEFORM_SAMPLES: usize = 100;

                // Take a subset of samples for waveform display (downsample if needed)
                let step = if data.samples.len() > WAVEFORM_SAMPLES {
                    data.samples.len() / WAVEFORM_SAMPLES
                } else {
                    1
                };

                let new_waveform_data: Vec<f32> = data
                    .samples
                    .iter()
                    .step_by(step)
                    .take(WAVEFORM_SAMPLES)
                    .cloned()
                    .collect();

                // Add new data and maintain sliding window
                app.audio_waveform.extend(new_waveform_data);
                if app.audio_waveform.len() > WAVEFORM_SAMPLES {
                    let excess = app.audio_waveform.len() - WAVEFORM_SAMPLES;
                    app.audio_waveform.drain(0..excess);
                }

                // Debug: Log waveform data occasionally
                static mut DEBUG_COUNTER: usize = 0;
                unsafe {
                    DEBUG_COUNTER += 1;
                    if DEBUG_COUNTER % 50 == 0 {
                        tracing::debug!(
                            "Waveform: {} samples, range: {:.3} to {:.3}",
                            app.audio_waveform.len(),
                            app.audio_waveform
                                .iter()
                                .min_by(|a, b| a.partial_cmp(b).unwrap())
                                .unwrap_or(&0.0),
                            app.audio_waveform
                                .iter()
                                .max_by(|a, b| a.partial_cmp(b).unwrap())
                                .unwrap_or(&0.0)
                        );
                    }
                }

                // Now extend recorded_audio (this consumes data.samples)
                recorded_audio.extend(data.samples);
            }
        }

        if app.state == AppState::Transcribing {
            if !app.transcription_initiated {
                app.transcription_initiated = true;
                stop_audio_tx.send(()).ok(); // Signal audio thread to stop
            }

            // Check if audio thread has confirmed stop (non-blocking)
            if audio_stopped_rx.try_recv().is_ok() {
                // Drain any remaining audio data from the channel
                while let Ok(data) = audio_rx.try_recv() {
                    recorded_audio.extend(data.samples);
                }

                let audio_to_process = std::mem::take(&mut recorded_audio);
                let config = app.config.clone();
                let stt_tx_clone = stt_tx.clone();
                let processor_clone = stt_processor_arc.clone();
                let log_tx_clone_transcribe = log_tx.clone();

                let audio_duration_sec =
                    audio_to_process.len() as f32 / config.audio.sample_rate as f32;
                tracing::debug!(
                    "Processing audio: {} samples, duration: {:.2} seconds",
                    audio_to_process.len(),
                    audio_duration_sec
                );

                // Save the audio file in the main thread to avoid race conditions
                let audio_file = wav_utils::save_wav(
                    &audio_to_process,
                    config.audio.sample_rate,
                    config.audio.channels,
                )?;

                tokio::spawn(async move {
                    let processor = processor_clone.lock().await;
                    let result = match processor
                        .transcribe(audio_file.path(), Some(log_tx_clone_transcribe.clone()))
                        .await
                    {
                        Ok(Some(text)) => text,
                        Ok(None) => {
                            log_tx_clone_transcribe
                                .send("Transcription: No speech detected.".to_string())
                                .await
                                .ok();
                            "No speech detected.".to_string()
                        }
                        Err(e) => {
                            let error_msg = format!("Transcription error: {e}");
                            log_tx_clone_transcribe.send(error_msg.clone()).await.ok();
                            error_msg
                        }
                    };
                    stt_tx_clone.send(result).await.ok();
                    drop(audio_file); // Ensure the temporary file is dropped after transcription
                });
            }
        }

        if let Ok(text) = stt_rx.try_recv() {
            if text != "No speech detected." {
                clipboard_manager.copy_to_clipboard(&text)?;
            }
            app.finish_processing(text);
            app.reset(); // Reset state for new transcription
            recorded_audio.clear();
        }

        app.tick();
        drop(app); // Release lock
        std::thread::sleep(Duration::from_millis(10));
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

use tracing_appender::rolling;

fn setup_logging() -> Result<()> {
    let cache_dir = cache_dir().context("Could not determine XDG cache directory")?;
    let log_dir = cache_dir.join("simple-stt");
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {log_dir:?}"))?;
    let log_file = rolling::daily(log_dir, "simple-stt.log");
    let log_level = "debug"; // Changed to debug for more verbose logging
    let log_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .unwrap();

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(log_file).with_filter(log_filter))
        .init();

    Ok(())
}
