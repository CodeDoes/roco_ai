//! TUI Application — terminal UI with rich widgets.
//!
//! Features:
//! - Split-pane layout (story, outline, plot state)
//! - Real-time preview
//! - Keyboard shortcuts
//! - Status bar
//! - Command palette

use std::io;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

// ═════════════════════════════════════════════════════════════════════════════
// App State
// ═════════════════════════════════════════════════════════════════════════════

/// Application state
pub struct App {
    /// Current mode
    mode: Mode,
    /// Outline items
    outline: Vec<OutlineItem>,
    /// Current chapter index
    current_chapter: usize,
    /// Chapter content
    chapters: Vec<String>,
    /// Plot state
    plot_state: PlotState,
    /// Status message
    status: String,
    /// Should quit
    should_quit: bool,
}

#[derive(Clone, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

struct OutlineItem {
    number: usize,
    title: String,
    summary: String,
}

struct PlotState {
    characters: Vec<String>,
    locations: Vec<String>,
    conflicts: Vec<String>,
    arc_stage: String,
}

impl App {
    /// Create a new app
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            outline: vec![
                OutlineItem {
                    number: 1,
                    title: "The Beginning".to_string(),
                    summary: "Introduction of the protagonist".to_string(),
                },
                OutlineItem {
                    number: 2,
                    title: "The Journey".to_string(),
                    summary: "The protagonist sets out on a quest".to_string(),
                },
                OutlineItem {
                    number: 3,
                    title: "The End".to_string(),
                    summary: "Resolution of the conflict".to_string(),
                },
            ],
            current_chapter: 0,
            chapters: vec![
                "# Chapter 1\n\nThe knight stood at the crossroads, his hand resting on the hilt of his sword...".to_string(),
                "# Chapter 2\n\nThe journey was long and arduous...".to_string(),
                "# Chapter 3\n\nIn the end, the knight found what he was looking for...".to_string(),
            ],
            plot_state: PlotState {
                characters: vec!["Knight".to_string(), "Stranger".to_string()],
                locations: vec!["Crossroads".to_string()],
                conflicts: vec!["The knight must choose".to_string()],
                arc_stage: "rising_action".to_string(),
            },
            status: "Normal mode".to_string(),
            should_quit: false,
        }
    }

    /// Handle key events
    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.current_chapter < self.outline.len() - 1 {
                        self.current_chapter += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.current_chapter > 0 {
                        self.current_chapter -= 1;
                    }
                }
                KeyCode::Char('i') => {
                    self.mode = Mode::Insert;
                    self.status = "Insert mode".to_string();
                }
                KeyCode::Char(':') => {
                    self.mode = Mode::Command;
                    self.status = "Command mode".to_string();
                }
                KeyCode::Char('g') => {
                    self.status = "Generating...".to_string();
                    // TODO: Generate chapter
                }
                KeyCode::Char('r') => {
                    self.status = "Revising...".to_string();
                    // TODO: Revise chapter
                }
                KeyCode::Char('s') => {
                    self.status = "Saved".to_string();
                    // TODO: Save
                }
                _ => {}
            },
            Mode::Insert => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.status = "Normal mode".to_string();
                }
                _ => {
                    // TODO: Handle text input
                }
            },
            Mode::Command => match key.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.status = "Normal mode".to_string();
                }
                KeyCode::Enter => {
                    // TODO: Execute command
                    self.mode = Mode::Normal;
                    self.status = "Normal mode".to_string();
                }
                _ => {
                    // TODO: Handle command input
                }
            },
        }
    }

    /// Check if should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Draw the UI
    pub fn draw(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(f.size());

        // Left panel: Outline
        self.draw_outline(f, chunks[0]);

        // Center panel: Editor
        self.draw_editor(f, chunks[1]);

        // Right panel: Plot state
        self.draw_plot_state(f, chunks[2]);

        // Bottom: Status bar
        self.draw_status(f, f.size());
    }

    fn draw_outline(&self, f: &mut Frame, area: Rect) {
        let outline: Vec<Spans> = self.outline.iter().enumerate().map(|(i, item)| {
            let style = if i == self.current_chapter {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Spans::from(vec![
                Span::styled(format!("{}. ", item.number), style),
                Span::styled(&item.title, style),
            ])
        }).collect();

        let block = Block::default()
            .title("Outline")
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(outline)
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn draw_editor(&self, f: &mut Frame, area: Rect) {
        let content = if self.current_chapter < self.chapters.len() {
            &self.chapters[self.current_chapter]
        } else {
            "No chapter selected"
        };

        let block = Block::default()
            .title(format!("Chapter {}", self.current_chapter + 1))
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn draw_plot_state(&self, f: &mut Frame, area: Rect) {
        let mut lines = vec![
            Spans::from(Span::styled("Characters", Style::default().fg(Color::Cyan))),
        ];
        for ch in &self.plot_state.characters {
            lines.push(Spans::from(Span::raw(format!("  • {}", ch))));
        }

        lines.push(Spans::from(Span::raw("")));
        lines.push(Spans::from(Span::styled("Locations", Style::default().fg(Color::Cyan))));
        for loc in &self.plot_state.locations {
            lines.push(Spans::from(Span::raw(format!("  • {}", loc))));
        }

        lines.push(Spans::from(Span::raw("")));
        lines.push(Spans::from(Span::styled("Conflicts", Style::default().fg(Color::Cyan))));
        for con in &self.plot_state.conflicts {
            lines.push(Spans::from(Span::raw(format!("  • {}", con))));
        }

        lines.push(Spans::from(Span::raw("")));
        lines.push(Spans::from(Span::styled("Arc Stage", Style::default().fg(Color::Cyan))));
        lines.push(Spans::from(Span::raw(&self.plot_state.arc_stage)));

        let block = Block::default()
            .title("Plot State")
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let status = Spans::from(vec![
            Span::styled(&self.status, Style::default().fg(Color::White)),
            Span::raw(" │ "),
            Span::styled(
                match self.mode {
                    Mode::Normal => "NORMAL",
                    Mode::Insert => "INSERT",
                    Mode::Command => "COMMAND",
                },
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" │ "),
            Span::styled(
                format!("Chapter {}/{}", self.current_chapter + 1, self.outline.len()),
                Style::default().fg(Color::Gray),
            ),
        ]);

        let block = Block::default()
            .borders(Borders::ALL);

        let paragraph = Paragraph::new(status)
            .block(block);

        f.render_widget(paragraph, area);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Run TUI
// ═════════════════════════════════════════════════════════════════════════════

/// Run the TUI application
pub fn run() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();

    // Main loop
    loop {
        // Draw
        terminal.draw(|f| app.draw(f))?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit() {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
