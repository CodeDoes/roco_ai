//! `roco jobs` — Inspect active jobs, status, and health of the inference daemon (`roco-inferd`).

use roco_protocol::InferJobsResponse;
use std::env;

pub fn cmd_jobs(extra: &[&str]) {
    let port = extra
        .iter()
        .position(|&a| a == "--port")
        .and_then(|i| extra.get(i + 1))
        .and_then(|p| p.parse::<u16>().ok())
        .or_else(|| env::var("RWKV_PORT").ok().and_then(|p| p.parse().ok()))
        .unwrap_or(8080);

    let host = env::var("RWKV_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let url = format!("http://{host}:{port}/jobs");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            eprintln!("⚠ HTTP client error: {e}");
            std::process::exit(1);
        }
    };

    match client.get(&url).send() {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(info) = resp.json::<InferJobsResponse>() {
                println!("\n  \x1b[1mRoCo Inference Daemon (roco-inferd) Status\x1b[0m");
                println!("  ──────────────────────────────────────────────");
                println!("  Status:       \x1b[32m{}\x1b[0m", info.status);
                println!("  Backend:      {}", info.backend);
                println!("  Active Jobs:  \x1b[1m{}\x1b[0m", info.active_jobs);
                println!("  Uptime:       {}s", info.uptime_secs);
                println!("  Features:     {}", info.features.join(", "));
                println!("  Endpoint:     http://{host}:{port}\n");
                return;
            }
        }
        _ => {}
    }

    // Fallback to /health for legacy/running daemon instances
    let health_url = format!("http://{host}:{port}/health");
    if let Ok(resp) = client.get(&health_url).send() {
        if resp.status().is_success() {
            if let Ok(health) = resp.json::<roco_protocol::HealthResponse>() {
                println!("\n  \x1b[1mRoCo Inference Daemon (roco-inferd) Status\x1b[0m");
                println!("  ──────────────────────────────────────────────");
                println!("  Status:       \x1b[32m{}\x1b[0m", health.status);
                println!("  Backend:      {}", health.backend);
                println!("  Active Jobs:  0 (legacy daemon)");
                println!("  Endpoint:     http://{host}:{port}\n");
                return;
            }
        }
    }

    eprintln!("  ⚠ Could not connect to roco-inferd at {url}");
    eprintln!(
        "    Is roco-inferd running? Start it with: ./run_desktop.sh or cargo run -p roco-inferd\n"
    );
}
