//! `roco interact` — the interactive chat surface.
//!
//! Three modes, one conversation engine ([`crate::conversation::ChatSession`]):
//!
//! - **Interactive** (default): streaming REPL with pacing controls, slash
//!   commands, identity awareness and session persistence.
//! - **Prompt** (`--prompt "text"`): one-shot generation, saves the session,
//!   prints the result, exits.
//! - **Resume** (`--resume <id>`): reload a transcript and carry on.
//!
//! All three previously duplicated prompt-building, backend invocation and
//! response cleanup — with subtly different bugs in each. They now share
//! `ChatSession`, so streaming, context budgeting, identity handling and
//! auto-save behave identically everywhere.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use roco_agent::interaction::{InteractionMode, InteractionState};
use roco_protocol::ConversationState;

use crate::conversation::{ChatSession, TurnOutcome};
use crate::rich_output as r;

/// Session files kept on disk. Older ones are pruned on startup so a daily
/// user doesn't accumulate an unbounded `.roco/sessions/` directory.
pub const MAX_SAVED_SESSIONS: usize = 100;

/// Default persona for the chat surface. The identity preamble is prepended
/// by `ChatSession`, so this only describes *behaviour*.
const CHAT_PERSONA: &str = "\
Hold a natural conversation.
- Answer the user's actual question first, then add detail if it helps.
- Match their tone and length: a short question gets a short answer.
- Use the conversation history — refer back to what was already said.
- If you don't know something, say so instead of inventing it.
- Write one reply as yourself. Never write the user's next message.";

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// How to run the interactive session.
#[derive(Debug, Clone)]
pub enum InteractMode {
    /// One-shot prompt: generate, save, print, exit.
    Prompt { prompt: String },
    /// Full interactive REPL with pacing control.
    Interactive {
        pacing: PacingChoice,
        prompt: Option<String>,
    },
    /// Resume a previous session by ID.
    Resume { session_id: String, instant: bool },
}

