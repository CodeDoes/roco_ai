//! Standalone model eval runner.

use std::env;
use std::path::PathBuf;

use roco_engine::{MockBackend, EvalReport, EvalCase,
    run_suite, cases::default_eval_suite, cases::message_eval_cases, cases::fim_eval_cases, write_report, print_report, write_sidecars,
};
use roco_inference::RwkvBackend;
use roco_infer_client::RemoteBackend;
use roco_bnf_engine::create_bnf_mask;
use roco_grammar::gbnf_to_kbnf;
use roco_engine::{ModelBackend, BnfMask};

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
            "--backend" => { i += 1; if i < args.len() { backend = args[i].clone(); } }
            "--filter" => { i += 1; if i < args.len() { filter = Some(args[i].clone()); } }
            "--output" => { i += 1; if i < args.len() { output = PathBuf::from(&args[i]); } }
            "--suite" => { i += 1; if i < args.len() { suite = args[i].clone(); } }
            "--help" | "-h" => {
                println!("Usage: cargo run --example eval_suite [OPTIONS]");
                println!("  --backend STR    mock, rwkv, or remote [default: mock]");
                println!("  --filter STR     Filter eval cases");
                println!("  --output PATH    Report path [default: evals/results/latest.json]");
                println!("  --suite STR      Suite name [default: roco-eval-suite]");
                std::process::exit(0);
            }
            _ => { eprintln!("Unknown: {}", args[i]); std::process::exit(1); }
        }
        i += 1;
    }
    Args { backend, filter, output, suite }
}

/// Build BNF masks for every eval case that carries a `grammar` string, using
/// the backend's vocabulary. This must happen in the application layer (not in
/// `roco-engine`): the inference backend cannot construct masks itself without
/// pulling grammar-engine types into the same compilation unit as `web-rwkv`
/// (`error[E0275]`). Cases without a grammar, or backends without a vocab
/// (e.g. `MockBackend`), are left unconstrained.
fn build_masks<B: ModelBackend + ?Sized>(backend: &B, cases: &mut [EvalCase]) {
    let Some(vocab) = backend.vocab_bytes() else { return };
    for case in cases.iter_mut() {
        if let Some(g) = &case.grammar {
            let kbnf = gbnf_to_kbnf(g);
            if let Ok(mask) = create_bnf_mask(&kbnf, &vocab) {
                case.bnf_mask = Some(mask);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();
    let args = parse_args();
    let trace_path = {
        let stem = args.output.file_stem().map(|s| format!("{}_trace.txt", s.to_string_lossy())).unwrap_or_else(|| "eval_trace.txt".to_string());
        Some(args.output.with_file_name(stem))
    };

    let report: EvalReport = match args.backend.as_str() {
        "mock" => {
            let backend = MockBackend::new("mock-3b", 0);
            run_suite(&args.suite, &backend, default_eval_suite(), args.filter.as_deref(), trace_path.as_deref()).await
        }
        "rwkv" => {
            let backend = RwkvBackend::from_env().unwrap_or_else(|e| {
                eprintln!("ERROR: Failed to create RwkvBackend: {e}");
                eprintln!("Set RWKV_MODEL and RWKV_VOCAB environment variables.");
                std::process::exit(1);
            });
            // The real model additionally runs the message-layer baseline
            // probes (system-instruction following + user-turn coherence),
            // the FIM completion probes (the shape the Zed/VS Code LSP sends),
            // which the non-semantic MockBackend cannot represent.
            let mut cases = default_eval_suite();
            cases.extend(message_eval_cases());
            cases.extend(fim_eval_cases());
            // Bake the few-shot FIM examples into a named session so the FIM
            // bridge cases resume from the correct recurrent state (state-tuning).
            if let Err(e) = roco_engine::bake_fim_session(&backend).await {
                eprintln!("WARN: FIM session bake failed: {e}");
            }
            build_masks(&backend, &mut cases);
            run_suite(&args.suite, &backend, cases, args.filter.as_deref(), trace_path.as_deref()).await
        }
        "remote" => {
            // Talk to the singleton inference API server (the same one the
            // Zed/VS Code LSP uses via RemoteBackend). Requires the server to
            // be running, e.g. `roco server --story --detach`.
            let backend = roco_infer_client::RemoteBackend::from_env();
            // Bake the few-shot FIM examples into a named session so the FIM
            // cases resume from the correct recurrent state (state-tuning).
            if let Err(e) = roco_engine::bake_fim_session(&backend).await {
                eprintln!("WARN: FIM session bake failed: {e}");
            }
            let mut cases = default_eval_suite();
            cases.extend(message_eval_cases());
            cases.extend(fim_eval_cases());
            build_masks(&backend, &mut cases);
            run_suite(&args.suite, &backend, cases, args.filter.as_deref(), trace_path.as_deref()).await
        }
        other => { eprintln!("Unknown backend: {other}"); std::process::exit(1); }
    };

    write_report(&args.output, &report).ok();
    print_report(&report);
    println!("Report written to: {}", args.output.display());
    if let Some(ref trace_path) = trace_path {
        println!("Trace written to:  {}", trace_path.display());
        write_sidecars(&report, trace_path);
    }
    if report.failed > 0 { std::process::exit(1); }
}
