use std::sync::Arc;
use std::time::Duration;
use roco_engine::{CompletionRequest, ModelBackend};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use tokio::sync::mpsc;
use tracing::error;

pub struct TuiApp {
    pub backend: Arc<dyn ModelBackend>,
}

enum TuiEvent {
    Token(String),
    Finished,
}

impl TuiApp {
    pub fn new(backend: Arc<dyn ModelBackend>) -> Self {
        Self { backend }
    }

    pub fn run(&self) -> Result<(), String> {
        // Setup terminal
        enable_raw_mode().map_err(|e| format!("failed to enable raw mode: {e}"))?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| format!("failed to setup terminal: {e}"))?;
        let backend_rt = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend_rt).map_err(|e| format!("failed to create terminal: {e}"))?;

        // Run application
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("failed to build tokio runtime for TUI: {e}"))?;

        let run_result = rt.block_on(self.run_loop(&mut terminal));

        // Restore terminal
        disable_raw_mode().unwrap();
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
        terminal.show_cursor().unwrap();

        run_result.map_err(|e| format!("TUI run error: {e}"))
    }

    async fn run_loop<B: ratatui::backend::Backend>(&self, terminal: &mut Terminal<B>) -> Result<(), String> {
        let mut messages: Vec<(String, String)> = Vec::new();
        let mut input = String::new();
        let mut generating = false;

        // Channel to receive tokens in the main loop
        let (token_tx, mut token_rx) = mpsc::channel::<TuiEvent>(100);

        messages.push(("system".to_string(), "Welcome to RoCo TUI Chat!".to_string()));

        loop {
            // 1. Draw UI
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints(
                        [
                            Constraint::Min(5),    // Messages box
                            Constraint::Length(3), // Input box
                            Constraint::Length(1), // Help status bar
                        ]
                        .as_ref(),
                    )
                    .split(f.size());

                // Create Messages Paragraph
                let mut text = Vec::new();
                for (role, content) in &messages {
                    let color = match role.as_str() {
                        "user" => Color::Blue,
                        "assistant" => Color::Magenta,
                        "system" => Color::Yellow,
                        _ => Color::White,
                    };
                    text.push(Line::from(vec![
                        Span::styled(format!("{}: ", role.to_uppercase()), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                        Span::raw(content),
                    ]));
                }

                let messages_paragraph = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title("Conversation"))
                    .wrap(Wrap { trim: true });
                f.render_widget(messages_paragraph, chunks[0]);

                // Create Input Box
                let input_paragraph = Paragraph::new(input.as_str())
                    .block(Block::default().borders(Borders::ALL).title("Input Message"));
                f.render_widget(input_paragraph, chunks[1]);

                // Help status
                let status_text = if generating {
                    "Generating response..."
                } else {
                    "ESC: Quit | Enter: Send message"
                };
                let status_paragraph = Paragraph::new(status_text)
                    .style(Style::default().fg(Color::DarkGray));
                f.render_widget(status_paragraph, chunks[2]);
            }).map_err(|e| format!("failed to draw TUI: {e}"))?;

            // 2. Poll Event/Tokens
            tokio::select! {
                // Check if we received streaming tokens or finished signal
                Some(t_event) = token_rx.recv() => {
                    match t_event {
                        TuiEvent::Token(token) => {
                            if let Some(last) = messages.last_mut() {
                                if last.0 == "assistant" {
                                    last.1.push_str(&token);
                                } else {
                                    messages.push(("assistant".to_string(), token));
                                }
                            } else {
                                messages.push(("assistant".to_string(), token));
                            }
                        }
                        TuiEvent::Finished => {
                            generating = false;
                        }
                    }
                }
                // Check for user key events
                _ = tokio::time::sleep(Duration::from_millis(15)) => {
                    if event::poll(Duration::from_millis(5)).unwrap_or(false) {
                        if let Event::Key(key) = event::read().map_err(|e| format!("failed to read event: {e}"))? {
                            match key.code {
                                KeyCode::Esc => {
                                    return Ok(());
                                }
                                KeyCode::Enter => {
                                    if !input.trim().is_empty() && !generating {
                                        messages.push(("user".to_string(), input.clone()));
                                        generating = true;

                                        // Spawn generation task
                                        let backend_clone = self.backend.clone();
                                        let token_tx_clone = token_tx.clone();
                                        let prompt_text = input.clone();
                                        input.clear();

                                        tokio::spawn(async move {
                                            let token_tx_clone_on_tok = token_tx_clone.clone();
                                            let on_token = Box::new(move |token: &str| {
                                                let _ = token_tx_clone_on_tok.try_send(TuiEvent::Token(token.to_string()));
                                            });
                                            let req = CompletionRequest {
                                                system: "You are a friendly, helpful assistant inside a terminal TUI.".to_string(),
                                                prompt: prompt_text,
                                                on_token: Some(on_token),
                                                ..Default::default()
                                            };
                                            if let Err(e) = backend_clone.complete(req).await {
                                                error!("TUI generation failed: {e}");
                                            }
                                            let _ = token_tx_clone.try_send(TuiEvent::Finished);
                                        });
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if !generating {
                                        input.push(c);
                                    }
                                }
                                KeyCode::Backspace => {
                                    if !generating {
                                        input.pop();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
