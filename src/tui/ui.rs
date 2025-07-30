use crate::tui::app::{App, AppState};
use ratatui::{
    prelude::*,
    widgets::{BarChart, Block, Borders, List, ListItem, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &App) {
    let main_constraints = if app.show_logs {
        vec![
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(10), // Logs
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ]
    };

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(main_constraints)
        .split(frame.size());

    let top_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(main_layout[0]);

    let bottom_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ]
            .as_ref(),
        )
        .split(main_layout[2]);

    // Status and Duration
    let status_text = match app.state {
        AppState::Idle => "Idle",
        AppState::LoadingModel => "🔄 Loading Model...",
        AppState::Recording => "🎤 Recording",
        AppState::Processing => "🤖 Processing...",
        AppState::Transcribing => "🧠 Transcribing...",
        AppState::Finished => "✅ Finished",
        AppState::ModelSelection => "📋 Select Model",
        AppState::ShowingShortcuts => "❓ Shortcuts",
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().title("Status").borders(Borders::ALL));
    frame.render_widget(status, top_layout[0]);

    let duration_text = format!("{:.1}s", app.recording_duration.as_secs_f32());
    let duration = Paragraph::new(duration_text)
        .block(Block::default().title("Duration").borders(Borders::ALL));
    frame.render_widget(duration, top_layout[1]);

    // Middle area: Model selection, transcribed text, or waveform
    let middle_area_index = 1;
    match app.state {
        AppState::ModelSelection => {
            let model_items: Vec<ListItem> = app
                .available_models
                .iter()
                .enumerate()
                .map(|(i, model)| {
                    let mut style = Style::default();
                    if i == app.selected_model_index {
                        style = style.bg(Color::Blue).fg(Color::White);
                    }
                    if model == app.get_current_model() {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    ListItem::new(format!("  {model}")).style(style)
                })
                .collect();

            let model_list = List::new(model_items)
                .block(
                    Block::default()
                        .title("Select Model (↑/↓ to navigate, Enter to select, Esc to cancel)")
                        .borders(Borders::ALL),
                )
                .style(Style::default().fg(Color::White));
            frame.render_widget(model_list, main_layout[middle_area_index]);
        }
        AppState::ShowingShortcuts => {
            let shortcuts_text = vec![
                "Keyboard Shortcuts:",
                "",
                "Space         - Start/Stop recording",
                "Q / Escape    - Quit application",
                "M             - Change model (when idle)",
                "L             - Toggle logs",
                "?             - Show/hide this help",
                "",
                "Model Selection:",
                "↑/↓           - Navigate models",
                "Enter         - Select model",
                "Escape        - Cancel selection",
                "",
                "Recording:",
                "Space         - Stop recording",
                "",
                "Press Escape to close this help.",
            ]
            .join("\n");

            let shortcuts = Paragraph::new(shortcuts_text)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .block(
                    Block::default()
                        .title("Keyboard Shortcuts (Press Escape to close)")
                        .borders(Borders::ALL),
                )
                .style(Style::default().fg(Color::Cyan));
            frame.render_widget(shortcuts, main_layout[middle_area_index]);
        }
        _ => {
            if app.transcribed_text.is_some() {
                let text = app.transcribed_text.as_deref().unwrap_or("");
                let paragraph = Paragraph::new(text)
                    .wrap(ratatui::widgets::Wrap { trim: true })
                    .block(
                        Block::default()
                            .title("Transcription")
                            .borders(Borders::ALL),
                    );
                frame.render_widget(paragraph, main_layout[middle_area_index]);
            } else {
                let data: Vec<(&str, u64)> = app
                    .audio_waveform
                    .iter()
                    .map(|v| {
                        let scaled = (v.abs() * 1000.0) as u64; // Scale up more for visibility
                        let min_height = if scaled > 0 { 1 } else { 0 }; // Ensure non-zero values show
                        ("", scaled.max(min_height))
                    })
                    .collect();
                // Add debug info to title
                let title = if app.audio_waveform.is_empty() {
                    "Waveform (no data)".to_string()
                } else {
                    format!("Waveform ({} samples)", app.audio_waveform.len())
                };

                let barchart = BarChart::default()
                    .block(Block::default().title(title).borders(Borders::ALL))
                    .data(&data)
                    .bar_width(1)
                    .style(Style::default().fg(Color::Green));
                frame.render_widget(barchart, main_layout[middle_area_index]);
            }
        }
    }

    // Audio Level, Device, and Model
    let level_text = format!("Level: {:.0}", app.audio_level);
    let level = Paragraph::new(level_text)
        .block(Block::default().title("Audio Level").borders(Borders::ALL));
    frame.render_widget(level, bottom_layout[0]);

    let device = Paragraph::new(app.device_name.as_str())
        .block(Block::default().title("Device").borders(Borders::ALL));
    frame.render_widget(device, bottom_layout[1]);

    let model_info = format!("{}\n{}", app.get_current_model(), app.model_status);
    let model = Paragraph::new(model_info)
        .wrap(ratatui::widgets::Wrap { trim: true })
        .block(
            Block::default()
                .title("Model (M to change)")
                .borders(Borders::ALL),
        );
    frame.render_widget(model, bottom_layout[2]);

    // Log Box
    if app.show_logs {
        let log_items: Vec<ListItem> = app.logs.iter().map(|m| ListItem::new(m.as_str())).collect();
        let log_list = List::new(log_items)
            .block(
                Block::default()
                    .title("Logs (L to toggle)")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(log_list, main_layout[3]);
    }
}
