use crate::config::Config;
use std::time::Duration;

#[derive(PartialEq)]
pub enum AppState {
    Idle,
    LoadingModel,
    Recording,
    Processing,
    Transcribing,
    Finished,
    ModelSelection,
    ShowingShortcuts,
}

pub struct App {
    pub state: AppState,
    pub config: Config,
    pub recording_duration: Duration,
    pub audio_waveform: Vec<f32>,
    pub running: bool,
    pub device_name: String,
    pub model_status: String,
    pub audio_level: f32,
    pub transcribed_text: Option<String>,
    pub logs: Vec<String>,
    pub show_logs: bool,
    pub transcription_initiated: bool,
    pub available_models: Vec<String>,
    pub selected_model_index: usize,
    pub model_change_requested: bool,
}

impl App {
    pub fn new(config: Config, device_name: String) -> Self {
        let model_name = config.whisper.model.clone();
        let available_models = vec![
            "tiny.en".to_string(),
            "base.en".to_string(),
            "small.en".to_string(),
            "medium.en".to_string(),
            "large".to_string(),
            "large-v3-turbo".to_string(),
        ];
        let selected_model_index = available_models
            .iter()
            .position(|m| m == &model_name)
            .unwrap_or(0);

        Self {
            state: AppState::LoadingModel,
            config,
            recording_duration: Duration::default(),
            audio_waveform: Vec::new(),
            running: true,
            device_name,
            model_status: format!("Loading {model_name}..."),
            audio_level: 0.0,
            transcribed_text: None,
            logs: Vec::new(),
            show_logs: false,
            transcription_initiated: false,
            available_models,
            selected_model_index,
            model_change_requested: false,
        }
    }

    pub fn tick(&mut self) {
        if let AppState::Recording = self.state {
            self.recording_duration += Duration::from_millis(100);
        }
    }

    pub fn start_recording(&mut self) {
        if self.state == AppState::Idle {
            self.state = AppState::Recording;
            self.recording_duration = Duration::default();
            self.audio_waveform.clear();
            self.transcribed_text = None;
            self.transcription_initiated = false;
        }
    }

    pub fn stop_recording(&mut self) {
        if self.state == AppState::Recording {
            self.state = AppState::Transcribing;
        }
    }

    pub fn finish_processing(&mut self, text: String) {
        self.transcribed_text = Some(text);
        self.state = AppState::Finished;
    }

    pub fn reset(&mut self) {
        if self.state == AppState::Finished {
            self.state = AppState::Idle;
            self.transcription_initiated = false;
            self.audio_waveform.clear(); // Clear waveform when finished
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    // New method to add log messages
    pub fn add_log_message(&mut self, message: String) {
        self.logs.push(message);
        // Keep only the last N messages to prevent excessive memory usage
        const MAX_LOG_MESSAGES: usize = 50;
        if self.logs.len() > MAX_LOG_MESSAGES {
            self.logs.drain(0..self.logs.len() - MAX_LOG_MESSAGES);
        }
    }

    pub fn enter_model_selection(&mut self) {
        if self.state == AppState::Idle {
            self.state = AppState::ModelSelection;
        }
    }

    pub fn exit_model_selection(&mut self) {
        if self.state == AppState::ModelSelection {
            self.state = AppState::Idle;
        }
    }

    pub fn select_previous_model(&mut self) {
        if self.selected_model_index > 0 {
            self.selected_model_index -= 1;
        } else {
            self.selected_model_index = self.available_models.len() - 1;
        }
    }

    pub fn select_next_model(&mut self) {
        if self.selected_model_index < self.available_models.len() - 1 {
            self.selected_model_index += 1;
        } else {
            self.selected_model_index = 0;
        }
    }

    pub fn get_selected_model(&self) -> &str {
        &self.available_models[self.selected_model_index]
    }

    pub fn get_current_model(&self) -> &str {
        &self.config.whisper.model
    }

    pub fn confirm_model_selection(&mut self) {
        self.model_change_requested = true;
    }

    pub fn enter_shortcuts(&mut self) {
        if matches!(self.state, AppState::Idle | AppState::Finished) {
            self.state = AppState::ShowingShortcuts;
        }
    }

    pub fn exit_shortcuts(&mut self) {
        if self.state == AppState::ShowingShortcuts {
            self.state = AppState::Idle;
        }
    }
}
