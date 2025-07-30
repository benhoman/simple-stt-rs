use crate::tui::app::{App, AppState};
use crossterm::event::{self, Event, KeyCode};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub fn handle_key_events(
    app: &mut App,
    stop_audio_tx: Sender<()>,
    start_audio_tx: Sender<()>,
) -> anyhow::Result<()> {
    if event::poll(Duration::from_millis(50))? {
        // Reduced polling interval
        if let Event::Key(key) = event::read()? {
            match app.state {
                AppState::ModelSelection => match key.code {
                    KeyCode::Up => app.select_previous_model(),
                    KeyCode::Down => app.select_next_model(),
                    KeyCode::Enter => {
                        app.confirm_model_selection();
                    }
                    KeyCode::Esc => app.exit_model_selection(),
                    KeyCode::Char('q') => app.quit(),
                    _ => {}
                },
                AppState::ShowingShortcuts => match key.code {
                    KeyCode::Esc => app.exit_shortcuts(),
                    KeyCode::Char('q') => app.quit(),
                    _ => {}
                },
                _ => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                        KeyCode::Char('l') => app.show_logs = !app.show_logs,
                        KeyCode::Char('m') => {
                            if app.state == AppState::Idle {
                                app.enter_model_selection();
                            }
                        }
                        KeyCode::Char('?') => {
                            app.enter_shortcuts();
                        }
                        KeyCode::Char(' ') => match app.state {
                            AppState::Idle => {
                                app.start_recording();
                                start_audio_tx.send(()).ok(); // Signal audio thread to start
                            }
                            AppState::Recording => {
                                stop_audio_tx.send(()).ok();
                                app.stop_recording();
                            }
                            AppState::Finished => {
                                // Explicitly set to Idle to allow starting a new recording
                                app.state = AppState::Idle;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}
