//! Model state inspection subcommand: `roco inspect`.
//!
//! Provides interpretability into the model's internal state, session cache,
//! and generation parameters. Useful for debugging determinism, understanding
//! why the model produced a specific output, and verifying state management.

use std::path::Path;

/// Run the inspect command.
///
/// Usage: `roco inspect [--json] [session | state | config]`
pub fn cmd_inspect(extra: &[&str]) {
    let json_mode = extra.iter().any(|&a| a == "--json" || a == "-j");
    let target = extra
        .iter()
        .find(|&&a| !a.starts_with('-'))
        .copied()
        .unwrap_or("all");

    let workspace_dir = Path::new(".roco");

    match target {
        "caches" | "cache" => {
            inspect_caches(workspace_dir, json_mode);
        }
        "sessions" | "session" => {
            inspect_sessions(workspace_dir, json_mode);
        }
        "config" => {
            inspect_config(json_mode);
        }
        "model" => {
            inspect_model(json_mode);
        }
        "seed" | "determinism" => {
            inspect_seed_info();
        }
        _ => {
            if !json_mode {
                println!("================================================================");
                println!("  RoCo AI — Model & System Inspection");
                println!("================================================================");
            }
            inspect_caches(workspace_dir, json_mode);
            if !json_mode {
                println!("----------------------------------------------------------------");
            }
            inspect_sessions(workspace_dir, json_mode);
            if !json_mode {
                println!("----------------------------------------------------------------");
            }
            inspect_config(json_mode);
            if !json_mode {
                println!("----------------------------------------------------------------");
            }
            inspect_seed_info();
            if !json_mode {
                println!("================================================================");
            }
        }
    }
}

fn inspect_caches(workspace_dir: &Path, json_mode: bool) {
    let session_dir = workspace_dir.join("sessions");
    // Use $HOME/.cache/roco as the cache directory
    let cache_dir = std::env::var("HOME")
        .map(|h| Path::new(&h).join(".cache").join("roco"))
        .unwrap_or_else(|_| Path::new("/tmp/roco-cache").to_path_buf());

    let mut info = serde_json::json!({
        "workspace_sessions": if session_dir.exists() {
            count_files(&session_dir)
        } else {
            0
        },
        "cache_directory": cache_dir.to_string_lossy(),
        "cache_exists": cache_dir.exists(),
    });

    // Check quant cache
    if let Ok(home) = std::env::var("HOME") {
        let quant_cache = Path::new(&home)
            .join(".cache")
            .join("roco")
            .join("quant-cache");
        info["quant_cache_exists"] = serde_json::json!(quant_cache.exists());
        if quant_cache.exists() {
            if let Ok(entries) = std::fs::read_dir(&quant_cache) {
                info["quant_cache_entries"] = serde_json::json!(entries.count());
            }
        }
        let pipeline_cache = Path::new(&home)
            .join(".cache")
            .join("roco")
            .join("pipeline-cache");
        info["pipeline_cache_exists"] = serde_json::json!(pipeline_cache.exists());
        if pipeline_cache.exists() {
            if let Ok(entries) = std::fs::read_dir(&pipeline_cache) {
                info["pipeline_cache_entries"] = serde_json::json!(entries.count());
            }
        }
    }

    if json_mode {
        let json_out = serde_json::json!({ "caches": info });
        println!("{}", serde_json::to_string_pretty(&json_out).unwrap());
    } else {
        println!("  Cache & Storage:");
        println!(
            "    Workspace sessions:    {} files",
            info["workspace_sessions"]
        );
        println!("    Quant cache dir:       {}", info["quant_cache_exists"]);
        if let Some(n) = info.get("quant_cache_entries").and_then(|v| v.as_u64()) {
            println!("    Quant cache entries:   {n}");
        }
        println!(
            "    Pipeline cache dir:    {}",
            info["pipeline_cache_exists"]
        );
        if let Some(n) = info.get("pipeline_cache_entries").and_then(|v| v.as_u64()) {
            println!("    Pipeline cache entries: {n}");
        }
    }
}

fn inspect_sessions(workspace_dir: &Path, json_mode: bool) {
    let session_dir = workspace_dir.join("sessions");
    let mut sessions = Vec::new();

    if session_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&session_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    sessions.push(serde_json::json!({
                        "name": name,
                        "size_bytes": size,
                        "modified": std::fs::metadata(&path)
                            .and_then(|m| m.modified())
                            .ok()
                            .map(|t| {
                                let d: std::time::SystemTime = t;
                                let since_epoch = d.duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default();
                                since_epoch.as_secs()
                            }),
                    }));
                }
            }
        }
    }

    sessions.sort_by(|a, b| {
        let a_name = a["name"].as_str().unwrap_or("");
        let b_name = b["name"].as_str().unwrap_or("");
        a_name.cmp(b_name)
    });

    if json_mode {
        let json_out = serde_json::json!({ "sessions": sessions });
        println!("{}", serde_json::to_string_pretty(&json_out).unwrap());
    } else {
        println!("  Sessions ({} total):", sessions.len());
        if sessions.is_empty() {
            println!("    (no saved sessions found)");
        } else {
            for s in &sessions {
                let name = s["name"].as_str().unwrap_or("?");
                let size = s["size_bytes"].as_u64().unwrap_or(0);
                let modified = s["modified"]
                    .as_u64()
                    .map(|ts| {
                        let d = std::time::Duration::from_secs(ts);
                        let st = std::time::UNIX_EPOCH + d;
                        let dur = st
                            .duration_since(std::time::SystemTime::now())
                            .unwrap_or_default();
                        format!("{}s ago", dur.as_secs())
                    })
                    .unwrap_or_else(|| "unknown".into());
                println!("    {name:25} {size:>8}B  {modified}");
            }
        }
    }
}

