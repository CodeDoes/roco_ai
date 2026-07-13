//! Standalone model eval runner.
//!
//! Runs the eval suite against any configured backend and produces a JSON report.
//!
//! ```bash
//! # Mock backend (no model needed)
//! cargo run --example eval_suite
//!
//! # Local RWKV (release mode required for GPU)
//! cargo run --example eval_suite --release -- --backend rwkv
//!
//! # Filter to specific evals
//! cargo run --example eval_suite -- --filter smoke
//!
//! # Write report to file
//! cargo run --example eval_suite -- --output evals/results/latest.json
//! ```

use std::env;
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

use roco_core::engine::MockBackend;
use roco_core::eval_suite::{self, EvalReport};

#[cfg(feature = "local-rwkv")]
use roco_core::rwkv_backend::RwkvBackend;

struct Args {
    backend: String,
    filter: Option<String>,
    output: PathBuf,
    suite: String,
}

fn parse_args() -> Args {
    let args: Vec<String> = env::args().collect();
    let mut backend = "mock".to_string();
    let mut filter: Option<String> = None;
    let mut output = PathBuf::from("evals/results/latest.json");
    let mut suite = "roco-eval-suite".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                i += 1;
                if i < args.len() {
                    backend = args[i].clone();
                }
            }
            "--filter" => {
                i += 1;
                if i < args.len() {
                    filter = Some(args[i].clone());
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() {
                    output = PathBuf::from(&args[i]);
                }
            }
            "--suite" => {
                i += 1;
                if i < args.len() {
                    suite = args[i].clone();
                }
            }
            "--help" | "-h" => {
                println!("Usage: cargo run --example eval_suite [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --backend STR    Backend to use: mock, rwkv [default: mock]");
                println!("  --filter STR     Filter eval cases by name or category");
                println!("  --output PATH    Output path for JSON report [default: evals/results/latest.json]");
                println!("  --suite STR      Suite name for the report [default: roco-eval-suite]");
                println!("  --help, -h       Print this help");
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Use --help for usage info.");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args {
        backend,
        filter,
        output,
        suite,
    }
}

fn setup_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

#[tokio::main]
async fn main() {
    setup_tracing();
    let args = parse_args();

    // Build the backend
    let trace_path: Option<std::path::PathBuf> = {
        let stem = args
            .output
            .file_stem()
            .map(|s| format!("{}_trace.txt", s.to_string_lossy()))
            .unwrap_or_else(|| "eval_trace.txt".to_string());
        Some(args.output.with_file_name(stem))
    };

    let report: EvalReport = match args.backend.as_str() {
        "mock" => {
            let backend = MockBackend {
                name: "mock-3b".into(),
                ..Default::default()
            };
            let cases = eval_suite::default_eval_suite();
            eval_suite::run_suite(
                &args.suite,
                &backend,
                &cases,
                args.filter.as_deref(),
                trace_path.as_deref(),
            )
            .await
        }

        #[cfg(feature = "local-rwkv")]
        "rwkv" => {
            let backend = RwkvBackend::from_env().unwrap_or_else(|e| {
                eprintln!("ERROR: Failed to create RwkvBackend: {e}");
                eprintln!("Set RWKV_MODEL and RWKV_VOCAB environment variables.");
                std::process::exit(1);
            });
            let cases = eval_suite::default_eval_suite();
            eval_suite::run_suite(
                &args.suite,
                &backend,
                &cases,
                args.filter.as_deref(),
                trace_path.as_deref(),
            )
            .await
        }

        other => {
            eprintln!("Unknown backend: {other}. Choose: mock, rwkv");
            std::process::exit(1);
        }
    };

    // Write report
    eval_suite::write_report(&args.output, &report).unwrap_or_else(|e| {
        eprintln!(
            "WARNING: could not write report to {}: {e}",
            args.output.display()
        );
    });

    // Print human-readable summary
    eval_suite::print_report(&report);
    println!("Report written to:  {}", args.output.display());
    if let Some(ref trace_path) = trace_path {
        println!("Trace written to:   {}", trace_path.display());
        eval_suite::write_sidecars(&report, trace_path);
    }

    // Exit with code if any evals failed
    if report.failed > 0 {
        std::process::exit(1);
    }
}
