//! `roco` — unified CLI for RoCo AI.
//!
//! ════════════════════════════════════════════════════════════════════════════
//! FILE STATUS: EDITABLE (CLI binary). See EDIT_GUIDE.md for rules.
//! SIZE: ~130 lines / 4 KB. Entry-point only — subcommand logic lives in
//! `crates/cli/src/cmd/` (modularised 2026-07-20).
//! KEY SECTIONS (in order):
//!   1. Module declarations + helpers (lines 1-50)
//!   2. main() — subcommand dispatch → cmd::* (lines 51-95)
//!   3. helper functions: help, parse_opt, run_cargo (lines 96-130)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Usage:
//!   roco eval [--output PATH]              run the RWKV eval suite
//!   roco bless [--snapshot PATH]           bless current outputs as new oracle
//!   roco rwkv                              smoke-test the RWKV backend
//!   roco grammar                           grammar-constrained decode
//!   roco gpu-check                         show Vulkan + model info

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[path = "../story_routes.rs"]
mod story_routes;

#[path = "../lsp.rs"]
mod lsp_handler;

#[path = "../interact.rs"]
mod interact_cli;

#[path = "../rich_output.rs"]
mod rich_output;

#[path = "../daemon.rs"]
mod daemon;

#[path = "../cmd/mod.rs"]
mod cmd;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => cmd::eval::cmd_eval(&extra),
        "bless" => cmd::eval::cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &["-p", "roco-cli", "--example", "rwkv_test", "--release"],
            &extra,
        ),
        "grammar" => run_cargo(
            "run",
            &["-p", "roco-cli", "--example", "grammar_smoke", "--release"],
            &extra,
        ),
        "gpu-check" => cmd::gpu::cmd_gpu_check(&extra),
        "server" => cmd::server::cmd_server(&extra),
        "gateway" => cmd::server::cmd_gateway(&extra),
        "tui" => cmd::desktop::cmd_tui(&extra),
        "gui" => cmd::desktop::cmd_gui(&extra),
        "stop" => daemon::stop_all(),
        "story" => cmd::story::cmd_story(&extra),
        "interact" => cmd::interact::cmd_interact(&extra),
        _ => help(sub),
    }
}

fn help(sub: &str) {
    eprintln!("Usage: roco <subcommand> [args]\n");
    eprintln!("  eval [--output PATH]              Run evals, save snapshot");
    eprintln!("  bless [--snapshot PATH]            Bless snapshot as new oracle");
    eprintln!("  rwkv                              Smoke-test the RWKV backend");
    eprintln!("  grammar                           Grammar-constrained decode");
    eprintln!("  gpu-check [--json|-j]              Show Vulkan + model info");
    eprintln!("  server [--host ADDR] [--port PORT] [--story] [--detach|-d]  HTTP server");
    eprintln!("  server --stdio-lsp [--inference-url URL]        LSP client for editor plugins");
    eprintln!("  gateway [--host ADDR] [--port PORT] [--target URL] [--rate-limit L]  API gateway");
    eprintln!("  tui                               Terminal chat UI");
    eprintln!("  gui                               Desktop GUI (auto-starts gateway + inference)");
    eprintln!("  stop                              Stop background inference + gateway");
    eprintln!("  story <prompt> [--strategy S] [--max-tokens T]  Generate a structure short story");
    eprintln!(
        "  interact [--interactive] [--prompt PROMPT] [--resume SESSION] [--pace MODE]  Interactive CLI"
    );
    eprintln!("  interact --list-sessions         List saved sessions");
    std::process::exit(if sub == "help" { 0 } else { 1 });
}

/// Extract an option value from `--name value` style args.
pub(crate) fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    args.windows(2)
        .find_map(|w| if w[0] == name { Some(w[1]) } else { None })
}

/// Run a cargo subcommand (e.g. `cargo run --example ...`) and pass through
/// any extra args. Convenience wrapper for quick smoke-test delegation.
fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let code = run_cargo_get_code(cmd, args, extra);
    std::process::exit(code);
}

/// Like `run_cargo` but returns the exit code instead of exiting.
pub(crate) fn run_cargo_get_code(cmd: &str, args: &[&str], extra: &[&str]) -> i32 {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}
