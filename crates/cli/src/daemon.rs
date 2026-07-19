//! Daemon lifecycle — auto-start gateway and inference server as needed.
//!
//! `roco gui` → auto-starts Gateway (if not running)
//! Gateway → auto-starts Inference Server (if not running)
//! Both check PID files and health endpoints to detect running instances.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Default ports
pub const GATEWAY_PORT: u16 = 8000;
pub const INFERENCE_PORT: u16 = 8080;
pub const GATEWAY_TARGET: &str = "http://127.0.0.1:8080";

// ═════════════════════════════════════════════════════════════════════════════
// PID file management
// ═════════════════════════════════════════════════════════════════════════════

fn pid_dir() -> PathBuf {
    std::env::temp_dir().join("roco")
}

fn pid_path(name: &str) -> PathBuf {
    pid_dir().join(format!("{}.pid", name))
}

fn log_path(name: &str, port: u16) -> PathBuf {
    pid_dir().join(format!("{}_{}.log", name, port))
}

/// Check if a daemon is running via health endpoint
fn is_running(name: &str, port: u16) -> bool {
    let pid_file = pid_path(name);
    if !pid_file.exists() {
        return false;
    }

    // Verify health endpoint
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return false,
    };
    let healthy = rt.block_on(async {
        let url = format!("http://127.0.0.1:{}/health", port);
        match reqwest::get(&url).await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    });
    if !healthy {
        let _ = std::fs::remove_file(&pid_file);
        return false;
    }
    true
}

/// Ensure a daemon is running. Spawns it if not.
/// Returns true if it was already running, false if we spawned it.
pub fn ensure_daemon(exe: &PathBuf, subcmd: &str, port: u16, extra_args: &[&str]) -> bool {
    if is_running(subcmd, port) {
        return true; // Already running
    }

    // Clean up stale PID file if any
    let _ = std::fs::remove_file(pid_path(subcmd));

    let log_file_path = log_path(subcmd, port);
    let pid_file_path = pid_path(subcmd);

    // Build args
    let mut args = vec![subcmd.to_string()];
    args.extend(extra_args.iter().map(|s| s.to_string()));
    args.push(format!("--port={}", port));

    // stdout/stderr → log file
    let log_file = match std::fs::File::create(&log_file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Warning: failed to create log file {}: {e}",
                log_file_path.display()
            );
            return false;
        }
    };
    let log_clone = match log_file.try_clone() {
        Ok(c) => c,
        Err(_) => return false,
    };

    match Command::new(exe)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_clone)
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            if let Err(e) = std::fs::write(&pid_file_path, pid.to_string()) {
                eprintln!("Warning: failed to write PID file: {e}");
            }
            eprintln!(
                "Started {subcmd} (PID {pid}, log: {})",
                log_file_path.display()
            );
            false
        }
        Err(e) => {
            eprintln!("Warning: failed to spawn {subcmd}: {e}");
            false
        }
    }
}

/// Wait for a daemon to become healthy
pub async fn wait_for_healthy(port: u16, timeout: Duration, label: &str) -> Result<(), String> {
    let start = std::time::Instant::now();
    let url = format!("http://127.0.0.1:{}/health", port);

    while start.elapsed() < timeout {
        match reqwest::get(&url).await {
            Ok(resp) if resp.status().is_success() => {
                return Ok(());
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    Err(format!(
        "{} did not become healthy within {:.0}s",
        label,
        timeout.as_secs_f64()
    ))
}

// ═════════════════════════════════════════════════════════════════════════════
// Gateway daemon — auto-starts inference server on startup
// ═════════════════════════════════════════════════════════════════════════════

/// Entry point for the gateway when spawned as a daemon.
/// Ensures the inference server is running before starting.
pub fn run_gateway_with_auto_inference(host: &str, port: u16, target: &str, rate_limit: usize) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    // Ensure inference server is running
    ensure_daemon(&exe, "server", INFERENCE_PORT, &["--detach"]);

    // Build args for the gateway (without --detach, as we're already the child)
    let args = vec![
        format!("--host={}", host),
        format!("--port={}", port),
        format!("--target={}", target),
        format!("--rate-limit={}", rate_limit),
    ];

    let log_path = log_path("gateway", port);
    let pid_path = pid_path("gateway");

    // Redirect stdio
    let log_file = std::fs::File::create(&log_path)
        .unwrap_or_else(|e| panic!("failed to create log file {}: {e}", log_path.display()));
    let log_clone = log_file
        .try_clone()
        .expect("failed to clone log file handle");

    let mut cmd = Command::new(&exe);
    cmd.args(&["gateway"]);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(log_file);
    cmd.stderr(log_clone);

    // Write PID
    if let Ok(child) = cmd.spawn() {
        let pid = child.id();
        std::fs::write(&pid_path, pid.to_string()).ok();
        eprintln!("Gateway started (PID {pid})");
        // Wait for it to finish (child process)
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_paths() {
        let p = pid_path("gateway");
        assert!(p.to_string_lossy().contains("gateway.pid"));
    }

    #[test]
    fn test_log_paths() {
        let p = log_path("server", 8080);
        assert!(p.to_string_lossy().contains("server_8080.log"));
    }

    #[test]
    fn test_constants() {
        assert_eq!(GATEWAY_PORT, 8000);
        assert_eq!(INFERENCE_PORT, 8080);
        assert_eq!(GATEWAY_TARGET, "http://127.0.0.1:8080");
    }
}
