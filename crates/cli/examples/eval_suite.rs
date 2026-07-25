//! Single-eval runner for the RoCo eval harness.
//!
//! Runs ONE eval case at a time against a backend so you can isolate
//! failures without noise from the rest of the suite.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --example eval_suite -p roco-cli --features net -- http://127.0.0.1:8080 story_outline_json
//! cargo run --release --example eval_suite -p roco-cli --features net -- http://127.0.0.1:8080 story_chapter_json
//! ```
//!
//! The first non-flag argument after `--` is the backend URL.
//! The second non-flag argument is the eval case name.

use std::env;
use std::path::PathBuf;

use roco_engine::eval::run_eval;
use roco_engine::{ModelBackend, cases::default_eval_suite};
use roco_infer_client::RemoteBackend;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut backend_url = "http://127.0.0.1:8080".to_string();
    let mut case_name: Option<String> = None;

    let mut positional_count = 0usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" | "--url" | "--host" => {
                i += 1;
                if let Some(url) = args.get(i) {
                    backend_url = url.clone();
                }
            }
            _ => {
                if !args[i].starts_with("--") {
                    positional_count += 1;
                    if positional_count == 1 {
                        backend_url = args[i].clone();
                    } else if positional_count == 2 {
                        case_name = Some(args[i].clone());
                    }
                }
            }
        }
        i += 1;
    }

    let backend = RemoteBackend::new(backend_url);

    let suite = default_eval_suite();
    let names: Vec<String> = suite.iter().map(|c| c.name.clone()).collect();
    let target = match case_name {
        Some(name) => name,
        None => {
            eprintln!("Available eval cases:");
            for name in &names {
                eprintln!("  {name}");
            }
            eprintln!("\nUsage: cargo run --release --example eval_suite -p roco-cli --features net -- <url> <case-name>");
            std::process::exit(1);
        }
    };

    let case = suite.into_iter().find(|c| c.name == target).unwrap_or_else(|| {
        eprintln!("Unknown eval case: {target}");
        eprintln!("Available eval cases:");
        for name in &names {
            eprintln!("  {name}");
        }
        std::process::exit(1);
    });

    eprintln!("═══ Single Eval ═══");
    eprintln!("Backend: {}", backend.name());
    eprintln!("Case:    {} ({})", case.name, case.category);
    eprintln!("Description: {}", case.description);
    eprintln!();

    let trace_path = PathBuf::from(format!("evals/results/{}_trace.txt", case.name));

    let result = run_eval(
        &backend,
        case,
        Some(&trace_path),
        None,
        true, // live_console: stream tokens to stdout
    )
    .await;

    let symbol = if result.passed { "PASS" } else { "FAIL" };
    eprintln!("\n═══ Result: {} ═══", symbol);
    eprintln!("Case:      {}", result.name);
    eprintln!("Category:  {}", result.category);
    eprintln!("Passed:    {}", result.passed);

    for check in &result.checks {
        let s = if check.passed { "PASS" } else { "FAIL" };
        eprintln!("  [{}] {}: {}", s, check.name, check.detail);
    }

    for err in &result.errors {
        eprintln!("  [ERROR] {}", err);
    }

    if result.latency_ms > 0 {
        eprint!("Latency:    {}ms", result.latency_ms);
        if result.token_usage.completion_tokens > 0 {
            eprint!(
                ", {} tok/s ({}+{} tokens)",
                result.tokens_per_sec.round(),
                result.token_usage.prompt_tokens,
                result.token_usage.completion_tokens
            );
        }
        eprintln!();
    }

    if !result.passed {
        std::process::exit(1);
    }
}
