//! `roco` — thin CLI dispatcher.
//!
//! FILE STATUS: EDITABLE. Subcommand bodies live in `roco_cli::cmd::*`
//! so this file stays small and cheap to recompile.

use roco_cli::cmd;
use roco_cli::{has_help_flag, help, parse_opt, run_cargo};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --mock flag to enable mock backend across all subcommands
    if args.iter().any(|a| a == "--mock") {
        std::env::set_var("ROCO_USE_MOCK_BACKEND", "1");
    }

    // Initialize AgentJournal for client-side logging
    let _ = roco_app::agent_journal::AgentJournal::init();

    // Load config before anything else so RWKV_MODEL / RWKV_VOCAB propagate.
    let cfg = roco_app::RoCoConfig::load();
    cfg.apply_to_environment();
    let filtered_args: Vec<&str> = args
        .iter()
        .skip(1)
        .map(|s| s.as_str())
        .filter(|&s| s != "--mock" && !s.is_empty())
        .collect();

    // Handle `-p` / `--prompt` flag at the top level: `roco -p "text"`.
    // Skip when the subcommand consumes `-p` itself — `roco session <id> -p`
    // must reach cmd_session, not prompt mode.
    let first_sub = filtered_args.first().copied().unwrap_or("");
    if first_sub != "session" {
        if let Some(prompt) =
            parse_opt("-p", &filtered_args).or_else(|| parse_opt("--prompt", &filtered_args))
        {
            if prompt.is_empty() {
                eprintln!("Error: -p requires a non-empty prompt");
                std::process::exit(1);
            }
            // Route to interact with prompt mode
            cmd::interact::cmd_interact(&["--prompt", prompt]);
            return;
        }
    }

    let sub = filtered_args.first().copied().unwrap_or("router");
    let extra: Vec<&str> = if filtered_args.is_empty() {
        vec![]
    } else {
        filtered_args[1..].to_vec()
    };

    roco_app::agent_journal::AgentJournal::info(
        "client",
        &format!("roco command: sub={sub}, extra={:?}", extra),
    );

    match sub {
        // ── Help — always show help when asked ────────────────────────────
        "help" | "--help" | "-h" => {
            let topic = extra.first().filter(|&&t| !t.starts_with('-')).copied();
            let hidden = extra.contains(&"--hidden");
            help(topic, hidden);
        }

        // ── Evaluations ──────────────────────────────────────────────────
        "eval" => {
            if has_help_flag(&extra) {
                help(Some("eval"), false);
            }
            cmd::eval::cmd_eval(&extra);
        }
        "bless" => {
            if has_help_flag(&extra) {
                help(Some("eval"), false);
            }
            cmd::eval::cmd_bless(&extra);
        }

        // ── RWKV backend smoke tests ─────────────────────────────────────
        "rwkv" => {
            if has_help_flag(&extra) {
                help(Some("rwkv"), false);
            }
            run_cargo(
                "run",
                &[
                    "-p",
                    "roco-engine-gpu",
                    "--example",
                    "rwkv_test",
                    "--release",
                ],
                &extra,
            );
        }
        "grammar" => {
            if has_help_flag(&extra) {
                help(Some("grammar"), false);
            }
            run_cargo(
                "run",
                &[
                    "-p",
                    "roco-engine-gpu",
                    "--example",
                    "grammar_smoke",
                    "--release",
                ],
                &extra,
            );
        }

        // ── Debug REPL ───────────────────────────────────────────────────
        "debug" => {
            cmd::debug::cmd_debug(&extra);
        }

        // ── GPU / Jobs ───────────────────────────────────────────────────
        "gpu-check" => {
            if has_help_flag(&extra) {
                help(Some("gpu-check"), false);
            }
            cmd::gpu::cmd_gpu_check(&extra);
        }
        "jobs" | "inferd-jobs" | "inferd-status" => {
            if has_help_flag(&extra) {
                help(Some("jobs"), false);
            }
            cmd::jobs::cmd_jobs(&extra);
        }

        // ── Inference daemon control ─────────────────────────────────────
        "inferd" | "server" => {
            // --help / -h anywhere in args -> show help
            if has_help_flag(&extra) {
                help(Some("inferd"), false);
            }
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("start") => {
                    let exe = std::env::current_exe().expect("exe");
                    if roco_cli::daemon::ensure_inference_daemon(
                        &exe,
                        roco_cli::daemon::INFERENCE_PORT,
                    ) {
                        println!(
                            "✓ roco-inferd started on port {}.",
                            roco_cli::daemon::INFERENCE_PORT
                        );
                    }
                }
                Some("stop") => {
                    roco_cli::daemon::stop_inference();
                }
                Some("restart") | Some("reload") => {
                    #[cfg(feature = "net")]
                    cmd::server::cmd_inferd_reload(&extra[1..]);
                    #[cfg(not(feature = "net"))]
                    need_feature(
                        "inferd reload",
                        "net",
                        "cargo build -p roco-cli --features net",
                    );
                }
                Some("status") | Some("jobs") => {
                    cmd::jobs::cmd_jobs(&extra[1..]);
                }
                Some("help") => {
                    help(Some("inferd"), false);
                }
                _ => {
                    // No recognized subcommand: pass through to cmd_server
                    // which handles flags like --port, --story, --stdio-lsp, --detach.
                    // This preserves backward compat with `roco server --story ...`
                    // and direct flag-based invocations.
                    #[cfg(feature = "net")]
                    cmd::server::cmd_server(&extra);
                    #[cfg(not(feature = "net"))]
                    need_feature("inferd", "net", "cargo run -p roco-inferd");
                }
            }
        }

        // ── Gateway control ──────────────────────────────────────────────
        "gateway" => {
            // --help / -h anywhere in args -> show help
            if has_help_flag(&extra) {
                help(Some("gateway"), false);
            }
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("start") => {
                    let exe = std::env::current_exe().expect("exe");
                    if roco_cli::daemon::ensure_daemon(
                        &exe,
                        "gateway",
                        roco_cli::daemon::GATEWAY_PORT,
                        &["--detach"],
                    ) {
                        println!(
                            "✓ Gateway started on port {}.",
                            roco_cli::daemon::GATEWAY_PORT
                        );
                    }
                }
                Some("stop") => {
                    roco_cli::daemon::stop_gateway();
                }
                Some("restart") | Some("reload") => {
                    #[cfg(feature = "net")]
                    cmd::server::cmd_gateway_reload(&extra[1..]);
                    #[cfg(not(feature = "net"))]
                    need_feature(
                        "gateway reload",
                        "net",
                        "cargo build -p roco-cli --features net",
                    );
                }
                Some("status") => {
                    let running =
                        roco_cli::daemon::is_running("gateway", roco_cli::daemon::GATEWAY_PORT);
                    if running {
                        println!(
                            "✓ Gateway is running on port {}.",
                            roco_cli::daemon::GATEWAY_PORT
                        );
                    } else {
                        println!(
                            "✗ Gateway is not running on port {}.",
                            roco_cli::daemon::GATEWAY_PORT
                        );
                    }
                }
                Some("help") => {
                    help(Some("gateway"), false);
                }
                _ => {
                    // No recognized subcommand: pass through to cmd_gateway
                    // which handles flags like --detach, --port, --target.
                    // This preserves backward compat with `roco gateway --detach`.
                    #[cfg(feature = "net")]
                    cmd::server::cmd_gateway(&extra);
                    #[cfg(not(feature = "net"))]
                    need_feature("gateway", "net", "cargo build -p roco-cli --features net");
                }
            }
        }

        // ── Reload both daemons ──────────────────────────────────────────
        "reload" => {
            if has_help_flag(&extra) {
                help(Some("reload"), false);
            }
            #[cfg(feature = "net")]
            cmd::server::cmd_reload(&extra);
            #[cfg(not(feature = "net"))]
            need_feature("reload", "net", "cargo build -p roco-cli --features net");
        }

        // ── Stop daemons (NEVER starts anything) ─────────────────────────
        "stop" => {
            if has_help_flag(&extra) {
                help(Some("stop"), false);
            }
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("gateway") => roco_cli::daemon::stop_gateway(),
                Some("inferd") | Some("server") => roco_cli::daemon::stop_inference(),
                _ => roco_cli::daemon::stop_all(),
            }
        }

        // ── Desktop GUI ──────────────────────────────────────────────────
        "gui" => {
            if has_help_flag(&extra) {
                help(Some("gui"), false);
            }
            #[cfg(any(feature = "gui", feature = "desktop"))]
            cmd::desktop::cmd_gui(&extra);
            #[cfg(not(any(feature = "gui", feature = "desktop")))]
            need_feature("gui", "gui", "cargo build -p roco-cli --features gui");
        }

        // ── Desktop pet ──────────────────────────────────────────────────
        "pet" => {
            if has_help_flag(&extra) {
                help(Some("pet"), false);
            }
            cmd::pet::cmd_pet(&extra);
        }

        // ── Session control ──────────────────────────────────────────────
        "session" => {
            if has_help_flag(&extra) {
                help(Some("session"), false);
            }
            cmd::session::cmd_session(&extra);
        }

        // ── Status ───────────────────────────────────────────────────────
        "status" => {
            if has_help_flag(&extra) {
                help(Some("status"), false);
            }
            cmd::status::cmd_status(&extra);
        }

        // ── Story mode (interactive writing assistant) ───────────────────
        "story-mode" | "sm" => {
            if has_help_flag(&extra) {
                help(Some("story"), false);
            }
            let story_name = parse_opt("--story", &extra);
            let command = extra.first().copied();
            match command {
                Some("help") => help(Some("story"), false),
                Some(cmd) if !cmd.starts_with("--") => {
                    cmd::story_mode::run_story_command(story_name, cmd);
                }
                _ => {
                    cmd::story_mode::run_story_mode(story_name);
                }
            }
        }

        // ── Structured story pipeline ────────────────────────────────────
        "story" => {
            if has_help_flag(&extra) {
                help(Some("story"), false);
            }
            if extra.first() == Some(&"branch") {
                cmd::story::cmd_story_branch(&extra[1..]);
            } else if extra.first() == Some(&"edit") {
                cmd::story::cmd_story_edit(&extra[1..]);
            } else {
                cmd::story::cmd_story(&extra);
            }
        }

        // ── Game / HTML / Code / Interact ────────────────────────────────
        "game" => {
            if has_help_flag(&extra) {
                help(Some("game"), false);
            }
            cmd::game::cmd_game(&extra);
        }
        "ttrpg" => {
            if has_help_flag(&extra) {
                help(Some("ttrpg"), false);
            }
            cmd::ttrpg::cmd_ttrpg(&extra);
        }
        "world-sim" => {
            if has_help_flag(&extra) {
                help(Some("world-sim"), false);
            }
            cmd::world_sim::cmd_world_sim(&extra);
        }

        "map" => {
            if has_help_flag(&extra) {
                help(Some("map"), false);
            }
            cmd::wfc::cmd_map(&extra);
        }
        "html" => {
            if has_help_flag(&extra) {
                help(Some("html"), false);
            }
            cmd::html::cmd_html(&extra);
        }
        "code" | "coder" => {
            if has_help_flag(&extra) {
                help(Some("code"), false);
            }
            cmd::coder::cmd_coder(&extra);
        }
        "interact" => {
            if has_help_flag(&extra) {
                help(Some("interact"), false);
            }
            cmd::interact::cmd_interact(&extra);
        }
        "workspace" => {
            if has_help_flag(&extra) {
                help(Some("workspace"), false);
            }
            cmd::workspace::cmd_workspace(&extra);
        }
        "vector-search" | "vector_search" => {
            if has_help_flag(&extra) {
                help(Some("vector-search"), false);
            }
            cmd::vector_search::cmd_vector_search(&extra);
        }

        // ── Export ───────────────────────────────────────────────────────
        "export" => {
            if has_help_flag(&extra) {
                help(Some("export"), false);
            }
            cmd::export::run(
                extra.first().copied().unwrap_or("."),
                parse_opt("--format", &extra),
                parse_opt("--output", &extra),
            );
        }

        // ── Stats & Review ───────────────────────────────────────────────
        "stats" | "review" => {
            if has_help_flag(&extra) {
                help(Some("stats"), false);
            }
            cmd::stats::cmd_stats(&extra);
        }

        // ── Inspect (interpretability, state, config) ────────────────────
        "inspect" => {
            if has_help_flag(&extra) {
                help(Some("inspect"), false);
            }
            cmd::inspect::cmd_inspect(&extra);
        }

        // ── Deterministic Eval Suite ─────────────────────────────────────
        "eval-suite" => {
            if has_help_flag(&extra) {
                help(Some("eval-suite"), false);
            }
            cmd::eval_suite::cmd_eval_suite(&extra);
        }

        // ── Solution Bench ───────────────────────────────────────────────
        "solution-bench" => {
            if has_help_flag(&extra) {
                help(Some("solution-bench"), false);
            }
            cmd::solution_bench::cmd_solution_bench(&extra);
        }

        // ── Shell Completions ────────────────────────────────────────────
        "completions" => {
            if has_help_flag(&extra) {
                help(Some("completions"), false);
            }
            let shell = extra.first().copied().unwrap_or("bash");
            roco_cli::generate_completions(shell);
            std::process::exit(0);
        }

        // ── Identity ─────────────────────────────────────────────────────
        "whoami" | "who-am-i" => {
            if has_help_flag(&extra) {
                help(Some("whoami"), false);
            }
            roco_cli::identity::cmd_whoami(&extra);
        }

        // ── Quickstart ───────────────────────────────────────────────────
        "quickstart" => {
            if has_help_flag(&extra) {
                help(Some("quickstart"), false);
            }
            cmd::quickstart::cmd_quickstart(&extra);
        }

        // ── Version ─────────────────────────────────────────────────────
        "version" | "--version" => {
            eprintln!(
                "RoCo AI v{} — collaborative writing assistant",
                env!("CARGO_PKG_VERSION")
            );
            std::process::exit(0);
        }

        // ── Router (default — no subcommand or unknown) ──────────────────
        "router" => cmd::router::cmd_router(&extra),

        // ── Unknown subcommand ───────────────────────────────────────────
        _ => {
            // If the user typed something like "roco unknown --help", show help.
            if has_help_flag(&extra) {
                help(None, false);
            }
            // Check if user mistyped a known subcommand and suggest the closest match
            if let Some(suggestion) = roco_cli::suggest_subcommand(sub) {
                eprintln!("Note: Unknown subcommand '{sub}'. Did you mean 'roco {suggestion}'?\n");
            }
            // Unknown subcommand → route through mode router with that text as prompt.
            let mut args_with_prompt = vec![sub];
            args_with_prompt.extend(extra.iter().copied());
            cmd::router::cmd_router(&args_with_prompt);
        }
    }
}

#[allow(dead_code)]
fn need_feature(cmd: &str, feature: &str, hint: &str) {
    eprintln!("error: `roco {cmd}` requires `--features {feature}`.");
    eprintln!("rebuild with: cargo build -p roco-cli --features {feature}");
    eprintln!("or:            {hint}");
    std::process::exit(2);
}
