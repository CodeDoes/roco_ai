//! `roco` — thin CLI dispatcher.
//!
//! FILE STATUS: EDITABLE. Subcommand bodies live in `roco_cli::cmd::*`
//! so this file stays small and cheap to recompile.

use roco_cli::cmd;
use roco_cli::{help, parse_opt, run_cargo};

fn main() {
    // Load config before anything else so RWKV_MODEL / RWKV_VOCAB propagate.
    let cfg = roco_app::RoCoConfig::load();
    cfg.apply_to_environment();

    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("interact");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => cmd::eval::cmd_eval(&extra),
        "bless" => cmd::eval::cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &["-p", "roco-inference", "--example", "rwkv_test", "--release"],
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
        "server" => {
            #[cfg(feature = "net")]
            cmd::server::cmd_server(&extra);
            #[cfg(not(feature = "net"))]
            need_feature("server", "net", "cargo run -p roco-inferd");
        }
        "gateway" => {
            #[cfg(feature = "net")]
            cmd::server::cmd_gateway(&extra);
            #[cfg(not(feature = "net"))]
            need_feature("gateway", "net", "cargo build -p roco-cli --features net");
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
            roco_cli::daemon::stop_all();
        }
        "story" => cmd::story::cmd_story(&extra),
        "interact" => cmd::interact::cmd_interact(&extra),
        "export" => {
            cmd::export::run(
                extra.first().copied().unwrap_or("."),
                parse_opt("--format", &extra),
                parse_opt("--output", &extra),
            );
        }
        "help" | "--help" | "-h" => help(None),
        _ => {
            // Unknown subcommand → interact with that text as the prompt.
            let mut args_with_prompt = vec![sub];
            args_with_prompt.extend(extra.iter().copied());
            cmd::interact::cmd_interact(&args_with_prompt);
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
