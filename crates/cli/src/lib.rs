//! `roco-cli` library — subcommands and shared helpers.
//!
//! The `roco` binary is a thin dispatcher over this crate so editing one
//! subcommand does not recompile a single 1500-line translation unit.

pub mod cmd;
pub mod conversation;
pub mod daemon;
pub mod identity;
pub mod rich_output;
pub mod streaming;
pub mod test_harness;

#[path = "interact.rs"]
pub mod interact_cli;

#[cfg(feature = "net")]
#[path = "lsp.rs"]
pub mod lsp_handler;

#[cfg(feature = "net")]
pub mod story_routes;

use std::process::Command;

/// Parse a named option from args, supporting both `--key=value` and
/// `--key value` (separate arg) formats. Returns `None` if not present.
pub fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    // Try `--key=value` first (single arg with = separator)
    let eq_prefix = format!("{}=", name);
    if let Some(arg) = args.iter().find(|a| a.starts_with(&eq_prefix)) {
        return Some(&arg[name.len() + 1..]);
    }
    // Try `--key` with value in next arg (separate args)
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

/// Check if `--help` or `-h` appears in the argument list.
pub fn has_help_flag(args: &[&str]) -> bool {
    args.iter().any(|&a| a == "--help" || a == "-h")
}

/// Print help for a specific subcommand, or the top-level help.
pub fn help(sub: Option<&str>) {
    match sub {
        Some("inferd") | Some("server") => help_inferd(),
        Some("gateway") => help_gateway(),
        Some("stop") => help_stop(),
        Some("story") | Some("story-mode") | Some("sm") => help_story(),
        Some("interact") => help_interact(),
        Some("eval") | Some("bless") => help_eval(),
        Some("gui") => help_gui(),
        Some("pet") => help_pet(),
        Some("export") => help_export(),
        Some("game") => help_game(),
        Some("html") => help_html(),
        Some("code") | Some("coder") => help_code(),
        Some("rwkv") => help_rwkv(),
        Some("grammar") => help_grammar(),
        Some("gpu-check") => help_gpu_check(),
        Some("reload") => help_reload(),
        Some("jobs") => help_jobs(),
        Some("whoami") => help_whoami(),
        Some("version") | Some("--version") => help_version(),
        _ => help_root(),
    }
}

fn help_root() {
    eprintln!("RoCo AI — Collaborative Writing Assistant\n");
    eprintln!("Usage:");
    eprintln!("  roco                                 Start interactive chat (natural language)");
    eprintln!("  roco <prompt>                        Chat with a starting prompt");
    eprintln!("  roco <subcommand> [args]             Run a specific command");
    eprintln!("  roco <subcommand> --help             Show help for a specific command\n");
    eprintln!("Commands:");
    eprintln!("  interact     Interactive CLI with pacing (default mode)");
    eprintln!("  story        Structured short story from premise");
    eprintln!("  story-mode   Interactive story writing assistant");
    eprintln!("  sm           Alias for story-mode");
    eprintln!("  game         Adventure game mode (interactive fiction)");
    eprintln!("  html         Live HTML canvas");
    eprintln!("  code         AI coding assistant");
    eprintln!("  eval         Run evaluations");
    eprintln!("  bless        Bless snapshot as new oracle");
    eprintln!("  export       Export a finished story");
    eprintln!("  inferd       Inference daemon control (start/stop/restart/status)");
    eprintln!("  gateway      API gateway control (start/stop/restart/status)");
    eprintln!("  stop         Stop background daemons");
    eprintln!("  jobs         Show daemon status and active jobs");
    eprintln!("  gui          Desktop GUI (--features desktop)");
    eprintln!("  pet          Desktop pet (--features desktop)");
    eprintln!("  rwkv         Smoke-test the RWKV backend");
    eprintln!("  grammar      Grammar-constrained decode test");
    eprintln!("  gpu-check    Show Vulkan device + model info");
    eprintln!("  whoami       Show what RoCo is and what it knows about you");
    eprintln!("  version      Show version\n");
    eprintln!("Config: RWKV_MODEL / .roco/config.toml / $ROCO_CONFIG / ~/.config/roco/config.toml");
    std::process::exit(0);
}