/// Initial pacing mode for interactive sessions.
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

    /// Parse the pacing recorded in a saved session.
    pub fn from_label(label: &str) -> Self {
        match label {
            "planning" | "plan" => PacingChoice::Planning,
            "rolling" | "batch" => PacingChoice::Rolling,
            "auto-accept" | "auto" => PacingChoice::AutoAccept,
            _ => PacingChoice::Careful,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Entry point
// ═════════════════════════════════════════════════════════════════════════════

/// Run the interactive CLI. Entry point for `roco interact`.
pub fn run(mode: InteractMode, backend: &dyn roco_engine::ModelBackend) -> anyhow::Result<()> {
    // Journal init is idempotent; surfaces may be entered directly.
    let _ = roco_app::agent_journal::AgentJournal::init();

    match mode {
        InteractMode::Prompt { prompt } => run_prompt(backend, &prompt),
        InteractMode::Interactive { pacing, prompt } => run_interactive(backend, pacing, prompt),
        InteractMode::Resume { session_id, instant } => run_resume(backend, &session_id, instant),
    }
}

/// Like [`run`] but with a deterministic seed for reproducible sampling.
pub fn run_with_seed(
    mode: InteractMode,
    backend: &dyn roco_engine::ModelBackend,
    seed: u64,
) -> anyhow::Result<()> {
    let _ = roco_app::agent_journal::AgentJournal::init();

    match mode {
        InteractMode::Prompt { prompt } => run_prompt_with_seed(backend, &prompt, seed),
        InteractMode::Interactive { pacing, prompt } => {
            run_interactive_with_seed(backend, pacing, prompt, seed)
        }
        InteractMode::Resume { session_id, instant } => {
            run_resume_with_seed(backend, &session_id, seed, instant)
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Prompt Mode
// ═════════════════════════════════════════════════════════════════════════════

fn run_prompt(backend: &dyn roco_engine::ModelBackend, prompt: &str) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    prune_old_sessions(&session_dir, MAX_SAVED_SESSIONS);

    let session_id = format!("prompt_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_path = session_dir.join(format!("{}.json", session_id));

    r::header("RoCo AI — Prompt");
    r::info(&format!("Prompt: {}", prompt));
    r::dim(&format!("Session: {}", session_id));
    println!();

    let state = ConversationState::new(session_id.clone(), "auto-accept");
    let mut chat = ChatSession::new(state, session_path.clone(), CHAT_PERSONA, backend);

    // Streams straight to the terminal as tokens arrive.
    chat.turn(backend, prompt);

    r::success(&format!("Session saved: {}", session_path.display()));
    Ok(())
}

fn run_prompt_with_seed(
    backend: &dyn roco_engine::ModelBackend,
    prompt: &str,
    seed: u64,
) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    prune_old_sessions(&session_dir, MAX_SAVED_SESSIONS);

    let session_id = format!("prompt_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_path = session_dir.join(format!("{}.json", session_id));

    r::header("RoCo AI — Prompt (deterministic)");
    r::info(&format!("Prompt: {}", prompt));
    r::info(&format!("Seed: {}", seed));
    r::dim(&format!("Session: {}", session_id));
    println!();

    let state = ConversationState::new(session_id.clone(), "auto-accept");
    let mut chat =
        ChatSession::new(state, session_path.clone(), CHAT_PERSONA, backend).with_seed(seed);

    chat.turn(backend, prompt);

    r::success(&format!("Session saved: {}", session_path.display()));
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Interactive Mode
// ═════════════════════════════════════════════════════════════════════════════

fn run_interactive(
    backend: &dyn roco_engine::ModelBackend,
    pacing: PacingChoice,
    initial_prompt: Option<String>,
) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    prune_old_sessions(&session_dir, MAX_SAVED_SESSIONS);

    let session_id = format!("interact_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_path = session_dir.join(format!("{}.json", session_id));

    let state = ConversationState::new(session_id, pacing.label());
    let mut chat = ChatSession::new(state, session_path, CHAT_PERSONA, backend);

    r::header("RoCo AI — Chat");
    println!("{}", chat.greeting());
    r::dim("  Type your message and press Enter.  :h for help, :q to quit.");

    let mut pacing_mode = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(pacing_mode.clone(), 0);

    if let Some(initial) = initial_prompt {
        println!("\n{}You:{} {}", r::Colors::BOLD, r::Colors::RESET, initial);
        run_turn(&mut chat, backend, &initial, &mut interaction, &pacing_mode);
    }

    repl_loop(&mut chat, backend, &mut pacing_mode, &mut interaction)
}

fn run_interactive_with_seed(
    backend: &dyn roco_engine::ModelBackend,
    pacing: PacingChoice,
    initial_prompt: Option<String>,
    seed: u64,
) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    std::fs::create_dir_all(&session_dir)?;
    prune_old_sessions(&session_dir, MAX_SAVED_SESSIONS);

    let session_id = format!("interact_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_path = session_dir.join(format!("{}.json", session_id));

    let state = ConversationState::new(session_id, pacing.label());
    let mut chat = ChatSession::new(state, session_path, CHAT_PERSONA, backend).with_seed(seed);

    r::header("RoCo AI — Chat (deterministic)");
    r::info(&format!("Seed: {}", seed));
    println!("{}", chat.greeting());
    r::dim("  Type your message and press Enter.  :h for help, :q to quit.");

    let mut pacing_mode = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(pacing_mode.clone(), 0);

    if let Some(initial) = initial_prompt {
        println!("\n{}You:{} {}", r::Colors::BOLD, r::Colors::RESET, initial);
        run_turn(&mut chat, backend, &initial, &mut interaction, &pacing_mode);
    }

    repl_loop(&mut chat, backend, &mut pacing_mode, &mut interaction)
}

// ═════════════════════════════════════════════════════════════════════════════
// Resume Mode
// ═════════════════════════════════════════════════════════════════════════════

fn run_resume(
    backend: &dyn roco_engine::ModelBackend,
    session_id: &str,
    instant: bool,
) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    let session_path = session_dir.join(format!("{}.json", session_id));
    let state_path = session_dir.join(format!("{}.state", session_id));

    if !session_path.exists() {
        eprintln!("Session not found: {}", session_path.display());
        eprintln!("Available sessions:");
        list_sessions();
        std::process::exit(1);
    }

    let state = ConversationState::load(&session_path)
        .map_err(|e| anyhow::anyhow!("Failed to load session: {e}"))?;

    r::header(&format!("Resuming Session: {}", state.id));
    if instant {
        r::info("Mode: instant resume (loading saved backend state)");
    }
    r::info(&format!(
        "{} messages, pacing: {}",
        state.messages.len(),
        state.pacing
    ));
    r::dim("Reviewing past messages:\n");

    // Show the tail of the transcript — replaying hundreds of messages helps
    // nobody and floods the scrollback.
    let shown = state.messages.len().min(12);
    if state.messages.len() > shown {
        r::dim(&format!(
            "  … {} earlier messages omitted",
            state.messages.len() - shown
        ));
    }
    for msg in state.messages.iter().skip(state.messages.len() - shown) {
        let label = match msg.role.as_str() {
            "user" => format!("{}User{}", r::Colors::BLUE, r::Colors::RESET),
            "assistant" | "ai" => format!("{}AI{}", r::Colors::GREEN, r::Colors::RESET),
            "system" => format!("{}System{}", r::Colors::DIM, r::Colors::RESET),
            _ => format!("{}{}{}", r::Colors::MAGENTA, msg.role, r::Colors::RESET),
        };
        let preview: String = msg.content.chars().take(100).collect();
        println!("  [{}] {}...", label, preview);
    }

    let pacing = PacingChoice::from_label(&state.pacing);
    println!("\nSession resumed. Continue typing to chat.");
    println!("Use /quit to save and exit.");

    let mut pacing_mode = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(pacing_mode.clone(), state.messages.len());
    let mut chat = ChatSession::new(state, session_path, CHAT_PERSONA, backend)
        .with_state_file(state_path);

    // Instant resume: load saved backend state so the first turn skips
    // context history. Gracefully fall back to replay if the state file is
    // missing or corrupt.
    if instant {
        if let Ok(saved) = std::fs::read(&chat.state_file.as_ref().unwrap()) {
            if futures::executor::block_on(backend.load_state(saved)).is_ok() {
                chat = chat.with_instant_resume();
                r::info("Instant resume: backend state loaded.");
            } else {
                r::warning("Instant resume: state file corrupt, falling back to replay.");
            }
        } else {
            r::warning("Instant resume: no state file found, falling back to replay.");
        }
    }

    repl_loop(&mut chat, backend, &mut pacing_mode, &mut interaction)
}

fn run_resume_with_seed(
    backend: &dyn roco_engine::ModelBackend,
    session_id: &str,
    seed: u64,
    instant: bool,
) -> anyhow::Result<()> {
    let session_dir = get_sessions_dir();
    let session_path = session_dir.join(format!("{}.json", session_id));
    let state_path = session_dir.join(format!("{}.state", session_id));

    if !session_path.exists() {
        eprintln!("Session not found: {}", session_path.display());
        eprintln!("Available sessions:");
        list_sessions();
        std::process::exit(1);
    }

    let state = ConversationState::load(&session_path)
        .map_err(|e| anyhow::anyhow!("Failed to load session: {e}"))?;

    r::header(&format!(
        "Resuming Session: {} (deterministic, seed={})",
        state.id, seed
    ));
    if instant {
        r::info("Mode: instant resume (loading saved backend state)");
    }
    r::info(&format!(
        "{} messages, pacing: {}",
        state.messages.len(),
        state.pacing
    ));

    let pacing = PacingChoice::from_label(&state.pacing);
    let mut pacing_mode = pacing.to_interaction_mode();
    let mut interaction = InteractionState::new(pacing_mode.clone(), state.messages.len());
    let mut chat = ChatSession::new(state, session_path, CHAT_PERSONA, backend)
        .with_seed(seed)
        .with_state_file(state_path);

    // Instant resume: load saved backend state so the first turn skips
    // context history. Gracefully fall back to replay if the state file is
    // missing or corrupt.
    if instant {
        if let Ok(saved) = std::fs::read(&chat.state_file.as_ref().unwrap()) {
            if futures::executor::block_on(backend.load_state(saved)).is_ok() {
                chat = chat.with_instant_resume();
                r::info("Instant resume: backend state loaded.");
            } else {
                r::warning("Instant resume: state file corrupt, falling back to replay.");
            }
        } else {
            r::warning("Instant resume: no state file found, falling back to replay.");
        }
    }

    repl_loop(&mut chat, backend, &mut pacing_mode, &mut interaction)
}

// ═════════════════════════════════════════════════════════════════════════════
// Shared REPL
// ═════════════════════════════════════════════════════════════════════════════

/// The read/eval/print loop shared by interactive and resume modes.
fn repl_loop(
    chat: &mut ChatSession,
    backend: &dyn roco_engine::ModelBackend,
    pacing: &mut InteractionMode,
    interaction: &mut InteractionState,
) -> anyhow::Result<()> {
    // One reusable input buffer: allocating a fresh String per iteration in a
    // long REPL is needless churn.
    let mut buf = String::new();

    loop {
        print!("\n{}You:{} ", r::Colors::BOLD, r::Colors::RESET);
        io::stdout().flush()?;

        buf.clear();
        if io::stdin().read_line(&mut buf)? == 0 {
            // EOF (piped stdin exhausted, or Ctrl-D). Save and leave cleanly.
            chat.save();
            break;
        }
        let input = buf.trim();
        if input.is_empty() {
            continue;
        }

        if let Some(cmd) = as_command(input) {
            if handle_command(&cmd, chat, pacing, interaction)
                .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                break;
            }
            continue;
        }

        let input = input.to_string();
        run_turn(chat, backend, &input, interaction, pacing);
    }

    Ok(())
}

/// Strip a leading `/` or `:` and normalise, or `None` for ordinary text.
fn as_command(input: &str) -> Option<String> {
    if !(input.starts_with('/') || input.starts_with(':')) {
        return None;
    }
    Some(
        input
            .trim_start_matches('/')
            .trim_start_matches(':')
            .trim()
            .to_lowercase(),
    )
}

/// Execute one conversational turn and apply pacing.
fn run_turn(
    chat: &mut ChatSession,
    backend: &dyn roco_engine::ModelBackend,
    input: &str,
    interaction: &mut InteractionState,
    pacing: &InteractionMode,
) {
    let outcome = chat.turn(backend, input);

    // Only real generations count toward pacing — an instant identity answer
    // shouldn't trigger a "review this batch" pause.
    if !matches!(outcome, TurnOutcome::Generated(_)) {
        return;
    }

    interaction.tasks_completed += 1;
    let should_pause = pacing.should_pause(
        interaction.tasks_completed,
        interaction.total_tasks.max(interaction.tasks_completed + 1),
    );
    if should_pause {
        r::dim("  [a]ccept  [s]kip  [q]uit");
        interaction.waiting_for_human = true;
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Command handler
// ═════════════════════════════════════════════════════════════════════════════

/// Handle a slash command. Returns `true` if the session should quit.
pub fn handle_command(
    cmd: &str,
    chat: &mut ChatSession,
    pacing: &mut InteractionMode,
    interaction: &mut InteractionState,
) -> Result<bool, String> {
    // Identity commands (`:whoami`, `:remember …`, `:name …`, `:forget`) are
    // shared with every other surface.
    if let Some(reply) = chat.identity_command(cmd) {
        println!("{reply}");
        return Ok(false);
    }

    match cmd {
        "help" | "h" | "?" => {
            r::panel(
                "Commands",
                &[
                    "  /accept      Accept current AI output and continue",
                    "  /skip        Skip current AI output",
                    "  /stop        Stop generation",
                    "  /pause       Pause generation",
                    "  /resume      Resume paused generation",
                    "  /undo        Undo last exchange",
                    "  /redo        Redo last undone action",
                    "  /clear       Clear the conversation",
                    "",
                    "  /whoami      What RoCo knows about you",
                    "  /whois       What RoCo is",
                    "  /name <you>  Tell RoCo your name",
                    "  /remember <fact>  Remember something about you",
                    "  /forget      Forget everything about you",
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
            chat.save();
            r::info("Session saved. Goodbye!");
            Ok(true)
        }

        "undo" => {
            if chat.undo() {
                r::success("Undone last exchange.");
            } else {
                r::warning("Nothing to undo.");
            }
            Ok(false)
        }

        "clear" => {
            chat.clear();
            chat.save();
            r::success("Conversation cleared.");
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
                chat.state.id,
                chat.state.messages.len()
            ));
            for (i, msg) in chat.state.messages.iter().enumerate() {
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
            match chat.state.save(&chat.path) {
                Ok(_) => r::success(&format!("Session saved: {}", chat.path.display())),
                Err(e) => r::error(&format!("Save failed: {e}")),
            }
            Ok(false)
        }

        "quit" | "q" | "exit" => {
            chat.save();
            r::info("Session saved. Goodbye!");
            Ok(true)
        }

        _ if cmd.starts_with("pace") || cmd.starts_with("pacing") => {
            let new_pace = cmd.split_whitespace().nth(1).unwrap_or("");
            match new_pace {
                "planning" | "plan" => {
                    *pacing = InteractionMode::NoControl;
                    chat.state.pacing = "planning".into();
                    r::success("Pacing: Planning (agent runs to completion)");
                }
                "careful" | "full" => {
                    *pacing = InteractionMode::FullControl;
                    chat.state.pacing = "careful".into();
                    r::success("Pacing: Careful (one task at a time)");
                }
                "rolling" | "batch" => {
                    *pacing = InteractionMode::ModerateControl { batch_size: 3 };
                    chat.state.pacing = "rolling".into();
                    r::success("Pacing: Rolling (batches of 3)");
                }
                "auto" | "auto-accept" | "ham" => {
                    *pacing = InteractionMode::GoHam;
                    chat.state.pacing = "auto-accept".into();
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
// Session storage
// ═════════════════════════════════════════════════════════════════════════════

/// Directory where session transcripts live.
pub fn get_sessions_dir() -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.join(".roco").join("sessions")
}

/// Delete the oldest session files beyond `keep`.
///
/// Every run of `roco interact` writes a new `.json`, so without pruning
/// `.roco/sessions/` grows forever — a slow but real storage leak (and it
/// makes `--list-sessions` progressively slower, since listing parses every
/// file). Sorting by filename works because IDs are timestamp-prefixed; we
/// fall back to mtime when a name doesn't parse.
pub fn prune_old_sessions(dir: &Path, keep: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .map(|e| {
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (mtime, e.path())
        })
        .collect();

    if files.len() <= keep {
        return 0;
    }

    // Oldest first, then delete from the front.
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let remove = files.len() - keep;
    let mut removed = 0;
    for (_, path) in files.into_iter().take(remove) {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// List available sessions.
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
        let Ok(json) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(state) = serde_json::from_str::<ConversationState>(&json) else {
            continue;
        };
        let first = state
            .messages
            .first()
            .map(|m| format!(" — {}", &m.content.chars().take(60).collect::<String>()))
            .unwrap_or_default();
        println!(
            "  {}  ({}){}",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            state.messages.len(),
            first,
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    fn test_chat(dir: &Path) -> ChatSession {
        let backend = MockBackend::default();
        ChatSession::new(
            ConversationState::new("test".into(), "careful"),
            dir.join("session.json"),
            CHAT_PERSONA,
            &backend,
        )
        .with_profile_path(dir.join("profile.json"))
        .quiet(true)
    }

    // ── Pacing ───────────────────────────────────────────────────────────

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
    fn test_pacing_label_roundtrip() {
        for p in [
            PacingChoice::Planning,
            PacingChoice::Careful,
            PacingChoice::Rolling,
            PacingChoice::AutoAccept,
        ] {
            assert_eq!(PacingChoice::from_label(p.label()), p);
        }
        // Unknown labels fall back to careful.
        assert_eq!(PacingChoice::from_label("nonsense"), PacingChoice::Careful);
    }

    // ── ConversationState ────────────────────────────────────────────────

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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_session.json");

        let mut state = ConversationState::new("roundtrip".into(), "rolling");
        state.add_message("user", "Test message");
        state.add_message("assistant", "Test response");
        assert!(state.save(&path).is_ok());

        let loaded = ConversationState::load(&path).unwrap();
        assert_eq!(loaded.id, "roundtrip");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Test message");
    }

    // ── Command parsing ──────────────────────────────────────────────────

    #[test]
    fn test_as_command_recognises_both_prefixes() {
        assert_eq!(as_command("/Quit").as_deref(), Some("quit"));
        assert_eq!(as_command(":HELP").as_deref(), Some("help"));
        assert_eq!(as_command("  :pace rolling").as_deref(), None); // leading space => text
        assert_eq!(as_command("hello").as_deref(), None);
        assert_eq!(as_command("/pace rolling").as_deref(), Some("pace rolling"));
    }

    // ── Command handling ─────────────────────────────────────────────────

    #[test]
    fn test_handle_command_quit() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        assert!(handle_command("quit", &mut chat, &mut pacing, &mut interaction).unwrap());
        assert!(!handle_command("help", &mut chat, &mut pacing, &mut interaction).unwrap());
    }

    #[test]
    fn test_handle_command_pace() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        handle_command("pace planning", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert_eq!(pacing, InteractionMode::NoControl);
        assert_eq!(chat.state.pacing, "planning");

        handle_command("pace auto", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert_eq!(pacing, InteractionMode::GoHam);
        assert_eq!(chat.state.pacing, "auto-accept");

        handle_command("pace rolling", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert_eq!(pacing, InteractionMode::ModerateControl { batch_size: 3 });
    }

    #[test]
    fn test_handle_command_accept_skip_stop() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        interaction.waiting_for_human = true;
        handle_command("accept", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert!(!interaction.waiting_for_human);

        interaction.waiting_for_human = true;
        handle_command("skip", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert!(!interaction.waiting_for_human);

        interaction.waiting_for_human = true;
        let result = handle_command("stop", &mut chat, &mut pacing, &mut interaction);
        assert!(result.unwrap(), "stop should signal exit");
    }

    #[test]
    fn test_handle_command_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        chat.push("user", "Hello");
        chat.push("assistant", "Hi");
        assert_eq!(chat.state.messages.len(), 2);

        handle_command("undo", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert_eq!(chat.state.messages.len(), 0);
    }

    #[test]
    fn test_handle_command_identity_is_routed_to_chat_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        assert!(!handle_command("name Ada", &mut chat, &mut pacing, &mut interaction).unwrap());
        assert_eq!(chat.profile().name.as_deref(), Some("Ada"));
        assert!(!handle_command("whoami", &mut chat, &mut pacing, &mut interaction).unwrap());
    }

    #[test]
    fn test_handle_command_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mut chat = test_chat(dir.path());
        let mut pacing = InteractionMode::FullControl;
        let mut interaction = InteractionState::new(pacing.clone(), 0);

        chat.push("user", "a");
        chat.push("assistant", "b");
        handle_command("clear", &mut chat, &mut pacing, &mut interaction).unwrap();
        assert!(chat.state.messages.is_empty());
    }

    // ── Session pruning (storage leak regression) ────────────────────────

    #[test]
    fn prune_removes_only_the_oldest_beyond_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        for i in 0..10 {
            let path = root.join(format!("s{i:02}.json"));
            let mut state = ConversationState::new(format!("s{i:02}"), "careful");
            state.add_message("user", "hi");
            state.save(&path).unwrap();
        }

        let removed = prune_old_sessions(root, 4);
        assert_eq!(removed, 6);

        let left: Vec<String> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".json"))
            .collect();
        assert_eq!(left.len(), 4, "got {left:?}");
        assert!(
            left.contains(&"s09.json".to_string()),
            "newest kept: {left:?}"
        );
        assert!(!left.contains(&"s00.json".to_string()), "oldest dropped");
    }

    #[test]
    fn prune_is_a_noop_under_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..3 {
            ConversationState::new(format!("s{i}"), "careful")
                .save(&dir.path().join(format!("s{i}.json")))
                .unwrap();
        }
        assert_eq!(prune_old_sessions(dir.path(), 10), 0);
    }

    #[test]
    fn prune_tolerates_a_missing_directory() {
        assert_eq!(prune_old_sessions(Path::new("/nonexistent/roco/xyz"), 5), 0);
    }
}
