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

/// Parse a `--seed` argument from args, returning `Some(seed)` if present.
/// Supports both `--seed=42` and `--seed 42` formats.
pub fn parse_seed(args: &[&str]) -> Option<u64> {
    parse_opt("--seed", args).and_then(|s| s.parse::<u64>().ok())
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
pub fn help(sub: Option<&str>, hidden: bool) {
    match sub {
        Some("inferd") | Some("server") => help_inferd(),
        Some("gateway") => help_gateway(),
        Some("session") => help_session(),
        Some("stop") => help_stop(),
        Some("story") | Some("story-mode") | Some("sm") => help_story(),
        Some("workspace") => help_workspace(),
        Some("vector-search") | Some("vector_search") => help_vector_search(),
        Some("interact") => help_interact(),
        Some("eval") | Some("bless") => help_eval(),
        Some("gui") => help_gui(),
        Some("pet") => help_pet(),
        Some("export") => help_export(),
        Some("game") => help_game(),
        Some("ttrpg") => help_ttrpg(),
        Some("map") => help_map(),
        Some("world-sim") => help_world_sim(),
        Some("html") => help_html(),
        Some("code") | Some("coder") => help_code(),
        Some("rwkv") => help_rwkv(),
        Some("grammar") => help_grammar(),
        Some("gpu-check") => help_gpu_check(),
        Some("reload") => help_reload(),
        Some("jobs") => help_jobs(),
        Some("stats") | Some("review") => help_stats(),
        Some("inspect") => help_inspect(),
        Some("eval-suite") => help_eval_suite(),
        Some("solution-bench") => help_solution_bench(),
        Some("completions") => help_completions(),
        Some("whoami") => help_whoami(),
        Some("quickstart") => help_quickstart(),
        Some("version") | Some("--version") => help_version(),
        _ => help_root(hidden),
    }
}

fn help_root(hidden: bool) {
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
    if hidden {
        eprintln!("  session      Session management (new, list, show, delete)");
        eprintln!("  workspace    Workspace management (new, list, show, delete)");
    }
    eprintln!("  vector-search Local offline vector embedding search");
    eprintln!("  game         Adventure game mode (interactive fiction)");
    eprintln!("  ttrpg        TTRPG campaign and world building system");
    eprintln!("  world-sim    World building and organic simulation engine");
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
    eprintln!("  stats        Story/workspace statistics and review");
    eprintln!("  inspect      Model & system inspection (caches, sessions, config)");
    eprintln!("  eval-suite   Deterministic evaluation suite");
    eprintln!("  solution-bench Solution evaluation bench for SSM architectural patterns\n");
    eprintln!("Identity:");
    eprintln!("  whoami       Show what RoCo is and what it knows about you");
    eprintln!("  version      Show version\n");
    eprintln!("Config: RWKV_MODEL / .roco/config.toml / $ROCO_DIR/config.toml\n");
    eprintln!("Try: roco quickstart");
    std::process::exit(0);
}

fn help_quickstart() {
    eprintln!("roco quickstart — Show first-run guide and setup instructions\n");
    eprintln!("Usage:");
    eprintln!("  roco quickstart");
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
    eprintln!("  roco story <premise>                      Generate a story from a premise");
    eprintln!("  roco story <premise> --resume             Resume from last completed phase");
    eprintln!("  roco story <premise> --phase <name>       Run only one phase (outline|wiki|chapters|validation|synopsis)");
    eprintln!("  roco story <premise> --fix chapter <n>    Regenerate a single chapter");
    eprintln!("  roco story <premise> --workspace <path>   Use existing workspace directory");
    eprintln!("  roco story <premise> --strategy <s>       Strategy: meticulous|collaborative|fast|state-tuned");
    eprintln!("  roco story <premise> --max-tokens <n>     Max tokens per phase (default 800, chapters min 1500)");
    eprintln!("  roco story <premise> --temperature <f>    Sampling temperature (default 0.7)");
    eprintln!("  roco story <premise> --seed <n>           Deterministic seed for sampling\n");
    eprintln!("Phases (in order): outline → wiki → chapters → validation → synopsis → publish\n");
    eprintln!("The pipeline resumes automatically if the workspace already has completed phases.");
    eprintln!("Examples:");
    eprintln!("  roco story \"A lighthouse keeper finds a message in the fog\"");
    eprintln!("  roco story --resume");
    eprintln!("  roco story --phase synopsis                           # re-run only synopsis");
    eprintln!("  roco story \"A detective in a cyberpunk city\" --fix chapter 3\n");
    std::process::exit(0);
}

fn help_session() {
    eprintln!("roco session — Session management\n");
    eprintln!("Usage:");
    eprintln!("  roco session new                         Create a new session");
    eprintln!("  roco session <id> -p \"prompt\"            Send a prompt to a session");
    eprintln!("  roco session list                        List all sessions");
    eprintln!("  roco session show <id>                   Show session transcript");
    eprintln!("  roco session delete <id>                 Delete a session\n");
    eprintln!("Recommended workflow:");
    eprintln!("  roco session new");
    eprintln!("  roco workspace new");
    eprintln!("  roco session <id> -p \"Use the workspace <workspace_id>\"");
    eprintln!("  roco session <id> -p \"Write a story about X\"\n");
    std::process::exit(0);
}

fn help_workspace() {
    eprintln!("roco workspace — Workspace management\n");
    eprintln!("Usage:");
    eprintln!("  roco workspace new                       Create a new workspace");
    eprintln!("  roco workspace list                      List all workspaces");
    eprintln!("  roco workspace show <id>                 Show workspace contents");
    eprintln!("  roco workspace delete <id>               Delete a workspace\n");
    eprintln!("Workspaces store story artifacts (outline, wiki, chapters, etc.).");
    eprintln!("Use with sessions for persistent collaborative writing.\n");
    std::process::exit(0);
}

fn help_vector_search() {
    eprintln!("roco vector-search — Local offline vector embedding search\n");
    eprintln!("Usage:");
    eprintln!("  roco vector-search init [--index PATH] [--dimensions NUM]    Initialize index");
    eprintln!("  roco vector-search add <text> [--id ID] [--index PATH] [--meta JSON] Add text");
    eprintln!("  roco vector-search query <query> [--limit LIMIT] [--index PATH] Query index");
    eprintln!(
        "  roco vector-search status [--index PATH]                     Show index details\n"
    );
    eprintln!("Features deterministic dense vector embeddings computed locally with zero third-party/API dependencies.\n");
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

fn help_ttrpg() {
    eprintln!("roco ttrpg — TTRPG campaign and world building system\n");
    eprintln!("Usage:");
    eprintln!("  roco ttrpg                  Start interactive TTRPG campaign REPL");
    eprintln!("In-game Commands:");
    eprintln!("  :sheet                     View character sheet & attributes");
    eprintln!("  :world                     Inspect locations, factions, lore");
    eprintln!("  :triggers                  List natural language triggers & checks");
    eprintln!("  :chat <npc_name>           Immersive character conversation chat");
    eprintln!(
        "  :add_trigger <text>        Register a natural language trigger checked every turn\n"
    );
    std::process::exit(0);
}

fn help_map() {
    eprintln!("roco map — Procedural WFC world map generator\n");
    eprintln!("Usage:");
    eprintln!("  roco map [--width W] [--height H] [--seed N] [--no-open]");
    eprintln!("  roco map --ttrpg                     Also export biome regions to TTRPG state\n");
    eprintln!("Flags:");
    eprintln!("  --width W / --height H               Map dimensions (default 40x20)");
    eprintln!("  --seed N                             Deterministic seed (default: random)");
    eprintln!("  --ttrpg                              Export travelable biome regions");
    eprintln!(
        "  --no-open                            Do not open the generated HTML in a browser\n"
    );
    std::process::exit(0);
}

fn help_world_sim() {
    eprintln!("roco world-sim — World building and simulation engine\n");
    eprintln!("Usage:");
    eprintln!("  roco world-sim              Start world simulation engine");
    eprintln!("In-game Commands:");
    eprintln!("  :inspect                   Inspect factions, regions, characters");
    eprintln!("  :tick                      Run one turn of organic simulation");
    eprintln!("  :influence <text>          Influence world events on next turn");
    eprintln!("  :generate <premise>        Generate a whole new world from a premise");
    eprintln!("  :history                   View historical timeline chronicles");
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

fn help_stats() {
    eprintln!("roco stats — Story & Workspace Statistics\n");
    eprintln!("Usage:");
    eprintln!("  roco stats [directory]           Show story/workspace stats");
    eprintln!("  roco stats --json                 JSON output");
    eprintln!("  roco stats [dir] --json           JSON output for directory\n");
    eprintln!("Analyzes a story directory for:");
    eprintln!("  - Chapter count, word count, character count");
    eprintln!("  - Estimated reading time");
    eprintln!("  - Outline completeness\n");
    std::process::exit(0);
}

fn help_interact() {
    eprintln!("roco interact — Interactive chat REPL\n");
    eprintln!("Usage:");
    eprintln!("  roco interact                       Start interactive chat");
    eprintln!("  roco interact <text>                 Chat with an opening message");
    eprintln!("  roco interact --prompt <text>        One-shot: generate, save, exit");
    eprintln!("  roco interact --resume <session>     Resume a saved session");
    eprintln!("  roco interact --resume <session> --instant  Instant resume (skip replay)");
    eprintln!("  roco interact --resume <session> --replay   Force full replay\n");
    eprintln!("  roco interact --list-sessions        List saved sessions");
    eprintln!("  roco interact --pace <mode>          Pacing: auto|careful|rolling|planning");
    eprintln!("  roco interact --seed <n>             Set deterministic seed for sampling");
    eprintln!(
        "  roco interact --trace                 Enable per-token trace logging for debugging\n"
    );
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

fn help_inspect() {
    eprintln!("roco inspect — Model & System Inspection\n");
    eprintln!("Usage:");
    eprintln!("  roco inspect                      Show all system information");
    eprintln!("  roco inspect caches               Show cache storage details");
    eprintln!("  roco inspect sessions             List saved sessions");
    eprintln!("  roco inspect config               Show configuration & env vars");
    eprintln!("  roco inspect model                Show model file information");
    eprintln!("  roco inspect seed                 Show determinism/seed info");
    eprintln!("  roco inspect --json               JSON output\n");
    eprintln!("Provides interpretability into model state, session cache,");
    eprintln!("generation parameters, and configuration. Useful for debugging");
    eprintln!("determinism and understanding model behaviour.\n");
    std::process::exit(0);
}

fn help_eval_suite() {
    eprintln!("roco eval-suite — Deterministic Evaluation Suite\n");
    eprintln!("Usage:");
    eprintln!("  roco eval-suite                   Run all deterministic evaluations");
    eprintln!("  roco eval-suite streaming          Run only streaming tests");
    eprintln!("  roco eval-suite identity           Run only identity tests");
    eprintln!("  roco eval-suite conversation       Run only conversation tests");
    eprintln!("  roco eval-suite --json             JSON output\n");
    eprintln!("Runs offline deterministic assertions to verify:");
    eprintln!("  - Stream monotonicity (text never shrinks)");
    eprintln!("  - Think-block stripping");
    eprintln!("  - Hallucinated turn cutting");
    eprintln!("  - Identity fast-path detection");
    eprintln!("  - Context budgeting\n");
    std::process::exit(0);
}

fn help_solution_bench() {
    eprintln!("roco solution-bench — Solution Evaluation Bench\n");
    eprintln!("Usage:");
    eprintln!("  roco solution-bench                  Run physical benchmarks and multi-workload simulation");
    eprintln!(
        "  roco solution-bench list             Show catalog of the 16 architectural aspects"
    );
    eprintln!("  roco solution-bench --json           JSON output format\n");
    eprintln!("Evaluates the 16 core aspects of SSM/RNN and Transformer-hybrid pipelines");
    eprintln!("such as state baking, BNFS grammars, subagents, multi-states, and routing.\n");
    std::process::exit(0);
}

fn help_completions() {
    eprintln!("roco completions — Generate shell completion script\n");
    eprintln!("Usage:");
    eprintln!("  roco completions bash             Generate bash completion script");
    eprintln!("  roco completions zsh              Generate zsh completion script");
    eprintln!("  roco completions fish             Generate fish completion script\n");
    std::process::exit(0);
}

/// Compute Levenshtein distance between two strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let mut costs = (0..=b.len()).collect::<Vec<_>>();
    for (i, ca) in a.chars().enumerate() {
        let mut last_cost = i;
        costs[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let old_cost = costs[j + 1];
            let cost = if ca == cb {
                last_cost
            } else {
                1 + last_cost.min(costs[j]).min(costs[j + 1])
            };
            last_cost = old_cost;
            costs[j + 1] = cost;
        }
    }
    costs[b.len()]
}

/// Known top-level subcommands in `roco`.
pub const KNOWN_SUBCOMMANDS: &[&str] = &[
    "story",
    "interact",
    "session",
    "code",
    "coder",
    "game",
    "ttrpg",
    "world-sim",
    "html",
    "inspect",
    "eval",
    "eval-suite",
    "solution-bench",
    "gui",
    "pet",
    "stats",
    "review",
    "export",
    "whoami",
    "version",
    "inferd",
    "server",
    "gateway",
    "gpu-check",
    "jobs",
    "reload",
    "stop",
    "rwkv",
    "grammar",
    "bless",
    "completions",
    "vector-search",
    "vector_search",
];

/// Find the closest matching subcommand for a given input if within distance 3.
pub fn suggest_subcommand(input: &str) -> Option<&'static str> {
    let mut best_match = None;
    let mut min_dist = usize::MAX;
    for &cmd in KNOWN_SUBCOMMANDS {
        let dist = levenshtein_distance(input, cmd);
        if dist < min_dist && dist <= 3 {
            min_dist = dist;
            best_match = Some(cmd);
        }
    }
    best_match
}

/// Generate shell completion script for bash, zsh, or fish.
pub fn generate_completions(shell: &str) {
    let cmds = KNOWN_SUBCOMMANDS.join(" ");
    match shell {
        "zsh" => {
            println!("#compdef roco");
            println!("_roco() {{");
            println!("    local -a commands");
            println!(
                "    commands=({})",
                KNOWN_SUBCOMMANDS
                    .iter()
                    .map(|c| format!("'{}'", c))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            println!("    _describe 'roco subcommand' commands");
            println!("}}");
            println!("compdef _roco roco");
        }
        "fish" => {
            for cmd in KNOWN_SUBCOMMANDS {
                println!("complete -c roco -n '__fish_use_subcommand' -a {}", cmd);
            }
        }
        _ => {
            // Default to bash
            println!("_roco_completions() {{");
            println!("    local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"");
            println!("    COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))", cmds);
            println!("}}");
            println!("complete -F _roco_completions roco");
        }
    }
}

#[cfg(test)]
mod autocomplete_tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("story", "story"), 0);
        assert_eq!(levenshtein_distance("stori", "story"), 1);
        assert_eq!(levenshtein_distance("exprot", "export"), 2);
        assert_eq!(levenshtein_distance("foo", "bar"), 3);
    }

    #[test]
    fn test_suggest_subcommand() {
        assert_eq!(suggest_subcommand("stori"), Some("story"));
        assert_eq!(suggest_subcommand("interac"), Some("interact"));
        assert_eq!(suggest_subcommand("exprot"), Some("export"));
        assert_eq!(suggest_subcommand("completely_unknown_long_str"), None);
    }
}
