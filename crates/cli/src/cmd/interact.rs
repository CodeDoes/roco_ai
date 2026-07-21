//! Interactive subcommand: `roco interact`.

use crate::interact_cli::{self, InteractMode, PacingChoice};
use crate::{daemon, parse_opt};

pub fn cmd_interact(extra: &[&str]) {
    // Check for --list-sessions
    if extra.iter().any(|&a| a == "--list-sessions" || a == "-l") {
        interact_cli::list_sessions();
        return;
    }

    // Determine mode
    let prompt_arg = parse_opt("--prompt", extra);
    let resume = parse_opt("--resume", extra);
    let interactive = extra.iter().any(|&a| a == "--interactive" || a == "-i");
    let pace_str = parse_opt("--pace", extra).unwrap_or("careful");
    let first_arg = extra.first().map(|s| *s).unwrap_or("");

    let pacing = match pace_str {
        "planning" | "plan" => PacingChoice::Planning,
        "careful" | "full" => PacingChoice::Careful,
        "rolling" | "batch" => PacingChoice::Rolling,
        "auto" | "auto-accept" => PacingChoice::AutoAccept,
        _ => PacingChoice::Careful,
    };

    let mode = if let Some(p) = prompt_arg {
        if p.is_empty() {
            eprintln!("Error: --prompt requires a non-empty prompt");
            std::process::exit(1);
        }
        InteractMode::Prompt {
            prompt: p.to_string(),
        }
    } else if let Some(session_id) = resume {
        InteractMode::Resume {
            session_id: session_id.to_string(),
        }
    } else if interactive || extra.is_empty() {
        let initial = if first_arg.is_empty() {
            None
        } else {
            Some(first_arg.to_string())
        };
        InteractMode::Interactive {
            pacing,
            prompt: initial,
        }
    } else {
        InteractMode::Interactive {
            pacing,
            prompt: Some(first_arg.to_string()),
        }
    };

    let backend = daemon::ensure_sync_backend();

    if let Err(e) = interact_cli::run(mode, &*backend) {
        eprintln!("Session error: {e}");
        std::process::exit(1);
    }
}
