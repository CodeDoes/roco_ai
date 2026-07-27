//! Interactive subcommand: `roco interact`.

use crate::interact_cli::{self, InteractMode, PacingChoice};
use crate::{daemon, parse_opt};

pub fn cmd_interact(extra: &[&str]) {
    // `--list-sessions` must not start the daemon chain — listing files has
    // nothing to do with the model, and waiting ~25s for a model load to read
    // a directory is a bad trade.
    if extra.iter().any(|&a| a == "--list-sessions" || a == "-l") {
        interact_cli::list_sessions();
        return;
    }

    let prompt_arg = parse_opt("--prompt", extra);
    let resume = parse_opt("--resume", extra);
    let interactive = extra.iter().any(|&a| a == "--interactive" || a == "-i");
    let pace_str = parse_opt("--pace", extra).unwrap_or("careful");
    let pacing = PacingChoice::from_label(pace_str);

    // The first positional argument (anything not starting with `-`) is an
    // opening message. Previously `extra.first()` was used unconditionally,
    // so `roco interact --pace rolling` treated `--pace` itself as the prompt.
    let first_positional = first_positional(extra);

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
    } else {
        InteractMode::Interactive {
            pacing,
            prompt: if interactive {
                None
            } else {
                first_positional.map(str::to_string)
            },
        }
    };

    let backend = daemon::ensure_sync_backend();

    if let Err(e) = interact_cli::run(mode, &*backend) {
        eprintln!("Session error: {e}");
        std::process::exit(1);
    }
}

/// First argument that is neither a flag nor the value of a known flag.
fn first_positional<'a>(args: &[&'a str]) -> Option<&'a str> {
    /// Flags that consume the following argument.
    const VALUE_FLAGS: &[&str] = &["--prompt", "--resume", "--pace", "--model", "--session"];

    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            // `--pace=rolling` carries its own value; `--pace rolling` does not.
            if VALUE_FLAGS.contains(arg) {
                skip_next = true;
            }
            continue;
        }
        return Some(*arg);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::first_positional;

    #[test]
    fn flag_values_are_not_mistaken_for_prompts() {
        assert_eq!(first_positional(&["--pace", "rolling"]), None);
        assert_eq!(first_positional(&["--interactive"]), None);
        assert_eq!(first_positional(&[]), None);
    }

    #[test]
    fn a_real_positional_is_found() {
        assert_eq!(first_positional(&["hello there"]), Some("hello there"));
        assert_eq!(
            first_positional(&["--pace", "rolling", "hello"]),
            Some("hello")
        );
        assert_eq!(first_positional(&["--pace=rolling", "hi"]), Some("hi"));
    }
}
