//! Story mode — interactive story writing assistant.
//!
//! This is the CLI entry point for story mode. It wraps
//! `roco_validation::agent::StoryModeAgent` and provides a REPL-like
//! interface where the user can:
//!
//! - Lock into a story workspace
//! - Validate chapters, outlines, and wiki
//! - Summarize and condense story content
//! - Search for information
//! - Edit and revise chapters
//! - Manage outlines and plan modifications
//! - Brainstorm new story ideas
//!
//! # Slash commands
//!
//! Every natural language input goes through the model for intent
//! classification. The only bypass is slash commands:
//!
//! - `/validate [N|outline|wiki]`
//! - `/summarize [N|all|story]`
//! - `/status`
//! - `/diff`
//! - `/plan`
//! - `/brainstorm [prompt]`
//! - `/switch <name>`
//! - `/lock <name>`
//! - `/unlock`
//! - `/help`

use roco_validation::agent::StoryModeAgent;
use roco_validation::intent::print_slash_help;

use crate::daemon;

/// Run the story mode interface.
///
/// If `story_name` is provided, lock into that story immediately.
/// Otherwise, wait for the user to request a story lock.
pub fn run_story_mode(story_name: Option<&str>) {
    let backend = daemon::ensure_sync_backend();
    let mut agent = StoryModeAgent::new();

    // Lock into story if name provided
    if let Some(name) = story_name {
        match agent.process(&*backend, &format!("/lock {name}")) {
            Ok(result) => println!("{}", result.display()),
            Err(e) => {
                eprintln!("Error: {e}");
                eprintln!("Use 'let's work on [story]' to start, or provide a workspace path.");
            }
        }
    }

    // ── Interactive REPL ────────────────────────────────────────────────
    println!();
    println!("📖 RoCo Story Mode");
    println!("   Type '/help' for commands, '/unlock' to exit story mode.");
    println!();

    loop {
        // Show prompt
        let prompt_prefix = match agent.active_story_name() {
            Some(name) => format!("📖 {name}"),
            None => "✨ RoCo".to_string(),
        };

        print!("{prompt_prefix}> ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        // Read input
        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Handle special commands
        match trimmed {
            "/help" | "/h" | "/?" => {
                print_slash_help();
                continue;
            }
            "/quit" | "/exit" | "/q" => {
                println!("Goodbye!");
                break;
            }
            _ => {}
        }

        // Process through the agent
        match agent.process(&*backend, trimmed) {
            Ok(result) => {
                let display = result.display();
                println!("{}", display);
                println!();
            }
            Err(e) => {
                eprintln!("Error: {e}");
                println!();
            }
        }
    }
}

/// Run a single story mode command and exit.
///
/// Used for one-shot CLI invocation: `roco story validate 1`
pub fn run_story_command(story_name: Option<&str>, command: &str) {
    let backend = daemon::ensure_sync_backend();
    let mut agent = StoryModeAgent::new();

    // Lock into story if name provided
    if let Some(name) = story_name {
        match agent.process(&*backend, &format!("/lock {name}")) {
            Ok(result) => {
                if !result.display().contains("Locked") {
                    eprintln!("Warning: Could not lock into story '{name}'.");
                }
            }
            Err(e) => {
                eprintln!("Error locking story: {e}");
                return;
            }
        }
    }

    // Run the command
    match agent.process(&*backend, command) {
        Ok(result) => {
            println!("{}", result.display());
        }
        Err(e) => {
            eprintln!("Error: {e}");
        }
    }
}
