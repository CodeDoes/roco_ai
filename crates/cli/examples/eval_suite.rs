//! Standalone model eval runner.

use std::env;
use std::path::PathBuf;

use roco_bnf_engine::create_bnf_mask;
use roco_engine::ModelBackend;
use roco_engine::{
    cases::default_eval_suite, cases::fim_eval_cases, cases::message_eval_cases, print_report,
    run_suite, write_report, write_sidecars, CheckResult, CompletionRequest, EvalCase, EvalReport,
    MockBackend,
};
use roco_grammar::gbnf_to_kbnf;

/// Resolve the eval backend through the shared `AppContext` primitive.
///
/// `rwkv` and `remote` both go through `roco_app::daemon` (the same backend
/// resolution / daemon-lifecycle path as every other surface). The concrete
/// `RemoteBackend` is returned so the engine's `run_suite` / `run_one_streaming`
/// bounds (`B: ModelBackend + Send + Sync`) are satisfied.
fn eval_backend(backend: &str) -> roco_infer_client::RemoteBackend {
    match backend {
        "mock" => panic!("mock backend must use MockBackend directly"),
        "rwkv" => {
            // Ensure the daemon chain (gateway → inference) is up, then point
            // at the gateway. No RWKV_MODEL needed — AppContext resolves it.
            roco_app::daemon::ensure_sync_backend();
            roco_infer_client::RemoteBackend::new(format!(
                "http://127.0.0.1:{}",
                roco_app::daemon::GATEWAY_PORT
            ))
        }
        "remote" => {
            let url = env::var("ROCO_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
            roco_infer_client::RemoteBackend::new(url)
        }
        other => {
            eprintln!("Unknown backend: {other}");
            std::process::exit(1);
        }
    }
}

struct Args {
    backend: String,
    filter: Option<String>,
    /// Run a single case (by name substring) with live token streaming and an
    /// immediate verdict, then exit. Skips report/sidecar writes.
    one: Option<String>,
    /// Stream every case's tokens to stdout live during a full-suite run.
    live: bool,
    output: PathBuf,
    suite: String,
}

fn parse_args() -> Args {
    let args: Vec<String> = env::args().collect();
    let mut backend = "mock".to_string();
    let mut filter: Option<String> = None;
    let mut one: Option<String> = None;
    let mut live = false;
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
            "--one" => {
                i += 1;
                if i < args.len() {
                    one = Some(args[i].clone());
                }
            }
            "--live" | "-l" => {
                live = true;
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
                println!("  --backend STR    mock, rwkv, or remote [default: mock]");
                println!("  --filter STR     Filter eval cases (name/desc/category substring)");
                println!("  --one STR        Run ONE case (name substring) with live streaming, then exit");
                println!("  --live, -l       Stream every case's tokens to stdout live");
                println!("  --output PATH    Report path [default: evals/results/latest.json]");
                println!("  --suite STR      Suite name [default: roco-eval-suite]");
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }
    Args {
        backend,
        filter,
        one,
        live,
        output,
        suite,
    }
}

/// Build BNF masks for every eval case that carries a `grammar` string, using
/// the backend's vocabulary. This must happen in the application layer (not in
/// `roco-engine`): the inference backend cannot construct masks itself without
/// pulling grammar-engine types into the same compilation unit as `web-rwkv`
/// (`error[E0275]`). Cases without a grammar, or backends without a vocab
/// (e.g. `MockBackend`), are left unconstrained.
fn build_masks<B: ModelBackend + ?Sized>(backend: &B, cases: &mut [EvalCase]) {
    let Some(vocab) = backend.vocab_bytes() else {
        return;
    };
    for case in cases.iter_mut() {
        if let Some(g) = &case.grammar {
            let kbnf = gbnf_to_kbnf(g);
            if let Ok(mask) = create_bnf_mask(&kbnf, &vocab) {
                case.bnf_mask = Some(mask);
            }
        }
    }
}

/// Run a single case with **live token streaming** to the terminal and an
/// immediate pass/fail verdict. This is the “show me it working” mode used by
/// `--one <name>`: tokens appear in real time, then static checks light up
/// green/red.
///
/// We do NOT use `run_suite` here because the harness buffers results and
/// dumps them at the end — that's the wrong UX for a one-case diagnostic.
/// We also skip `build_masks` against the model vocab (it triggers a
/// `block_on` from inside the `#[tokio::main]` runtime when targeting the
/// `remote` backend, see `crates/infer-client/src/vocab_bytes` for the
/// threaded-runtime fix that removed this only in the conventional path).
/// Single-case mode is a diagnostic; unmasked generation is fine for a
/// visual demo.
async fn run_one_streaming<B: ModelBackend + Send + Sync>(
    backend: &B,
    mut cases: Vec<EvalCase>,
    name: &str,
) {
    let idx = cases.iter().position(|c| c.name.contains(name));
    let idx = match idx {
        Some(i) => i,
        None => {
            eprintln!("No case matches '{name}'. Available cases:");
            for c in &cases {
                eprintln!("  - {}", c.name);
            }
            std::process::exit(1);
        }
    };
    let case = cases.swap_remove(idx);

    let full_input = if case.system.is_empty() {
        format!("User: {}\n\nAssistant:", case.prompt)
    } else {
        format!(
            "System: {}\n\nUser: {}\n\nAssistant:",
            case.system, case.prompt
        )
    };

    println!("╭───── EVAL (live): {}", case.name);
    println!("│ category:    {}", case.category);
    println!("│ description: {}", case.description);
    println!("╰───────────────────────────────────────────────");
    println!("\n{}\n", full_input);
    println!("─── streaming output ────────────────────────────────────────────────");

    let on_token: Box<dyn Fn(&str) + Send + Sync> = Box::new(|word: &str| {
        use std::io::Write;
        let _ = std::io::stdout().write_all(word.as_bytes());
        let _ = std::io::stdout().flush();
    });

    let request = CompletionRequest {
        system: case.system.clone(),
        prompt: case.prompt.clone(),
        grammar: case.grammar.clone(),
        bnf_mask: case.bnf_mask,
        prefill: case.prefill.clone(),
        session: case.session.clone(),
        preserve_state: case.preserve_state,
        temperature: case.temperature,
        max_tokens: case.max_tokens,
        on_token: Some(on_token),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let response = backend.complete(request).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    println!("\n──────────────────────────────────────────────────────────");
    match response {
        Ok(resp) => {
            let output = &resp.text;
            let usage = &resp.usage;
            let tps = if usage.completion_tokens > 0 && latency_ms > 0 {
                (usage.completion_tokens as f64 / latency_ms as f64) * 1000.0
            } else {
                0.0
            };
            println!(
                "  latency: {}ms | {} tok | {:.1} tok/s",
                latency_ms, usage.completion_tokens, tps
            );

            // Re-run the same static checks the harness uses, live.
            let mut checks: Vec<CheckResult> = Vec::new();
            checks.push(CheckResult {
                name: "non_empty".into(),
                passed: !output.trim().is_empty(),
                detail: format!("{} chars", output.len()),
            });
            checks.push(CheckResult {
                name: "min_output_length".into(),
                passed: output.len() >= case.min_output_chars,
                detail: format!("{} >= {}", output.len(), case.min_output_chars),
            });
            for h in &case.expected_hints {
                let f = output.to_lowercase().contains(&h.to_lowercase());
                checks.push(CheckResult {
                    name: format!("hint: {h}"),
                    passed: f,
                    detail: if f {
                        "found".into()
                    } else {
                        "NOT found".into()
                    },
                });
            }
            for bad in &case.forbidden_strings {
                let f = output.contains(bad);
                checks.push(CheckResult {
                    name: format!("forbidden: {bad}"),
                    passed: !f,
                    detail: if f { "LEAKED".into() } else { "clean".into() },
                });
            }
            let passed = checks.iter().all(|c| c.passed);
            println!("  checks:");
            for c in &checks {
                let s = if c.passed { "✅" } else { "❌" };
                println!("    {} {} — {}", s, c.name, c.detail);
            }
            println!(
                "\n  RESULT: {}\n",
                if passed { "✅ PASS" } else { "❌ FAIL" }
            );
        }
        Err(e) => {
            println!("  ERROR: {e}");
            println!("\n  RESULT: ❌ ERROR\n");
            std::process::exit(1);
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = parse_args();
    let trace_path = {
        let stem = args
            .output
            .file_stem()
            .map(|s| format!("{}_trace.txt", s.to_string_lossy()))
            .unwrap_or_else(|| "eval_trace.txt".to_string());
        Some(args.output.with_file_name(stem))
    };

    // `--one` takes precedence: run exactly one case, stream it live, exit.
    // No report / sidecar / timestamped trace file is emitted; the verdict
    // is the only output. See `run_one_streaming` for the rationale.
    if let Some(one) = &args.one {
        match args.backend.as_str() {
            "mock" => {
                let backend = MockBackend::new("mock-3b", 0);
                run_one_streaming(&backend, default_eval_suite(), one).await;
            }
            "rwkv" | "remote" => {
                let backend = eval_backend(&args.backend);
                let mut cases = default_eval_suite();
                cases.extend(roco_engine::cases::message_eval_cases());
                cases.extend(roco_engine::cases::fim_eval_cases());
                // The FIM bridge cases resume from a named recurrent-state
                // session (`roco_fim`) that was primed by bake_fim_session
                // with 3 few-shot BEFORE/AFTER pairs. If we skip the bake
                // here, the session is a *blank slate* and the prior model
                // default for "<FIM scaffold>" is to echo the prompt as
                // its own output — generating "BEFORE/AFTER/INSERT: ...
                // INSERT" instead of a bridge — which the actor's
                // substring-break on "INSERT" immediately truncates. So
                // bake before --one too when the requested case is a FIM
                // bridge.
                let is_fim_bridge = cases.iter().any(|c| {
                    c.name.contains(one) && c.session.as_deref() == Some(roco_engine::FIM_SESSION)
                });
                if is_fim_bridge {
                    if let Err(e) = roco_engine::bake_fim_session(&backend).await {
                        eprintln!("WARN: FIM session bake failed: {e}");
                    }
                }
                run_one_streaming(&backend, cases, one).await;
            }
            other => {
                eprintln!("Unknown backend: {other}");
                std::process::exit(1);
            }
        }
        return;
    }

    let report: EvalReport = match args.backend.as_str() {
        "mock" => {
            let backend = eval_backend("mock");
            run_suite(
                &args.suite,
                &backend,
                default_eval_suite(),
                args.filter.as_deref(),
                trace_path.as_deref(),
                args.live,
            )
            .await
        }
        "rwkv" | "remote" => {
            let backend = eval_backend(&args.backend);
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
            run_suite(
                &args.suite,
                &backend,
                cases,
                args.filter.as_deref(),
                trace_path.as_deref(),
                args.live,
            )
            .await
        }
        other => {
            eprintln!("Unknown backend: {other}");
            std::process::exit(1);
        }
    };

    write_report(&args.output, &report).ok();
    print_report(&report);
    println!("Report written to: {}", args.output.display());
    if let Some(ref trace_path) = trace_path {
        println!("Trace written to:  {}", trace_path.display());
        write_sidecars(&report, trace_path);
    }
    if report.failed > 0 {
        std::process::exit(1);
    }
}
