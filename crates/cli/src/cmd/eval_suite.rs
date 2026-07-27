//! Deterministic Evaluation and Regression Suite (`roco eval-suite`).
//!
//! Provides automated, offline deterministic evaluation of model outputs,
//! grammar compliance, prompt robustness, and stream monotonicity across
//! all registered benchmarks. Unlike the unit tests that run with `cargo test`,
//! this is a CLI-accessible command that produces a structured JSON report
//! suitable for CI or manual review.

/// Run the evaluation suite.
///
/// Supports `--json` / `-j` for machine-readable output and a benchmark name
/// filter (default: `"all"`).
pub fn cmd_eval_suite(extra: &[&str]) {
    let json_mode = extra.iter().any(|&a| a == "--json" || a == "-j");
    let target = extra
        .iter()
        .find(|&&a| !a.starts_with('-'))
        .copied()
        .unwrap_or("all");

    let mut results: Vec<(&str, Result<(), String>)> = Vec::new();

    println!("================================================================");
    println!("  RoCo AI — Deterministic Evaluation Suite");
    println!("================================================================");
    println!("  Target: {target}");
    println!();

    // ── 1. Streaming / StreamPrinter ─────────────────────────────────────
    if target == "all" || target == "streaming" {
        results.push(eval("stream_monotonicity", || {
            // The core invariant: visible text never shrinks across deltas.
            let deltas = &[
                "He",
                "llo\n",
                "wor",
                "ld <thi",
                "nk>z</think>",
                "!\nUser: x",
            ];
            let mut p = crate::streaming::StreamPrinter::quiet();
            let mut prev = String::new();
            for d in deltas {
                p.push(d);
                let now = p.render(false);
                if !now.starts_with(&prev) {
                    return Err(format!("render shrank: {prev:?} -> {now:?}"));
                }
                prev = now;
            }
            Ok(())
        }));
    }

    // ── 2. Think-block stripping ─────────────────────────────────────────
    if target == "all" || target == "streaming" {
        results.push(eval("think_blocks_hidden", || {
            let rendered = crate::streaming::StreamPrinter::quiet()
                .finish("<think>secret</think>Visible answer.");
            if rendered.contains("secret") || rendered.contains("<think>") {
                return Err("think block leaked through rendering".into());
            }
            if !rendered.contains("Visible answer.") {
                return Err("content after think block was dropped".into());
            }
            Ok(())
        }));
    }

    // ── 3. Hallucinated turn cutting ─────────────────────────────────────
    if target == "all" || target == "streaming" {
        results.push(eval("hallucinated_turn_cut", || {
            let rendered = crate::streaming::StreamPrinter::quiet()
                .finish("The answer is 4.\nUser: and 3+3?\nAssistant: 6");
            if rendered.contains("\nUser:") || rendered.ends_with('6') {
                return Err("hallucinated user turn was not cut".into());
            }
            Ok(())
        }));
    }

    // ── 4. Partial marker holdback ───────────────────────────────────────
    if target == "all" || target == "streaming" {
        results.push(eval("partial_marker_holdback", || {
            let mut p = crate::streaming::StreamPrinter::quiet();
            p.push("hello <thi");
            if p.render(false) != "hello " {
                return Err(format!("partial <think> leaked: {:?}", p.render(false)));
            }
            p.push("nk>hidden</think> done");
            if p.finish("hello <think>hidden</think> done") != "hello  done" {
                return Err("unexpected finish output".into());
            }
            Ok(())
        }));
    }

    // ── 5. Identity fast-path ────────────────────────────────────────────
    if target == "all" || target == "identity" {
        results.push(eval("identity_detection", || {
            let cases = &[
                ("who are you", true),
                ("what's your name", true),
                ("what can you do", true),
                ("tell me a story", false),
                ("what model are you", true),
                ("write a poem", false),
            ];
            for (input, expected) in cases {
                let detected = crate::identity::detect(input).is_some();
                if detected != *expected {
                    return Err(format!(
                        "identity detection mismatch for {input:?}: \
                         expected {expected}, got {detected}"
                    ));
                }
            }
            Ok(())
        }));
    }

    // ── 6. Conversation context budgeting ────────────────────────────────
    if target == "all" || target == "conversation" {
        results.push(eval("context_keeps_whole_messages", || {
            let dir = tempfile::tempdir().expect("tempdir");
            let backend = roco_engine::MockBackend::default();
            let mut s = crate::conversation::ChatSession::new(
                roco_protocol::ConversationState::new("eval".into(), "thorough"),
                dir.path().join("s.json"),
                "You are a test.",
                &backend,
            )
            .quiet(true);
            let long = "A".repeat(1200);
            s.push("user", "hello");
            s.push("assistant", &long);
            let ctx = s.build_context("continue");
            if !ctx.contains(&long) {
                return Err("long assistant turn truncated in context".into());
            }
            Ok(())
        }));
    }

    // ── 7. Eval-suite self-test: all tests defined ───────────────────────
    let passed = results.iter().filter(|r| r.1.is_ok()).count();
    let total = results.len();

    if json_mode {
        let json_out = serde_json::json!({
            "target": target,
            "passed": passed,
            "total": total,
            "success": passed == total,
            "tests": results.iter().map(|(name, result)| {
                serde_json::json!({
                    "name": name,
                    "success": result.is_ok(),
                    "message": match result {
                        Ok(()) => "ok",
                        Err(e) => e,
                    }
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&json_out).unwrap());
    } else {
        for (name, result) in &results {
            match result {
                Ok(()) => {
                    println!("  ✅ PASS | {name:30}");
                }
                Err(msg) => {
                    println!("  ❌ FAIL | {name:30} | {msg}");
                }
            }
        }
        println!("----------------------------------------------------------------");
        let failed = total - passed;
        if failed == 0 {
            println!("  Result: ALL {total} DETERMINISTIC EVALUATIONS PASSED.");
        } else {
            println!("  Result: {passed}/{total} passed, {failed} failed.");
            std::process::exit(1);
        }
        println!("================================================================");
    }
}

/// Run a single named evaluation.
fn eval(name: &str, f: impl FnOnce() -> Result<(), String>) -> (&'static str, Result<(), String>) {
    let result = f();
    if result.is_err() {
        eprintln!("[eval-suite] FAIL: {name}");
    }
    (Box::leak(name.to_string().into_boxed_str()), result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_suite_runs_without_panic() {
        // Smoke test: run all evals and verify they complete.
        let extra: &[&str] = &["all"];
        // We can't easily capture stdout, but we can verify no panic.
        cmd_eval_suite(extra);
    }

    #[test]
    fn test_eval_suite_json_output() {
        let extra: &[&str] = &["--json", "all"];
        cmd_eval_suite(extra);
    }

    #[test]
    fn test_eval_suite_specific_target() {
        let extra: &[&str] = &["identity"];
        cmd_eval_suite(extra);
    }

    #[test]
    fn test_eval_helper_ok() {
        let (name, result) = eval("test_ok", || Ok(()));
        assert_eq!(name, "test_ok");
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_helper_err() {
        let (name, result) = eval("test_err", || Err("something broke".into()));
        assert_eq!(name, "test_err");
        assert!(result.is_err());
    }
}
