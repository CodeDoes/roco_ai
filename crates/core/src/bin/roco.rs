//! `roco` — unified CLI for RoCo AI.
//!
//! Usage:
//!   roco eval        run the RWKV eval suite (--release)
//!   roco rwkv        smoke-test the RWKV backend
//!   roco grammar     grammar-constrained decode smoke test
//!   roco gpu-check   show Vulkan device + model/vocab status

use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => run_cargo("run", &[
            "-p", "roco-core", "--example", "eval_suite",
            "--release", "--", "--backend", "rwkv",
        ], &extra),
        "rwkv" => run_cargo("run", &[
            "-p", "roco-core", "--example", "rwkv_test", "--release",
        ], &extra),
        "grammar" => run_cargo("run", &[
            "-p", "roco-core", "--example", "grammar_smoke", "--release",
        ], &extra),
        "gpu-check" => cmd_gpu_check(),
        _ => help(sub),
    }
}

fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    let status = c.status().expect("cargo failed");
    std::process::exit(status.code().unwrap_or(1));
}

fn cmd_gpu_check() {
    println!("=== Vulkan devices ===");
    let _ = Command::new("vulkaninfo").arg("--summary").status();
    println!();
    println!("=== RWKV model ===");
    let _ = Command::new("ls")
        .args(["-lh", "models/rwkv7-g1g-2.9b-20260526-ctx8192-converted.st"])
        .status();
    println!("=== RWKV vocab ===");
    let _ = Command::new("ls")
        .args(["-lh", "assets/vocab/rwkv_vocab_v20230424.json"])
        .status();
}

fn help(sub: &str) {
    eprintln!("Usage: roco <subcommand> [args]");
    eprintln!();
    eprintln!("  eval        Run the RWKV eval suite (--release)");
    eprintln!("  rwkv        Smoke-test the RWKV backend");
    eprintln!("  grammar     Grammar-constrained decode smoke test");
    eprintln!("  gpu-check   Show Vulkan device + model/vocab status");
    std::process::exit(if sub == "help" { 0 } else { 1 });
}
