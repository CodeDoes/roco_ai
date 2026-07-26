//! `roco` — thin CLI dispatcher.
//!
//! FILE STATUS: EDITABLE. Subcommand bodies live in `roco_cli::cmd::*`
//! so this file stays small and cheap to recompile.

use roco_cli::cmd;
use roco_cli::{help, parse_opt, run_cargo};

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
        .filter(|&s| s != "--mock")
        .collect();
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
        "eval" => cmd::eval::cmd_eval(&extra),
        "bless" => cmd::eval::cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &[
                "-p",
                "roco-inference",
                "--example",
                "rwkv_test",
                "--release",
            ],
            &extra,
        ),
        "grammar" => run_cargo(
            "run",
            &[
                "-p",
                "roco-inference",
                "--example",
                "grammar_smoke",
                "--release",
            ],
            &extra,
        ),
        "gpu-check" => cmd::gpu::cmd_gpu_check(&extra),
        "jobs" | "inferd-jobs" | "inferd-status" => cmd::jobs::cmd_jobs(&extra),
        "inferd" | "server" => {
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("stop") => {
                    roco_cli::daemon::stop_inference();
                }
                Some("start") => {
                    let exe = std::env::current_exe().expect("exe");
                    if roco_cli::daemon::ensure_inference_daemon(&exe, roco_cli::daemon::INFERENCE_PORT) {
                        println!("✓ roco-inferd started on port {}.", roco_cli::daemon::INFERENCE_PORT);
                    }
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
                _ => {
                    #[cfg(feature = "net")]
                    cmd::server::cmd_server(&extra);
                    #[cfg(not(feature = "net"))]
                    need_feature("inferd", "net", "cargo run -p roco-inferd");
                }
            }
        }
        "gateway" => {
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("stop") => {
                    roco_cli::daemon::stop_gateway();
                }
                Some("start") => {
                    let exe = std::env::current_exe().expect("exe");
                    if roco_cli::daemon::ensure_daemon(&exe, "gateway", roco_cli::daemon::GATEWAY_PORT, &["--detach"]) {
                        println!("✓ Gateway started on port {}.", roco_cli::daemon::GATEWAY_PORT);
                    }
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
                    let running = roco_cli::daemon::is_running("gateway", roco_cli::daemon::GATEWAY_PORT);
                    if running {
                        println!("✓ Gateway is running on port {}.", roco_cli::daemon::GATEWAY_PORT);
                    } else {
                        println!("✗ Gateway is not running on port {}.", roco_cli::daemon::GATEWAY_PORT);
                    }
                }
                _ => {
                    #[cfg(feature = "net")]
                    cmd::server::cmd_gateway(&extra);
                    #[cfg(not(feature = "net"))]
                    need_feature("gateway", "net", "cargo build -p roco-cli --features net");
                }
            }
        }
        "reload" => {
            #[cfg(feature = "net")]
            cmd::server::cmd_reload(&extra);
            #[cfg(not(feature = "net"))]
            need_feature("reload", "net", "cargo build -p roco-cli --features net");
        }
        "gui" => {
            #[cfg(feature = "desktop")]
            cmd::desktop::cmd_gui(&extra);
            #[cfg(not(feature = "desktop"))]
            need_feature(
                "gui",
                "desktop",
                "cargo build -p roco-cli --features desktop",
            );
        }
        "stop" => {
            let sub_cmd = extra.first().copied();
            match sub_cmd {
                Some("gateway") => roco_cli::daemon::stop_gateway(),
                Some("inferd") | Some("server") => roco_cli::daemon::stop_inference(),
                _ => roco_cli::daemon::stop_all(),
            }
        }
        "story-mode" | "sm" => {
            let story_name = parse_opt("--story", &extra);
            let command = extra.first().copied();
            match command {
                Some(cmd) if !cmd.starts_with("--") => {
                    cmd::story_mode::run_story_command(story_name, cmd);
                }
                _ => {
                    cmd::story_mode::run_story_mode(story_name);
                }
            }
        }
        "story" => cmd::story::cmd_story(&extra),
        "game" => cmd::game::cmd_game(&extra),
        "html" => cmd::html::cmd_html(&extra),
        "code" => cmd::coder::cmd_coder(&extra),
        "interact" => cmd::interact::cmd_interact(&extra),
        "coder" => cmd::coder::cmd_coder(&extra),
        "export" => {
            cmd::export::run(
                extra.first().copied().unwrap_or("."),
                parse_opt("--format", &extra),
                parse_opt("--output", &extra),
            );
        }
        "help" | "--help" | "-h" => help(None),
        "pet" => cmd::pet::cmd_pet(&extra),
        "router" => cmd::router::cmd_router(&extra),
        _ => {
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