fn help_inferd() {
    eprintln!("roco inferd — Inference daemon control\n");
    eprintln!("Usage:");
    eprintln!("  roco inferd start      Start inference daemon (roco-inferd)");
    eprintln!("  roco inferd stop       Stop inference daemon");
    eprintln!("  roco inferd restart    Restart inference daemon");
    eprintln!("  roco inferd status     Show inference daemon status");
    eprintln!("  roco inferd reload     Reload (stop + start) inference daemon\n");
    eprintln!("The inference daemon (roco-inferd) loads the RWKV model on GPU");
    eprintln!("and serves the completion API. It is auto-started by gateway");
    eprintln!("and other commands that need it.\n");
    eprintln!("Requires: --features net");
    std::process::exit(0);
}

fn help_gateway() {
    eprintln!("roco gateway — API gateway control\n");
    eprintln!("Usage:");
    eprintln!("  roco gateway start      Start gateway");
    eprintln!("  roco gateway stop       Stop gateway");
    eprintln!("  roco gateway restart    Restart gateway");
    eprintln!("  roco gateway status     Show gateway status");
    eprintln!("  roco gateway reload     Reload (stop + start) gateway\n");
    eprintln!("The gateway is a HTTP reverse proxy that routes requests to");
    eprintln!("the inference daemon and story engine.\n");
    eprintln!("Requires: --features net");
    std::process::exit(0);
}

fn help_stop() {
    eprintln!("roco stop — Stop background daemons\n");
    eprintln!("Usage:");
    eprintln!("  roco stop              Stop all daemons (inferd + gateway)");
    eprintln!("  roco stop inferd       Stop inference daemon only");
    eprintln!("  roco stop gateway      Stop gateway only\n");
    eprintln!("Note: stop only stops running daemons. It never starts anything.");
    std::process::exit(0);
}

fn help_story() {
    eprintln!("roco story — Structured short story pipeline\n");
    eprintln!("Usage:");
    eprintln!("  roco story <premise>           Generate a story from a premise");
    eprintln!("  roco story <premise> --strategy <s>  Strategy: meticulous|collaborative|fast");
    eprintln!("  roco story <premise> --max-tokens <n>  Max tokens per chapter\n");
    std::process::exit(0);
}

fn help_interact() {
    eprintln!("roco interact — Interactive chat REPL\n");
    eprintln!("Usage:");
    eprintln!("  roco interact                       Start interactive chat");
    eprintln!("  roco interact <text>                 Chat with an opening message");
    eprintln!("  roco interact --prompt <text>        One-shot: generate, save, exit");
    eprintln!("  roco interact --resume <session>     Resume a saved session");
    eprintln!("  roco interact --list-sessions        List saved sessions");
    eprintln!("  roco interact --pace <mode>          Pacing: auto|careful|rolling|planning\n");
    eprintln!("Responses stream token-by-token as they are generated.\n");
    eprintln!("In-chat commands:");
    eprintln!("  :help            Show all commands");
    eprintln!("  :whoami          What RoCo knows about you");
    eprintln!("  :whois           What RoCo is");
    eprintln!("  :name <you>      Tell RoCo your name");
    eprintln!("  :remember <fact> Remember something about you");
    eprintln!("  :forget          Forget everything about you");
    eprintln!("  :quit            Save and exit\n");
    std::process::exit(0);
}

fn help_whoami() {
    eprintln!("roco whoami — Identity: who RoCo is, and who you are\n");
    eprintln!("Usage:");
    eprintln!("  roco whoami                    Show both identities");
    eprintln!("  roco whoami --json             Print the stored profile as JSON");
    eprintln!("  roco whoami --set-name <name>  Record your name");
    eprintln!("  roco whoami --forget           Erase the stored profile\n");
    eprintln!("Your profile lives in ./.roco/profile.json and never leaves this machine.");
    eprintln!("It is only written when you say so (\"my name is …\", \"remember that …\").\n");
    std::process::exit(0);
}

