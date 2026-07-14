//! `roco` — unified CLI for RoCo AI.
//!
//! Usage:
//!   roco eval [--output PATH]              run the RWKV eval suite
//!   roco bless [--snapshot PATH]           bless current outputs as new oracle
//!   roco rwkv                              smoke-test the RWKV backend
//!   roco grammar                           grammar-constrained decode
//!   roco gpu-check                         show Vulkan + model info

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let extra: Vec<&str> = args.iter().skip(2).map(|s| s.as_str()).collect();

    match sub {
        "eval" => cmd_eval(&extra),
        "bless" => cmd_bless(&extra),
        "rwkv" => run_cargo(
            "run",
            &["-p", "roco-core", "--example", "rwkv_test", "--release"],
            &extra,
        ),
        "grammar" => run_cargo(
            "run",
            &["-p", "roco-core", "--example", "grammar_smoke", "--release"],
            &extra,
        ),
        "gpu-check" => cmd_gpu_check(),
        _ => help(sub),
    }
}

// ── eval ────────────────────────────────────────────────────────────────────

fn cmd_eval(extra: &[&str]) {
    let output = parse_opt("--output", extra).unwrap_or("evals/results/latest.json");

    // Run eval, capture exit code.
    let exit_code = run_cargo_get_code(
        "run",
        &[
            "-p",
            "roco-core",
            "--example",
            "eval_suite",
            "--release",
            "--",
            "--backend",
            "rwkv",
        ],
        extra,
    );

    // Save snapshot regardless of pass/fail.
    let snapshot_path = snapshot_path(output);
    if let Ok(report) = std::fs::read_to_string(output) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&report) {
            if let Some(results) = parsed["results"].as_array() {
                let mut snap = serde_json::Map::new();
                for r in results {
                    let name = r["name"].as_str().unwrap_or("");
                    let out = r["output"].as_str().unwrap_or("").trim();
                    if !name.is_empty() {
                        snap.insert(name.to_string(), serde_json::Value::String(out.to_string()));
                    }
                }
                let snap_json = serde_json::Value::Object(snap);
                if let Ok(json_str) = serde_json::to_string_pretty(&snap_json) {
                    let _ = std::fs::write(&snapshot_path, &json_str);
                    eprintln!("Snapshot saved to: {}", snapshot_path.display());
                }
            }
        }
    }

    std::process::exit(exit_code);
}

// ── bless ───────────────────────────────────────────────────────────────────

fn cmd_bless(extra: &[&str]) {
    let snapshot = parse_opt("--snapshot", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| snapshot_path("evals/results/latest.json"));

    let snap: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&snapshot)
            .expect("snapshot file not found — run `roco eval` first"),
    )
    .expect("invalid snapshot JSON");

    let obj = snap.as_object().expect("snapshot must be a JSON object");

    // Read eval_suite.rs and eval_cases.rs, update oracle lines, write back.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let source_candidates = [
        PathBuf::from(&manifest_dir).join("src/eval_suite.rs"),
        PathBuf::from(&manifest_dir).join("crates/core/src/eval_suite.rs"),
        PathBuf::from(&manifest_dir).join("src/eval_cases.rs"),
        PathBuf::from(&manifest_dir).join("crates/core/src/eval_cases.rs"),
    ];
    let source_paths: Vec<PathBuf> = source_candidates
        .iter()
        .filter(|p| p.exists())
        .cloned()
        .collect();

    if source_paths.is_empty() {
        eprintln!("eval_suite.rs / eval_cases.rs not found");
        return;
    }

    let mut total_changed = 0;

    for source_path in &source_paths {
        let content = std::fs::read_to_string(source_path).expect("source not found");
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut changed = 0;

        for (name, out_val) in obj {
            let out_str = out_val.as_str().unwrap_or("");
            // Find the eval case block by name, then update its oracle line.
            if let Some(name_line) = lines
                .iter()
                .position(|l| l.trim() == &format!("name: \"{}\".into(),", name))
            {
                // Scan forward from name_line for the next `oracle:` line.
                let mut oracle_line = None;
                for i in name_line..lines.len() {
                    let trimmed = lines[i].trim();
                    if trimmed.starts_with("oracle: Some(") || trimmed.starts_with("oracle: None,") {
                        oracle_line = Some(i);
                        break;
                    }
                    if (trimmed.starts_with("category:") || trimmed.starts_with("name:"))
                        && i != name_line
                    {
                        break;
                    }
                }
                if let Some(oi) = oracle_line {
                    // Escape the output string for Rust source.
                    let escaped = out_str
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n");
                    let indent = &lines[oi][..lines[oi].len() - lines[oi].trim_start().len()];
                    lines[oi] = format!("{indent}oracle: Some(\"{escaped}\".into()),");
                    changed += 1;
                    eprintln!("  blessed {name}: \"{escaped}\"");
            } else {
                eprintln!("  skipping {name}: no oracle field found");
            }
        } else {
            eprintln!("  skipping {name}: eval case not found in eval_suite.rs");
        }
    }

    if changed > 0 {
        std::fs::write(source_path, lines.join("\n") + "\n")
            .expect("failed to write source file");
        eprintln!("\nBlessed {changed} oracle(s). Rebuild to pick up changes.");
    } else {
        eprintln!("No oracles blessed.");
    }
    total_changed += changed;
    }

    if total_changed > 0 {
        eprintln!("\nTotal blessed: {total_changed}");
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn run_cargo(cmd: &str, args: &[&str], extra: &[&str]) {
    let code = run_cargo_get_code(cmd, args, extra);
    std::process::exit(code);
}

fn run_cargo_get_code(cmd: &str, args: &[&str], extra: &[&str]) -> i32 {
    let mut c = Command::new("cargo");
    c.arg(cmd);
    c.args(args);
    c.args(extra);
    c.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
}

fn cmd_gpu_check() {
    println!("=== Vulkan devices ===");
    let _ = Command::new("vulkaninfo").arg("--summary").status();
    println!();
    println!("=== RWKV model ===");
    let _ = Command::new("ls")
        .args(["-lh", "models/rwkv7-g1h-2.9b-20260710-ctx10240-Q_K.st"])
        .status();
    println!("=== RWKV vocab ===");
    let _ = Command::new("ls")
        .args(["-lh", "assets/vocab/rwkv_vocab_v20230424.json"])
        .status();
}

fn help(sub: &str) {
    eprintln!("Usage: roco <subcommand> [args]");
    eprintln!();
    eprintln!("  eval [--output PATH]              Run evals, save snapshot");
    eprintln!("  bless [--snapshot PATH]            Bless snapshot as new oracle");
    eprintln!("  rwkv                              Smoke-test the RWKV backend");
    eprintln!("  grammar                           Grammar-constrained decode");
    eprintln!("  gpu-check                         Show Vulkan + model info");
    std::process::exit(if sub == "help" { 0 } else { 1 });
}

fn parse_opt<'a>(name: &str, args: &'a [&str]) -> Option<&'a str> {
    args.windows(2)
        .find_map(|w| if w[0] == name { Some(w[1]) } else { None })
}

fn snapshot_path(output: &str) -> PathBuf {
    let p = Path::new(output);
    let mut s = p.to_path_buf();
    s.set_extension("snapshot.json");
    s
}