fn inspect_model(_json_mode: bool) {
    // Model information — discover the model file from env or default locations.
    let model_path = std::env::var("RWKV_MODEL").ok();
    match model_path {
        Some(path) => {
            let size = std::fs::metadata(&path)
                .map(|m| format!("{:.1} MB", m.len() as f64 / (1024.0 * 1024.0)))
                .unwrap_or_else(|_| "unknown".into());
            println!("  Model:");
            println!("    Path:     {}", path);
            println!("    Size:     {}", size);
            println!("    To inspect live state, start a session and use `roco interact`.");
        }
        None => {
            // Scan default locations
            for dir in &["models", "../models"] {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                            if name.ends_with(".st") {
                                let size = std::fs::metadata(&p)
                                    .map(|m| {
                                        format!("{:.1} MB", m.len() as f64 / (1024.0 * 1024.0))
                                    })
                                    .unwrap_or_else(|_| "unknown".into());
                                println!("  Model:   {} ({})", p.display(), size);
                                return;
                            }
                        }
                    }
                }
            }
            println!("  Model:   (no .st model file found in models/)");
        }
    }
}

fn inspect_config(json_mode: bool) {
    let mut info = serde_json::json!({
        "rwkv_model": std::env::var("RWKV_MODEL").unwrap_or_else(|_| "(not set, auto-detect)".into()),
        "rwkv_vocab": std::env::var("RWKV_VOCAB").unwrap_or_else(|_| "(not set, default)".into()),
        "rwkv_quant": std::env::var("RWKV_QUANT").unwrap_or_else(|_| "(not set, auto)".into()),
        "rwkv_cache_dir": std::env::var("RWKV_CACHE_DIR").unwrap_or_else(|_| "(not set, default ~/.cache/roco)".into()),
        "rwkv_deadline_ms": std::env::var("RWKV_DEADLINE_MS").unwrap_or_else(|_| "(not set, no deadline)".into()),
        "rwkv_backend_timeout": std::env::var("RWKV_BACKEND_TIMEOUT").unwrap_or_else(|_| "(not set, default 120s)".into()),
        "rwkv_adapter": std::env::var("RWKV_ADAPTER").unwrap_or_else(|_| "(not set, auto)".into()),
        "rwkv_deterministic_seed": std::env::var("RWKV_DETERMINISTIC_SEED").unwrap_or_else(|_| "(not set)".into()),
        "roco_config": std::env::var("ROCO_CONFIG").unwrap_or_else(|_| "(not set)".into()),
        "profile_path": identity_default_profile_path(),
    });

    // Detect backend availability
    let has_backend = std::env::var("ROCO_USE_MOCK_BACKEND").is_ok();
    info["mock_backend"] = serde_json::json!(has_backend);

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!("  Configuration:");
        println!("    RWKV_MODEL (env):        {}", info["rwkv_model"]);
        println!("    RWKV_VOCAB (env):        {}", info["rwkv_vocab"]);
        println!("    RWKV_QUANT (env):        {}", info["rwkv_quant"]);
        println!("    RWKV_CACHE_DIR (env):    {}", info["rwkv_cache_dir"]);
        println!("    RWKV_DEADLINE_MS (env):  {}", info["rwkv_deadline_ms"]);
        println!(
            "    RWKV_BACKEND_TIMEOUT:    {}",
            info["rwkv_backend_timeout"]
        );
        println!("    RWKV_ADAPTER (env):      {}", info["rwkv_adapter"]);
        println!("    ROCO_CONFIG (env):       {}", info["roco_config"]);
        println!("    Profile path:            {}", info["profile_path"]);
        println!("    Mock backend:            {}", info["mock_backend"]);
        let seed_info = info["rwkv_deterministic_seed"].as_str().unwrap_or("");
        println!("    RWKV_DETERMINISTIC_SEED: {}", seed_info);
        if seed_info != "(not set)" && !seed_info.is_empty() {
            println!("      → All generations will be reproducible with this seed.");
            println!("      → Override per-request by setting `\"seed\"` in the request JSON.");
        }
    }
}

fn inspect_seed_info() {
    // Show seed/determinism information
    println!();
    println!("  Seed & Determinism Info:");
    println!("    Set `\"seed\": <u64>` in your completion request to get");
    println!("    deterministic, reproducible outputs from the model.");
    println!("    Same seed + same prompt + same temperature = same output.");
    println!();
    println!("    Example: --data '{{\"prompt\":\"hello\",\"seed\":42,\"temperature\":0.8}}'");
    println!("    Environment: RWKV_DETERMINISTIC_SEED (optional)");
}

/// Get the default profile path (same as identity::UserProfile::default_path).
fn identity_default_profile_path() -> String {
    if let Ok(home) = std::env::var("HOME") {
        let path = Path::new(&home)
            .join(".config")
            .join("roco")
            .join("profile.json");
        path.to_string_lossy().to_string()
    } else {
        ".roco/profile.json".to_string()
    }
}

fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspect_runs_without_panic() {
        // Smoke test: verify the function doesn't panic with default args
        cmd_inspect(&["all"]);
    }

    #[test]
    fn test_inspect_json_output() {
        cmd_inspect(&["--json", "all"]);
    }

    #[test]
    fn test_inspect_sessions_only() {
        cmd_inspect(&["sessions"]);
    }

    #[test]
    fn test_inspect_config_only() {
        cmd_inspect(&["config"]);
    }

    #[test]
    fn test_count_files_on_nonexistent_dir() {
        let p = Path::new("/tmp/roco-inspect-test-nonexistent-XXXXXX");
        assert_eq!(count_files(p), 0);
    }
}
