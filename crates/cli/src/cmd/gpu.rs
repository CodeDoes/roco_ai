//! GPU check subcommand: `roco gpu-check`.

use std::process::Command;

pub fn cmd_gpu_check(extra: &[&str]) {
    let json_mode = extra.iter().any(|&a| a == "--json" || a == "-j");
    let model_path = "models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st";
    let vocab_path = "assets/vocab/rwkv_vocab_v20230424.json";

    let vulkan_ok = Command::new("vulkaninfo")
        .arg("--summary")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let model_exists = std::path::Path::new(model_path).exists();
    let vocab_exists = std::path::Path::new(vocab_path).exists();

    if json_mode {
        let info = serde_json::json!({
            "vulkan": { "available": vulkan_ok },
            "model": { "path": model_path, "exists": model_exists },
            "vocab": { "path": vocab_path, "exists": vocab_exists },
        });
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        println!("=== Vulkan devices ===");
        let _ = Command::new("vulkaninfo").arg("--summary").status();
        if !vulkan_ok {
            eprintln!("(vulkaninfo not available — GPU check may be limited)");
        }
        println!();
        println!("=== RWKV model ===");
        if model_exists {
            let _ = Command::new("ls").args(["-lh", model_path]).status();
        } else {
            eprintln!("Model not found at {model_path}");
        }
        println!("=== RWKV vocab ===");
        if vocab_exists {
            let _ = Command::new("ls").args(["-lh", vocab_path]).status();
        } else {
            eprintln!("Vocab not found at {vocab_path}");
        }
    }
}
