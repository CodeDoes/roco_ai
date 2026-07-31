//! Session management subcommand: `roco session`
//!
//! Provides explicit session lifecycle management:
//! - `roco session create` — create a new session and print its ID
//! - `roco session <id> -p "prompt"` — send a prompt to a session
//! - `roco session list` — list all sessions
//! - `roco session show <id>` — show session transcript
//! - `roco session delete <id>` — delete a session

use std::path::PathBuf;

use crate::interact_cli::{self, InteractMode};
use crate::{daemon, parse_opt};
use roco_protocol::ConversationState;

const SESSIONS_DIR: &str = ".roco/sessions";

/// Entry point for `roco session` subcommand.
pub fn cmd_session(extra: &[&str]) {
    let sub = extra.first().copied().unwrap_or("list");
    let args: Vec<&str> = extra[if extra.first().map(|s| *s == sub).unwrap_or(false) {
        1..
    } else {
        0..
    }]
    .to_vec();

    match sub {
        "create" => cmd_session_create(&args),
        "list" => cmd_session_list(),
        "show" => cmd_session_show(&args),
        "delete" => cmd_session_delete(&args),
        id if !id.is_empty() => cmd_session_chat(id, &args),
        _ => {
            eprintln!("Usage:");
            eprintln!("  roco session create              Create a new session");
            eprintln!("  roco session <id> -p \"prompt\"    Send a prompt to a session");
            eprintln!("  roco session list                List all sessions");
            eprintln!("  roco session show <id>           Show session transcript");
            eprintln!("  roco session delete <id>         Delete a session");
            std::process::exit(1);
        }
    }
}

/// Create a new session and print its ID.
fn cmd_session_create(_args: &[&str]) {
    let session_id = format!("session_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let session_path = get_sessions_dir().join(format!("{}.json", session_id));

    // Create initial empty state
    let state = ConversationState::new(session_id.clone(), "careful");

    match serde_json::to_string_pretty(&state) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&session_path, json) {
                eprintln!("Error: Failed to create session: {e}");
                std::process::exit(1);
            }
            println!("Created session: {}", session_id);
            println!("Path: {}", session_path.display());
            println!();
            println!("Use it with:");
            println!("  roco session {} -p \"your prompt here\"", session_id);
        }
        Err(e) => {
            eprintln!("Error: Failed to serialize session state: {e}");
            std::process::exit(1);
        }
    }
}

/// List all sessions.
fn cmd_session_list() {
    let sessions_dir = get_sessions_dir();
    if !sessions_dir.exists() {
        println!("No sessions found.");
        return;
    }

    let mut sessions: Vec<_> = std::fs::read_dir(&sessions_dir)
        .expect("Failed to read sessions directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "json" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    sessions.sort_by_key(|p| p.file_stem().unwrap().to_string_lossy().to_string());

    if sessions.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!("Sessions ({} total):\n", sessions.len());
    for path in &sessions {
        let state = ConversationState::load(path)
            .unwrap_or_else(|_| ConversationState::new("error".to_string(), "careful"));
        let msg_count = state.messages.len();
        let updated = state.updated_at.clone();
        println!(
            "  {:<35} {} messages  (updated: {})",
            path.file_stem().unwrap().to_string_lossy(),
            msg_count,
            updated
        );
    }
}

/// Show session transcript.
fn cmd_session_show(args: &[&str]) {
    let session_id = match args.first() {
        Some(id) => id,
        None => {
            eprintln!("Usage: roco session show <session-id>");
            std::process::exit(1);
        }
    };

    let session_path = get_sessions_dir().join(format!("{}.json", session_id));
    if !session_path.exists() {
        eprintln!("Error: Session '{}' not found", session_id);
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&session_path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to read session: {e}");
        std::process::exit(1);
    });

    let state: ConversationState = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("Error: Failed to parse session: {e}");
        std::process::exit(1);
    });

    println!("Session: {}\n", session_id);
    println!("Messages ({} total):\n", state.messages.len());

    for (i, msg) in state.messages.iter().enumerate() {
        let role = match msg.role.as_str() {
            "user" => "You",
            "assistant" => "RoCo",
            "system" => "System",
            _ => "Unknown",
        };
        println!("--- {} ({}) ---", role, i + 1);
        // Truncate long messages
        let text = if msg.content.len() > 200 {
            format!("{}...", &msg.content[..200])
        } else {
            msg.content.clone()
        };
        println!("{}", text);
        println!();
    }
}

/// Delete a session.
fn cmd_session_delete(args: &[&str]) {
    let session_id = match args.first() {
        Some(id) => id,
        None => {
            eprintln!("Usage: roco session delete <session-id>");
            std::process::exit(1);
        }
    };

    let session_path = get_sessions_dir().join(format!("{}.json", session_id));
    if !session_path.exists() {
        eprintln!("Error: Session '{}' not found", session_id);
        std::process::exit(1);
    }

    std::fs::remove_file(&session_path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to delete session: {e}");
        std::process::exit(1);
    });

    println!("Deleted session: {}", session_id);
}

/// Send a prompt to an existing session.
fn cmd_session_chat(session_id: &str, args: &[&str]) {
    let prompt = match parse_opt("-p", args).or_else(|| parse_opt("--prompt", args)) {
        Some(p) if !p.is_empty() => p.to_string(),
        Some(_) => {
            eprintln!("Error: -p requires a non-empty prompt");
            std::process::exit(1);
        }
        None => {
            eprintln!("Usage: roco session <id> -p \"prompt\"");
            std::process::exit(1);
        }
    };

    let session_path = get_sessions_dir().join(format!("{}.json", session_id));
    if !session_path.exists() {
        eprintln!(
            "Error: Session '{}' not found. Create one with: roco session create",
            session_id
        );
        std::process::exit(1);
    }

    // Load existing state
    let mut state = ConversationState::load(&session_path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to read session: {e}");
        std::process::exit(1);
    });

    // Add user message
    state.add_message("user", &prompt);

    // Save updated state
    state.save(&session_path).unwrap_or_else(|e| {
        eprintln!("Error: Failed to save session: {e}");
        std::process::exit(1);
    });

    println!("Session: {}", session_id);
    println!("Prompt: {}", prompt);
    println!();

    // Run the interaction
    let backend = daemon::ensure_sync_backend();
    let mode = InteractMode::Resume {
        session_id: session_id.to_string(),
        instant: false,
    };

    match interact_cli::run(mode, &*backend) {
        Ok(_) => {
            println!();
            println!("Session saved.");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Get the sessions directory path.
fn get_sessions_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(SESSIONS_DIR)
}
