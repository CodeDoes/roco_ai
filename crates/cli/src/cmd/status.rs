use std::fs;
use std::path::PathBuf;

pub fn cmd_status(_extra: &[&str]) {
    println!("=== RoCo Status ===");

    // 1. Current mode
    let mut mode = "Standalone CLI";
    let inferd_running = crate::daemon::is_running("inferd", crate::daemon::INFERENCE_PORT);
    let gateway_running = crate::daemon::is_running("gateway", crate::daemon::GATEWAY_PORT);

    if inferd_running && gateway_running {
        mode = "Local Server & Gateway";
    } else if inferd_running {
        mode = "Local Inference Server";
    } else if gateway_running {
        mode = "Gateway Only";
    }
    println!("Current Mode: {}", mode);
    println!();

    // 2. Recent stories in .roco/stories/
    println!("--- Stories ---");
    let stories_dir = PathBuf::from(".roco/stories");
    if stories_dir.exists() {
        if let Ok(entries) = fs::read_dir(&stories_dir) {
            let mut stories = Vec::new();
            for entry in entries.filter_map(Result::ok) {
                if let Ok(metadata) = entry.metadata() {
                    stories.push((
                        entry.file_name().to_string_lossy().to_string(),
                        metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    ));
                }
            }
            stories.sort_by_key(|b| std::cmp::Reverse(b.1));
            println!("Total stories: {}", stories.len());
            for (name, _) in stories.iter().take(5) {
                println!("  - {}", name);
            }
        }
    } else {
        println!("No stories found.");
    }
    println!();

    // 3. Workspace count in .roco/workspaces/
    println!("--- Workspaces ---");
    let workspaces_dir = PathBuf::from(".roco/workspaces");
    if workspaces_dir.exists() {
        if let Ok(entries) = fs::read_dir(&workspaces_dir) {
            let count = entries.filter_map(Result::ok).count();
            println!("Total workspaces: {}", count);
        }
    } else {
        println!("Total workspaces: 0");
    }
    println!();

    // 4. Test results from last CI run
    println!("--- Last Eval Run ---");
    let eval_file = PathBuf::from("evals/results/latest.json");
    if eval_file.exists() {
        if let Ok(content) = fs::read_to_string(&eval_file) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let passed = json.get("passed").and_then(|v| v.as_u64()).unwrap_or(0);
                let failed = json.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = json
                    .get("total")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(passed + failed);

                println!("Passed: {}", passed);
                println!("Failed: {}", failed);
                println!("Total:  {}", total);
            } else {
                println!("Could not parse test results.");
            }
        }
    } else {
        println!("No recent test results found.");
    }
}
