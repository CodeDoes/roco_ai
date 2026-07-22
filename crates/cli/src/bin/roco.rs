//! `roco` — unified CLI for RoCo AI.
//!
//! ════════════════════════════════════════════════════════════════════════════
//! FILE STATUS: EDITABLE (CLI binary). See EDIT_GUIDE.md for rules.
//! SIZE: ~572 lines. Thin dispatch shell — subcommand logic lives in `cmd/`.
//! KEY SECTIONS (in order):
//!   1. Module declarations + helpers (spawn_detached, default_detach_path)
//!   2. main() — subcommand dispatch to cmd_* modules
//!   3. Shared utilities (run_cargo, parse_opt, help)
//!
//! Subcommand implementations live in `crates/cli/src/cmd/*.rs`:
//!   cmd/eval.rs    — eval, bless
//!   cmd/gpu.rs     — gpu-check
//!   cmd/server.rs  — server, gateway
//!   cmd/desktop.rs — gui
//!   cmd/interact.rs — interact
//!   cmd/story.rs   — story pipeline
//!   cmd/export.rs  — export
//! ════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "net")]
use std::fs;
#[cfg(feature = "net")]
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(feature = "net")]
#[path = "../story_routes.rs"]
mod story_routes;

#[cfg(feature = "net")]
#[path = "../lsp.rs"]
mod lsp_handler;

#[path = "../interact.rs"]
mod interact_cli;

#[path = "../rich_output.rs"]
mod rich_output;

#[path = "../daemon.rs"]
mod daemon;

#[path = "../cmd/export.rs"]
mod cmd_export;
#[path = "../cmd/gpu.rs"]
mod cmd_gpu;
#[cfg(feature = "desktop")]
#[path = "../cmd/desktop.rs"]
mod cmd_desktop;
#[cfg(feature = "net")]
#[path = "../cmd/server.rs"]
mod cmd_server_mod;
#[path = "../cmd/eval.rs"]
mod cmd_eval_mod;
#[path = "../cmd/interact.rs"]
mod cmd_interact_mod;
#[path = "../cmd/story.rs"]
mod cmd_story_mod;

/// Spawn a detached child process for `roco server` or `roco gateway`.
/// The parent redirects stdio to a log file, writes a PID file, and exits.
#[cfg(feature = "net")]
pub(crate) fn spawn_detached(subcmd: &str, extra: &[&str], log_path: &Path, pid_path: &Path) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    // Build args for the child: subcommand + modified extras
    let mut child_args: Vec<String> = Vec::new();
    child_args.push(subcmd.to_string());

    let mut i = 0;
    while i < extra.len() {
        let a = extra[i];
        if a == "--detach" || a == "-d" {
            child_args.push(format!("--_child-{subcmd}"));
        } else if a == "--pid-file" || a == "--log-file" {
            child_args.push(a.to_string());
            if i + 1 < extra.len() {
                child_args.push(extra[i + 1].to_string());
                i += 1;
            }
        } else {
            child_args.push(a.to_string());
        }
        i += 1;
    }

    let log_file = fs::File::create(log_path)
        .unwrap_or_else(|e| panic!("failed to create log file {}: {e}", log_path.display()));
    let log_clone = log_file
        .try_clone()
        .expect("failed to clone log file handle");

    let child = Command::new(&exe)
        .args(&child_args)
        .stdin(fs::File::open("/dev/null").expect("no /dev/null"))
        .stdout(log_file)
        .stderr(log_clone)
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn child: {e}"));

    let pid = child.id();
    fs::write(pid_path, pid.to_string())
        .unwrap_or_else(|e| panic!("failed to write pid file {}: {e}", pid_path.display()));

    println!("roco {subcmd} started (PID {pid})");
    println!("  log:      {}", log_path.display());
    println!("  pidfile:  {}", pid_path.display());

    // Detach: child is a daemon that outlives the caller.
    // The parent (main) exits right after this, so child is adopted by init
    // which reaps it. `forget` prevents clippy::zombie_processes and also
    // keeps the process alive on Windows (where Child::drop would kill it).
    std::mem::forget(child);
}

/// Compute a default path under `/tmp/roco/` for PID or log files.
#[cfg(feature = "net")]
pub(crate) fn default_detach_path(subcmd: &str, port: u16, ext: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("roco");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{subcmd}_{port}.{ext}"))
}

