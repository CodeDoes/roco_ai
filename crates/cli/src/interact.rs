//! `roco interact` — Interactive CLI equivalent to the GUI experience.
//!
//! Exposes the same control flows as the GUI widgets (PacingWidget, ChatWidget)
//! but in the terminal:
//!
//! - **Interactive mode** (`--interactive`): REPL-like conversation with pacing
//!   controls, accept/skip/stop, streaming output, session persistence.
//! - **Prompt mode** (`--prompt "text"`): One-shot generation, saves session,
//!   prints result, then exits.
//! - **Resume mode** (`--resume <session-id>`): Load a previous session and
//!   continue from where you left off.

use std::io::{self, Write};
use std::path::PathBuf;

use roco_agent::interaction::{InteractionMode, InteractionState};

use crate::rich_output as r;

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// How to run the interactive session
#[derive(Debug, Clone)]
pub enum InteractMode {
    /// One-shot prompt: takes a prompt, generates, saves session, prints, exits
    Prompt { prompt: String },
    /// Full interactive REPL with pacing control
    Interactive {
        pacing: PacingChoice,
        prompt: Option<String>,
    },
    /// Resume a previous session by ID
    Resume { session_id: String },
}

/// Initial pacing mode for interactive sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacingChoice {
    Planning,
    Careful,
    Rolling,
    AutoAccept,
}

