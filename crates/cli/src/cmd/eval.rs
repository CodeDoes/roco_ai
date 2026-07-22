//! Eval/bless subcommands: `roco eval` and `roco bless`.

use std::path::{Path, PathBuf};

use crate::parse_opt;

pub fn cmd_eval(extra: &[&str]) {
    let output = parse_opt("--output", extra).unwrap_or("evals/results/latest.json");
    let exit_code = crate::run_cargo_get_code(
        "run",
        &[
            "-p",
            "roco-inference",
            "--example",
            "rwkv_test",
            "--release",
            "--",
            "--backend",
            "rwkv",
        ],
        extra,
    );

    let snapshot_path = snapshot_path(output);
    if let Ok(report) = std::fs::read_to_string(output) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&report) {
            if let Some(results) = parsed["results"].as_array() {
                let mut snap = serde_json::Map::new();
                for r in results {
                    let name = r["name"].as_str().unwrap_or("");
                    let out = r["output"].as_str().unwrap_or("").trim();
                    if !name.is_empty() {
                        snap.insert(
                            name.to_string(),
                            serde_json::Value::String(out.to_string()),
                        );
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

pub fn cmd_bless(extra: &[&str]) {
    let snapshot = parse_opt("--snapshot", extra)
        .map(PathBuf::from)
        .unwrap_or_else(|| snapshot_path("evals/results/latest.json"));

    let snap: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&snapshot)
            .expect("snapshot file not found — run `roco eval` first"),
    )
    .expect("invalid snapshot JSON");
    let obj = snap.as_object().expect("snapshot must be a JSON object");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let source_candidates = [
        PathBuf::from(&manifest_dir).join("src/engine/eval.rs"),
        PathBuf::from(&manifest_dir).join("crates/engine/src/eval.rs"),
        PathBuf::from(&manifest_dir).join("src/engine/cases.rs"),
        PathBuf::from(&manifest_dir).join("crates/engine/src/cases.rs"),
    ];
    let source_paths: Vec<PathBuf> = source_candidates
        .iter()
        .filter(|p| p.exists())
        .cloned()
        .collect();

    if source_paths.is_empty() {
        eprintln!("eval source files not found");
        return;
    }

    let mut total_changed = 0;
    for source_path in &source_paths {
        let content = std::fs::read_to_string(source_path).expect("source not found");
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut changed = 0;

        for (name, out_val) in obj.iter() {
            let out_str = out_val.as_str().unwrap_or("");
            if let Some(name_line) = lines.iter().position(|l| {
                l.trim() == &format!("name: \"{}\".into(),", name)
            }) {
                let mut oracle_line = None;
                for i in name_line..lines.len() {
                    let trimmed = lines[i].trim();
                    if trimmed.starts_with("oracle: Some(")
                        || trimmed.starts_with("oracle: None,")
                    {
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
                    let escaped = out_str
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n");
                    let indent =
                        &lines[oi][..lines[oi].len() - lines[oi].trim_start().len()];
                    lines[oi] = format!("{indent}oracle: Some(\"{escaped}\".into()),");
                    changed += 1;
                    eprintln!("  blessed {name}: \"{escaped}\"");
                } else {
                    eprintln!("  skipping {name}: no oracle field found");
                }
            } else {
                eprintln!("  skipping {name}: eval case not found");
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

fn snapshot_path(output: &str) -> PathBuf {
    let p = Path::new(output);
    let mut s = p.to_path_buf();
    s.set_extension("snapshot.json");
    s
}
