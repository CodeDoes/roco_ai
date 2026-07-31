//! Session subcommand: `roco session` — persistent multi-turn chat control.
//!
//! Provides CLI interfaces to create persistent session IDs and run individual
//! prompts against them, appending messages to a local session transcript file.

use crate::conversation::ChatSession;
use crate::interact_cli::{get_sessions_dir, CHAT_PERSONA};
use crate::{daemon, parse_opt};
use roco_protocol::ConversationState;

pub fn cmd_session(extra: &[&str]) {
    let command = extra.first().copied().unwrap_or("help");

    match command {
        "help" | "--help" | "-h" => {
            print_help();
        }
        "create" => {
            let session_id = format!("session_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
            let session_dir = get_sessions_dir();
            let session_path = session_dir.join(format!("{}.json", session_id));

            let state = ConversationState::new(session_id.clone(), "auto-accept");
            if let Err(e) = state.save(&session_path) {
                eprintln!("Error creating session file: {e}");
                std::process::exit(1);
            }

            println!("{session_id}");
        }
        session_id => {
            // Check for prompt parameter
            let prompt = parse_opt("-p", extra)
                .or_else(|| parse_opt("--prompt", extra))
                .unwrap_or("");

            if prompt.is_empty() {
                eprintln!(
                    "Error: -p or --prompt option is required when interacting with a session."
                );
                eprintln!("Usage: roco session <session_id> -p \"prompt_text\"");
                std::process::exit(1);
            }

            let session_dir = get_sessions_dir();
            let session_path = session_dir.join(format!("{}.json", session_id));

            if !session_path.exists() {
                eprintln!("Error: Session not found: {session_id}");
                std::process::exit(1);
            }

            let state = match ConversationState::load(&session_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error loading session file: {e}");
                    std::process::exit(1);
                }
            };

            let backend = daemon::ensure_sync_backend();
            let mut chat = ChatSession::new(state, session_path, CHAT_PERSONA, &*backend);

            // Execute the turn. ChatSession::turn streams output and saves automatically.
            chat.turn(&*backend, prompt);
        }
    }
}

fn print_help() {
    println!("RoCo AI — Session Management\n");
    println!("Usage:");
    println!(
        "  roco session create                      Create a new chat session and print its ID"
    );
    println!(
        "  roco session <session_id> -p \"<prompt>\"   Resume a session and run a single turn\n"
    );
    println!("Options:");
    println!("  -p, --prompt <string>                    The chat prompt text to append\n");
    println!("Examples:");
    println!("  roco session create");
    println!("  roco session session_20260801_120000 -p \"Hi, remember that I am Ada\"");
}