fn main() {
    // ── Load config before anything else ───────────────────────────────────
    // This sets RWKV_MODEL / RWKV_VOCAB from config file (if any) so that
    // every downstream path (server, interact, story, gui) picks them up
    // without requiring the user to set env vars manually.
    let cfg = roco_app::RoCoConfig::load();
    cfg.apply_to_environment();

    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("interact");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => cmd_eval_mod::cmd_eval(&extra),
        "bless" => cmd_eval_mod::cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &["-p", "roco-inference", "--example", "rwkv_test", "--release"],
            &extra,
        ),
        "grammar" => run_cargo(
            "run",
            &["-p", "roco-inference", "--example", "grammar_smoke", "--release"],
            &extra,
        ),
        "gpu-check" => cmd_gpu::cmd_gpu_check(&extra),
        "server" => {
            #[cfg(feature = "net")]
            cmd_server_mod::cmd_server(&extra);
            #[cfg(not(feature = "net"))]
            {
                eprintln!(
                    "error: `roco server` needs `--features net`.\n                     For local GPU inference use: cargo run -p roco-inferd\n                     Or rebuild: cargo build -p roco-cli --features net"
                );
                std::process::exit(2);
            }
        }
        "gateway" => {
            #[cfg(feature = "net")]
            cmd_server_mod::cmd_gateway(&extra);
            #[cfg(not(feature = "net"))]
            {
                eprintln!(
                    "error: `roco gateway` needs `--features net`.\n                     rebuild: cargo build -p roco-cli --features net"
                );
                std::process::exit(2);
            }
        }
        "gui" => {
            #[cfg(feature = "desktop")]
            cmd_desktop::cmd_gui(&extra);
            #[cfg(not(feature = "desktop"))]
            {
                eprintln!(
                    "error: `roco gui` requires the desktop feature.\n                     rebuild with: cargo build -p roco-cli --features desktop\n                     or:            make build-desktop"
                );
                std::process::exit(2);
            }
        }
        "stop" => {
            crate::daemon::stop_all();
        }
        "story" => cmd_story_mod::cmd_story(&extra),
        "interact" => cmd_interact_mod::cmd_interact(&extra),
        "export" => {
            // `roco export <story-dir> [--format md|html|txt] [--output PATH]`
            cmd_export::run(
                extra.first().copied().unwrap_or("."),
                parse_opt("--format", &extra),
                parse_opt("--output", &extra),
            );
        }
        // Unknown subcommand → treat as interact prompt (natural language)
        // e.g. `roco write a story about a cat` starts interact with that prompt.
        "help" | "--help" | "-h" => help(None),
        _ => {
            // First arg becomes the interact prompt; rest are extra flags.
            let mut args_with_prompt = vec![sub];
            args_with_prompt.extend(extra.iter().copied());
            cmd_interact_mod::cmd_interact(&args_with_prompt);
        }
    }
}

pub(crate) fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let code = run_cargo_get_code(cmd, args, extra);
    std::process::exit(code);
}

pub(crate) fn run_cargo_get_code(cmd: &str, args: &[&str], extra: &[&str]) -> i32 {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}



pub(crate) fn help(sub: Option<&str>) {
    eprintln!("RoCo AI — Collaborative Writing Assistant\n");
    eprintln!("Usage:");
    eprintln!("  roco                                 Start interactive chat (natural language)");
    eprintln!("  roco <prompt>                        Chat with a starting prompt");
    eprintln!("  roco <subcommand> [args]             Run a specific command\n");
    eprintln!("Subcommands:");
    eprintln!("  interact [--interactive] [--prompt PROMPT] [--resume SESSION] [--pace MODE]");
    eprintln!(
        "                                  Interactive CLI with pacing control, session resume (default)"
    );
    eprintln!("  interact --list-sessions           List saved sessions");
    eprintln!("  story <prompt> [--strategy S] [--max-tokens T] Generate a structured short story (formal pipeline)");
    eprintln!("  gui                               Start the desktop GUI application");
    eprintln!(
        "  server [--host ADDR] [--port PORT] [--story] [--detach|-d] Run the local HTTP server"
    );
    eprintln!(
        "  gateway [--host ADDR] [--port PORT] [--target URL] [--rate-limit L] Run the API gateway"
    );
    eprintln!("  stop                              Stop background inference + gateway");
    eprintln!("  export <story-dir> [--format md|html|txt] [--output PATH]");
    eprintln!("  eval [--output PATH]              Run evals, save snapshot");
    eprintln!("  bless [--snapshot PATH]            Bless snapshot as new oracle");
    eprintln!("  rwkv                              Smoke-test the RWKV backend");
    eprintln!("  grammar                           Grammar-constrained decode");
    eprintln!("  gpu-check [--json|-j]              Show Vulkan + model info\n");
    eprintln!("Config:");
    eprintln!("  Model path: set RWKV_MODEL env var, or create a config file:");
    eprintln!("    .roco/config.toml  |  $ROCO_CONFIG  |  ~/.config/roco/config.toml");
    eprintln!("  Example config.toml:");
    eprintln!("    [model]");
    eprintln!("    path = \"/path/to/model.st\"");
    eprintln!("    vocab = \"/path/to/vocab.json\"\n");
    eprintln!("  The model auto-detects in models/ directory if neither is set.");
    std::process::exit(match sub {
        Some("help") | Some("--help") | Some("-h") => 0,
        _ => 0,
    });
}

pub(crate) fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    args.windows(2)
        .find_map(|w| if w[0] == name { Some(w[1]) } else { None })
}