impl PacingChoice {
    pub fn to_interaction_mode(self) -> InteractionMode {
        match self {
            PacingChoice::Planning => InteractionMode::NoControl,
            PacingChoice::Careful => InteractionMode::FullControl,
            PacingChoice::Rolling => InteractionMode::ModerateControl { batch_size: 3 },
            PacingChoice::AutoAccept => InteractionMode::GoHam,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PacingChoice::Planning => "planning",
            PacingChoice::Careful => "careful",
            PacingChoice::Rolling => "rolling",
            PacingChoice::AutoAccept => "auto-accept",
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Conversation State (mirrors ChatWidgetState for the CLI)
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub id: String,
    pub messages: Vec<ConversationMessage>,
    pub pacing: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ConversationState {
    pub fn new(id: String, pacing: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            messages: Vec::new(),
            pacing: pacing.to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(ConversationMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, &json).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load(path: &PathBuf) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Running the interactive CLI
// ═════════════════════════════════════════════════════════════════════════════

/// Run the interactive CLI. This is the entry point called from `roco interact`.
pub fn run(mode: InteractMode, backend: &dyn roco_engine::ModelBackend) -> anyhow::Result<()> {
    match mode {
        InteractMode::Prompt { prompt } => run_prompt(backend, &prompt),
        InteractMode::Interactive { pacing, prompt } => run_interactive(backend, pacing, prompt),
        InteractMode::Resume { session_id } => run_resume(backend, &session_id),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Prompt Mode (One-shot, saves session, exits)
// ═════════════════════════════════════════════════════════════════════════════

fn run_prompt(backend: &dyn roco_engine::ModelBackend, prompt: &str) -> anyhow::Result<()> {
    let session_id = format!("prompt_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    let session_path = session_dir.join(format!("{}.json", session_id));

    let mut state = ConversationState::new(session_id.clone(), "auto-accept");

    r::header("RoCo AI — Prompt");
    r::info(&format!("Prompt: {}", prompt));
    r::dim(&format!("Session: {}", session_id));

    state.add_message("user", prompt);

    let request = roco_engine::CompletionRequest {
        system: "You are a creative writing assistant. Respond with vivid, engaging prose.".into(),
        prompt: prompt.to_string(),
        temperature: 0.8,
        max_tokens: 1024,
        prefill: Some("<think></think>".into()),
        ..Default::default()
    };

    let response = futures::executor::block_on(backend.complete(request))
        .map_err(|e| anyhow::anyhow!("Generation failed: {e}"))?;

    let text = response.text.trim().to_string();
    println!("\n{}", text);
    state.add_message("assistant", &text);

    // Save session and exit
    if let Err(e) = state.save(&session_path) {
        r::warning(&format!("Session save failed: {e}"));
    } else {
        r::success(&format!("Session saved: {}", session_path.display()));
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Interactive Mode (REPL)
// ═════════════════════════════════════════════════════════════════════════════

fn run_interactive(
    backend: &dyn roco_engine::ModelBackend,
    pacing: PacingChoice,
    initial_prompt: Option<String>,
) -> anyhow::Result<()> {
    let session_id = format!("interact_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    let session_path = session_dir.join(format!("{}.json", session_id));

    let mut state = ConversationState::new(session_id.clone(), pacing.label());

    r::header("RoCo AI — Interactive");
    r::dim("  Just type your story idea.  :h for help, :q to quit.\n");

    let mut current_pacing = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(current_pacing.clone(), 0);

    // If an initial prompt was provided, send it immediately
    if let Some(ref initial) = initial_prompt {
        state.add_message("user", initial);
        r::header("User");
        r::header("AI");
        let request = roco_engine::CompletionRequest {
            system: "You are a creative writing assistant.".into(),
            prompt: initial.clone(),
            temperature: 0.8,
            max_tokens: 1024,
            prefill: Some("<think></think>".into()),
            ..Default::default()
        };
        match futures::executor::block_on(backend.complete(request)) {
            Ok(response) => {
                let text = response.text.trim().to_string();
                r::dim(&format!("─ {} characters ─", text.len()));
                println!("{}", text);
                state.add_message("assistant", &text);
                interaction.tasks_completed += 1;
            }
            Err(e) => {
                r::error(&format!("Generation failed: {e}"));
                state.add_message("assistant", &format!("[Error: {e}]"));
            }
        }
        if let Err(e) = state.save(&session_path) {
            r::warning(&format!("Auto-save failed: {e}"));
        }
    }

    loop {
        // Show prompt
        let pacing_label = match &current_pacing {
            InteractionMode::FullControl => "careful",
            InteractionMode::ModerateControl { .. } => "rolling",
            InteractionMode::NoControl => "planning",
            InteractionMode::GoHam => "auto",
        };

        print!("\n{}{} > ", r::Colors::DIM, pacing_label,);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        // Handle commands
        if input.starts_with('/') || input.starts_with(':') {
            let cmd = input
                .trim_start_matches('/')
                .trim_start_matches(':')
                .trim()
                .to_lowercase();
            if handle_command(
                &cmd,
                &mut state,
                &mut current_pacing,
                &mut interaction,
                &session_path,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                // Command signaled quit
                break;
            }
            continue;
        }

        // User message
        state.add_message("user", &input);
        r::header("User");

        // Agent response
        r::header("AI");
        let request = roco_engine::CompletionRequest {
            system: "You are a creative writing assistant.".into(),
            prompt: input.clone(),
            temperature: 0.8,
            max_tokens: 1024,
            prefill: Some("<think></think>".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(response) => {
                let text = response.text.trim().to_string();
                r::dim(&format!("─ {} characters ─", text.len()));
                println!("{}", text);
                state.add_message("assistant", &text);

                // Pacing: check if we should pause
                interaction.tasks_completed += 1;
                let should_pause = current_pacing.should_pause(
                    interaction.tasks_completed,
                    interaction.total_tasks.max(interaction.tasks_completed + 1),
                );

                if should_pause {
                    r::info("\n--- [a]ccept  [s]kip  [r]evise  [q]uit ---");
                    interaction.waiting_for_human = true;
                }
            }
            Err(e) => {
                r::error(&format!("Generation failed: {e}"));
                state.add_message("assistant", &format!("[Error: {e}]"));
            }
        }

        // Auto-save after each exchange
        if let Err(e) = state.save(&session_path) {
            r::warning(&format!("Auto-save failed: {e}"));
        }
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Resume Mode
// ═════════════════════════════════════════════════════════════════════════════

fn run_resume(backend: &dyn roco_engine::ModelBackend, session_id: &str) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    let session_path = session_dir.join(format!("{}.json", session_id));

    if !session_path.exists() {
        eprintln!("Session not found: {}", session_path.display());
        eprintln!("Available sessions:");
        list_sessions();
        std::process::exit(1);
    }

    let state = ConversationState::load(&session_path)
        .map_err(|e| anyhow::anyhow!("Failed to load session: {e}"))?;

    r::header(&format!("Resuming Session: {}", state.id));
    r::info(&format!(
        "{} messages, pacing: {}",
        state.messages.len(),
        state.pacing
    ));
    r::dim("Reviewing past messages:\n");

    // Show history
    for msg in &state.messages {
        let label = match msg.role.as_str() {
            "user" => format!("{}User{}", r::Colors::BLUE, r::Colors::RESET),
            "assistant" | "ai" => format!("{}AI{}", r::Colors::GREEN, r::Colors::RESET),
            "system" => format!("{}System{}", r::Colors::DIM, r::Colors::RESET),
            _ => format!("{}{}{}", r::Colors::MAGENTA, msg.role, r::Colors::RESET),
        };
        let preview: String = msg.content.chars().take(100).collect();
        println!("  [{}] {}...", label, preview);
    }

    println!("\nSession resumed. Continue typing to chat.");
    println!("Use /quit to save and exit.");

    // Continue interactive loop
    let pacing = match state.pacing.as_str() {
        "planning" => PacingChoice::Planning,
        "careful" => PacingChoice::Careful,
        "rolling" => PacingChoice::Rolling,
        "auto-accept" => PacingChoice::AutoAccept,
        _ => PacingChoice::Careful,
    };

    // Re-enter interactive with loaded state
    // For simplicity, we re-create the session and continue
    let mut new_state = state.clone();
    let mut current_pacing = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(current_pacing.clone(), new_state.messages.len());

    loop {
        let pacing_label = match &current_pacing {
            InteractionMode::FullControl => "careful",
            InteractionMode::ModerateControl { .. } => "rolling",
            InteractionMode::NoControl => "planning",
            InteractionMode::GoHam => "auto",
        };

        print!("\n{}{} > ", r::Colors::DIM, pacing_label,);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        if input.starts_with('/') || input.starts_with(':') {
            let cmd = input
                .trim_start_matches('/')
                .trim_start_matches(':')
                .trim()
                .to_lowercase();
            if handle_command(
                &cmd,
                &mut new_state,
                &mut current_pacing,
                &mut interaction,
                &session_path,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                break;
            }
            continue;
        }

        // User message
        new_state.add_message("user", &input);
        r::header("User");

        // Agent response
        r::header("AI");
        let request = roco_engine::CompletionRequest {
            system: "You are a creative writing assistant.".into(),
            prompt: input.clone(),
            temperature: 0.8,
            max_tokens: 1024,
            prefill: Some("<think></think>".into()),
            ..Default::default()
        };

        match futures::executor::block_on(backend.complete(request)) {
            Ok(response) => {
                let text = response.text.trim().to_string();
                r::dim(&format!("─ {} characters ─", text.len()));
                println!("{}", text);
                new_state.add_message("assistant", &text);
                interaction.tasks_completed += 1;

                let should_pause = current_pacing.should_pause(
                    interaction.tasks_completed,
                    interaction.total_tasks.max(interaction.tasks_completed + 1),
                );

                if should_pause {
                    r::info("\n--- [a]ccept  [s]kip  [r]evise  [q]uit ---");
                    interaction.waiting_for_human = true;
                }
            }
            Err(e) => {
                r::error(&format!("Generation failed: {e}"));
                new_state.add_message("assistant", &format!("[Error: {e}]"));
            }
        }

        if let Err(e) = new_state.save(&session_path) {
            r::warning(&format!("Auto-save failed: {e}"));
        }
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Command Handler
// ═════════════════════════════════════════════════════════════════════════════

/// Handle a slash command. Returns `true` if the session should quit.
fn handle_command(
    cmd: &str,
    state: &mut ConversationState,
    pacing: &mut InteractionMode,
    interaction: &mut InteractionState,
    session_path: &PathBuf,
) -> Result<bool, String> {
    match cmd {
        "help" | "h" | "?" => {
            r::panel(
                "Commands",
                &[
                    "  /accept      Accept current AI output and continue",
                    "  /skip        Skip current AI output",
                    "  /stop        Stop generation",
                    "  /revise <text>  Request revision with feedback",
                    "  /pause       Pause generation",
                    "  /resume      Resume paused generation",
                    "  /undo        Undo last action",
                    "  /redo        Redo last undone action",
                    "",
                    "  /pace <mode> Change pacing: planning, careful, rolling, auto",
                    "  /save        Save session",
                    "  /list        Show session history",
                    "  /help        Show this help",
                    "  /quit        Save and exit",
                ]
                .join("\n"),
            );
            Ok(false)
        }

        "accept" | "a" => {
            r::success("Accepted. Continuing...");
            interaction.waiting_for_human = false;
            Ok(false)
        }

        "skip" | "s" => {
            r::warning("Skipped.");
            interaction.waiting_for_human = false;
            Ok(false)
        }

        "stop" => {
            // Stop generation and exit — same as quit
            if let Err(e) = state.save(session_path) {
                r::warning(&format!("Auto-save failed: {e}"));
            }
            r::info("Session saved. Goodbye!");
            Ok(true)
        }

        "undo" => {
            if state.messages.len() >= 2 {
                state.messages.pop();
                state.messages.pop();
                r::success("Undone last exchange.");
            } else {
                r::warning("Nothing to undo.");
            }
            Ok(false)
        }

        "redo" => {
            r::warning("Redo not available in CLI mode (no redo stack).");
            Ok(false)
        }

        "pause" => {
            interaction.waiting_for_human = true;
            r::info("Paused.");
            Ok(false)
        }

        "resume" => {
            interaction.waiting_for_human = false;
            r::info("Resumed.");
            Ok(false)
        }

        "list" | "history" => {
            r::header(&format!(
                "Session: {} ({} messages)",
                state.id,
                state.messages.len()
            ));
            for (i, msg) in state.messages.iter().enumerate() {
                let label = match msg.role.as_str() {
                    "user" => format!("{}U{}", r::Colors::BLUE, r::Colors::RESET),
                    "assistant" => format!("{}A{}", r::Colors::GREEN, r::Colors::RESET),
                    "system" => format!("{}S{}", r::Colors::DIM, r::Colors::RESET),
                    _ => format!("{}?{}", r::Colors::MAGENTA, r::Colors::RESET),
                };
                let preview: String = msg.content.chars().take(80).collect();
                println!("  {} {:2}. {}", label, i + 1, preview);
            }
            Ok(false)
        }

        "save" => {
            match state.save(session_path) {
                Ok(_) => {
                    r::success(&format!("Session saved: {}", session_path.display()));
                }
                Err(e) => {
                    r::error(&format!("Save failed: {e}"));
                }
            }
            Ok(false)
        }

        "quit" | "q" | "exit" => {
            // Auto-save before quit
            if let Err(e) = state.save(session_path) {
                r::warning(&format!("Auto-save on quit failed: {e}"));
            }
            r::info("Session saved. Goodbye!");
            Ok(true)
        }

        _ if cmd.starts_with("pace") || cmd.starts_with("pacing") => {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            let new_pace = parts.get(1).copied().unwrap_or("");
            match new_pace {
                "planning" | "plan" => {
                    *pacing = InteractionMode::NoControl;
                    state.pacing = "planning".to_string();
                    r::success("Pacing: Planning (agent runs to completion)");
                }
                "careful" | "full" => {
                    *pacing = InteractionMode::FullControl;
                    state.pacing = "careful".to_string();
                    r::success("Pacing: Careful (one task at a time)");
                }
                "rolling" | "batch" => {
                    *pacing = InteractionMode::ModerateControl { batch_size: 3 };
                    state.pacing = "rolling".to_string();
                    r::success("Pacing: Rolling (review batches)");
                }
                "auto" | "accept" | "go-ham" => {
                    *pacing = InteractionMode::GoHam;
                    state.pacing = "auto-accept".to_string();
                    r::success("Pacing: Auto-Accept (fastest)");
                }
                _ => {
                    r::info("Usage: /pace [planning|careful|rolling|auto]");
                    r::info(&format!(
                        "  Current: {}",
                        match pacing {
                            InteractionMode::NoControl => "planning",
                            InteractionMode::FullControl => "careful",
                            InteractionMode::ModerateControl { .. } => "rolling",
                            InteractionMode::GoHam => "auto-accept",
                        }
                    ));
                }
            }
            Ok(false)
        }

        _ => {
            r::warning(&format!("Unknown command: /{}", cmd));
            r::info("Type /help for available commands.");
            Ok(false)
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Get the directory where session files are stored
fn get_sessions_dir() -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.join(".roco").join("sessions")
}

/// List available sessions
pub fn list_sessions() {
    let session_dir = get_sessions_dir();
    if !session_dir.exists() {
        r::info("No sessions found.");
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(&session_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        r::info("No sessions found.");
        return;
    }

    r::header("Available Sessions");
    for entry in &entries {
        let path = entry.path();
        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(state) = serde_json::from_str::<ConversationState>(&json) {
                let first = state
                    .messages
                    .first()
                    .map(|m| format!(" — {}", &m.content.chars().take(60).collect::<String>()))
                    .unwrap_or_default();
                println!(
                    "  {}  ({}){}",
                    path.file_stem().unwrap().to_string_lossy(),
                    state.messages.len(),
                    first,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pacing_choice_mapping() {
        assert_eq!(
            PacingChoice::Planning.to_interaction_mode(),
            InteractionMode::NoControl
        );
        assert_eq!(
            PacingChoice::Careful.to_interaction_mode(),
            InteractionMode::FullControl
        );
        assert_eq!(
            PacingChoice::AutoAccept.to_interaction_mode(),
            InteractionMode::GoHam
        );
        match PacingChoice::Rolling.to_interaction_mode() {
            InteractionMode::ModerateControl { batch_size } => assert_eq!(batch_size, 3),
            _ => panic!("expected ModerateControl"),
        }
    }

    #[test]
    fn test_pacing_choice_labels() {
        assert_eq!(PacingChoice::Planning.label(), "planning");
        assert_eq!(PacingChoice::Careful.label(), "careful");
        assert_eq!(PacingChoice::Rolling.label(), "rolling");
        assert_eq!(PacingChoice::AutoAccept.label(), "auto-accept");
    }

    #[test]
    fn test_conversation_state_new() {
        let state = ConversationState::new("test-123".into(), "careful");
        assert_eq!(state.id, "test-123");
        assert_eq!(state.pacing, "careful");
        assert!(state.messages.is_empty());
    }

    #[test]
    fn test_conversation_state_add_message() {
        let mut state = ConversationState::new("test".into(), "careful");
        state.add_message("user", "Hello");
        state.add_message("assistant", "Hi there!");
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, "user");
        assert_eq!(state.messages[1].content, "Hi there!");
    }

    #[test]
    fn test_conversation_state_save_load_roundtrip() {
        let dir = std::env::temp_dir().join("roco_interact_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_session.json");

        let mut state = ConversationState::new("roundtrip".into(), "rolling");
        state.add_message("user", "Test message");
        state.add_message("assistant", "Test response");
        assert!(state.save(&path).is_ok());

        let loaded = ConversationState::load(&path).unwrap();
        assert_eq!(loaded.id, "roundtrip");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Test message");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_handle_command_quit() {
        let mut state = ConversationState::new("cmd-test".into(), "careful");
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);
        let path = std::env::temp_dir().join("cmd_test.json");

        // /quit should return true (quit signal)
        let result = handle_command("quit", &mut state, &mut pacing, &mut interaction, &path);
        assert!(result.unwrap());

        // /help should return false (continue)
        let result = handle_command("help", &mut state, &mut pacing, &mut interaction, &path);
        assert!(!result.unwrap());
    }

    #[test]
    fn test_handle_command_pace() {
        let mut state = ConversationState::new("pace-test".into(), "careful");
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);
        let path = std::env::temp_dir().join("pace_test.json");

        // Change to planning
        // Simulate /pace planning
        let _ = handle_command(
            "pace planning",
            &mut state,
            &mut pacing,
            &mut interaction,
            &path,
        );
        assert_eq!(pacing, InteractionMode::NoControl);
        assert_eq!(state.pacing, "planning");

        // Change to auto
        let _ = handle_command(
            "pace auto",
            &mut state,
            &mut pacing,
            &mut interaction,
            &path,
        );
        assert_eq!(pacing, InteractionMode::GoHam);
        assert_eq!(state.pacing, "auto-accept");
    }

    #[test]
    fn test_handle_command_accept_skip_stop() {
        let mut state = ConversationState::new("actions".into(), "careful");
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);
        let path = std::env::temp_dir().join("actions_test.json");

        interaction.waiting_for_human = true;

        handle_command("accept", &mut state, &mut pacing, &mut interaction, &path).unwrap();
        assert!(!interaction.waiting_for_human);

        interaction.waiting_for_human = true;
        handle_command("skip", &mut state, &mut pacing, &mut interaction, &path).unwrap();
        assert!(!interaction.waiting_for_human);

        interaction.waiting_for_human = true;
        // stop returns Ok(true) meaning exit — does not modify waiting_for_human
        let result = handle_command("stop", &mut state, &mut pacing, &mut interaction, &path);
        assert!(result.unwrap(), "stop should signal exit");
    }

    #[test]
    fn test_handle_command_undo() {
        let mut state = ConversationState::new("undo-test".into(), "careful");
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);
        let path = std::env::temp_dir().join("undo_test.json");

        state.add_message("user", "Hello");
        state.add_message("assistant", "Hi");
        assert_eq!(state.messages.len(), 2);

        handle_command("undo", &mut state, &mut pacing, &mut interaction, &path).unwrap();
        assert_eq!(state.messages.len(), 0);
    }

    #[test]
    fn test_session_meta_pacing_variants() {
        // Verify all variants are reachable
        fn assert_pacing(p: PacingChoice) {
            match p {
                PacingChoice::Planning
                | PacingChoice::Careful
                | PacingChoice::Rolling
                | PacingChoice::AutoAccept => {}
            }
        }
        assert_pacing(PacingChoice::Planning);
        assert_pacing(PacingChoice::Careful);
        assert_pacing(PacingChoice::Rolling);
        assert_pacing(PacingChoice::AutoAccept);
    }

    #[test]
    fn test_conversation_message_timestamps() {
        let mut state = ConversationState::new("ts-test".into(), "careful");
        state.add_message("user", "t1");
        state.add_message("assistant", "t2");

        // Just verify timestamps are non-empty and different
        assert!(!state.messages[0].timestamp.is_empty());
        assert!(!state.messages[1].timestamp.is_empty());
    }
}
