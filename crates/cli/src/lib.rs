//! `roco-cli` library — subcommands and shared helpers.
//!
//! The `roco` binary is a thin dispatcher over this crate so editing one
//! subcommand does not recompile a single 1500-line translation unit.

pub mod cmd;
pub mod daemon;
pub mod rich_output;

#[path = "interact.rs"]
pub mod interact_cli;

#[cfg(feature = "net")]
#[path = "lsp.rs"]
pub mod lsp_handler;

#[cfg(feature = "net")]
pub mod story_routes;

use std::process::Command;

/// Parse `--flag value` from a free-form argv slice.
pub fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    args.windows(2)
        .find_map(|w| if w[0] == name { Some(w[1]) } else { None })
}

/// Run a cargo subcommand and exit with its status code.
pub fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let code = run_cargo_get_code(cmd, args, extra);
    std::process::exit(code);
}

/// Run a cargo subcommand and return its exit code.
pub fn run_cargo_get_code(cmd: &str, args: &[&str], extra: &[&str]) -> i32 {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}

/// Print top-level CLI help.
pub fn help(_sub: Option<&str>) {
    eprintln!("RoCo AI — Collaborative Writing Assistant\n");
    eprintln!("Usage:");
    eprintln!("  roco                                 Start interactive chat (natural language)");
    eprintln!("  roco <prompt>                        Chat with a starting prompt");
    eprintln!("  roco <subcommand> [args]             Run a specific command\n");
    eprintln!("Subcommands:");
    eprintln!("  interact [--interactive] [--prompt PROMPT] [--resume SESSION] [--pace MODE]");
    eprintln!("                                  Interactive CLI with pacing (default)");
    eprintln!("  interact --list-sessions           List saved sessions");
    eprintln!("  story <prompt> [--strategy S] [--max-tokens T] Structured short story");
    eprintln!("  story-mode [--story STORY] [command]  Interactive story writing assistant");
    eprintln!("  sm         Alias for story-mode");
    eprintln!("  game [scenario]                 Adventure game mode (interactive fiction)");
    eprintln!("  html [--port PORT]                Live HTML canvas — agent responds in HTML, served via local web server");
    eprintln!("  code <question> [--lang LANG]   AI coding assistant");
    eprintln!("  gui                               Desktop GUI (--features desktop)");
    eprintln!(
        "  server [...]                      HTTP surface (--features net); GPU via roco-inferd"
    );
    eprintln!("  gateway [...]                     API gateway (--features net)");
    eprintln!("  stop                              Stop background inference + gateway");
    eprintln!("  export <story-dir> [--format md|html|txt] [--output PATH]");
    eprintln!("  eval [--output PATH]              Run evals, save snapshot");
    eprintln!("  bless [--snapshot PATH]            Bless snapshot as new oracle");
    eprintln!("  rwkv                              Smoke-test the RWKV backend");
    eprintln!("  grammar                           Grammar-constrained decode");
    eprintln!("  gpu-check [--json|-j]              Show Vulkan + model info\n");
    eprintln!("Config: RWKV_MODEL / .roco/config.toml / $ROCO_CONFIG / ~/.config/roco/config.toml");
    std::process::exit(0);
}