fn help_eval() {
    eprintln!("roco eval / bless — Evaluation suite\n");
    eprintln!("Usage:");
    eprintln!("  roco eval [--output PATH]       Run evals, save snapshot");
    eprintln!("  roco bless [--snapshot PATH]    Bless a snapshot as new oracle\n");
    std::process::exit(0);
}

fn help_gui() {
    eprintln!("roco gui — Desktop GUI\n");
    eprintln!("Usage:");
    eprintln!("  roco gui                Launch the desktop GUI\n");
    eprintln!("Requires: --features desktop");
    std::process::exit(0);
}

fn help_pet() {
    eprintln!("roco pet — Desktop pet\n");
    eprintln!("Usage:");
    eprintln!("  roco pet                Launch desktop pet");
    eprintln!("  roco pet stop           Stop running pet");
    eprintln!("  roco pet --hide         Start hidden (tray only)");
    eprintln!("  roco pet --install      Install .desktop file + auto-start");
    eprintln!("  roco pet --uninstall    Remove .desktop file\n");
    eprintln!("Requires: --features desktop");
    std::process::exit(0);
}

fn help_export() {
    eprintln!("roco export — Export a finished story\n");
    eprintln!("Usage:");
    eprintln!("  roco export <story-dir> [--format md|html|txt] [--output PATH]\n");
    std::process::exit(0);
}

fn help_game() {
    eprintln!("roco game — Adventure game mode\n");
    eprintln!("Usage:");
    eprintln!("  roco game                   Start interactive fiction game master");
    eprintln!("  roco game <scenario>        Start with a specific scenario\n");
    std::process::exit(0);
}

fn help_html() {
    eprintln!("roco html — Live HTML canvas\n");
    eprintln!("Usage:");
    eprintln!("  roco html                          Start interactive HTML session");
    eprintln!("  roco html <prompt>                 Start with an initial prompt");
    eprintln!("  roco html --port <port>            Custom port (default: 9090)\n");
    std::process::exit(0);
}

fn help_code() {
    eprintln!("roco code — AI coding assistant\n");
    eprintln!("Usage:");
    eprintln!("  roco code <question>                Ask a coding question");
    eprintln!("  roco code <question> --lang <lang>  Specify language (rust, python, ts, etc.)\n");
    std::process::exit(0);
}

fn help_rwkv() {
    eprintln!("roco rwkv — Smoke-test the RWKV backend\n");
    eprintln!("Usage:");
    eprintln!("  roco rwkv [extra args...]     Run the RWKV backend smoke test\n");
    std::process::exit(0);
}

fn help_grammar() {
    eprintln!("roco grammar — Grammar-constrained decode test\n");
    eprintln!("Usage:");
    eprintln!("  roco grammar [extra args...]  Run the grammar-constrained decode test\n");
    std::process::exit(0);
}

fn help_gpu_check() {
    eprintln!("roco gpu-check — Show Vulkan device + model info\n");
    eprintln!("Usage:");
    eprintln!("  roco gpu-check                Show Vulkan devices, model, vocab status");
    eprintln!("  roco gpu-check --json         Output as JSON\n");
    std::process::exit(0);
}

fn help_reload() {
    eprintln!("roco reload — Reload both inferd and gateway daemons\n");
    eprintln!("Usage:");
    eprintln!("  roco reload              Stop and restart both daemons\n");
    eprintln!("Note: This rebuilds nothing. Use `./dev.sh --watch` for auto-rebuild on change.\n");
    std::process::exit(0);
}

fn help_version() {
    eprintln!("roco version — Show version info\n");
    eprintln!("Usage:");
    eprintln!("  roco version       Show version");
    eprintln!("  roco --version     Show version\n");
    std::process::exit(0);
}

fn help_jobs() {
    eprintln!("roco jobs — Show daemon status and active jobs\n");
    eprintln!("Usage:");
    eprintln!("  roco jobs                 Show inference daemon health and active jobs\n");
    std::process::exit(0);
}
