//! Daemon lifecycle — auto-start gateway and inference server as needed.
//!
//! `roco gui` → auto-starts Gateway (if not running)
//! Gateway → auto-starts Inference Server (if not running)
//! All CLI commands use `ensure_backend()` instead of loading models directly.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
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


/// Default path under the system temp dir for PID/log files.
pub fn default_detach_path(subcmd: &str, port: u16, ext: &str) -> PathBuf {
    let dir = pid_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{subcmd}_{port}.{ext}"))
}

/// Spawn a detached child process for `roco server` / `roco gateway`.
/// Parent redirects stdio to a log file, writes a PID file, and returns.
pub fn spawn_detached(subcmd: &str, extra: &[&str], log_path: &PathBuf, pid_path: &PathBuf) {
    let exe = std::env::current_exe().expect("failed to get current exe path");
    let mut child_args: Vec<String> = Vec::new();
    child_args.push(subcmd.to_string());
    for a in extra {
        if *a == "--detach" || *a == "-d" {
            continue;
        }
        child_args.push((*a).to_string());
    }
    // Marker so the child does not re-detach.
    child_args.push(format!("--_child-{subcmd}"));

    let log_file = std::fs::File::create(log_path)
        .unwrap_or_else(|e| panic!("failed to create log {}: {e}", log_path.display()));
    let log_clone = log_file
        .try_clone()
        .unwrap_or_else(|e| panic!("failed to clone log handle: {e}"));

    let child = Command::new(&exe)
        .args(&child_args)
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_clone)
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn child: {e}"));

    let pid = child.id();
    std::fs::write(pid_path, pid.to_string())
        .unwrap_or_else(|e| panic!("failed to write pid file {}: {e}", pid_path.display()));

    println!("roco {subcmd} started (PID {pid})");
    println!("  log:      {}", log_path.display());
    println!("  pidfile:  {}", pid_path.display());
    std::mem::forget(child);
}


/// Check if a daemon is running via health endpoint (synchronous, spawns its
/// own runtime if needed).
pub fn is_running(name: &str, port: u16) -> bool {
    let pid_file = pid_path(name);
    if !pid_file.exists() {
        return false;
    }

    let url = format!("http://127.0.0.1:{}/health", port);

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return false,
    };
    let healthy = rt.block_on(async {
        reqwest::get(&url)
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    });

    if !healthy {
        let _ = std::fs::remove_file(&pid_file);
        return false;
    }
    true
}



/// Locate the `roco-inferd` binary (sibling of current exe, then PATH).
fn find_inferd(current_exe: &PathBuf) -> Option<PathBuf> {
    if let Some(dir) = current_exe.parent() {
        let sibling = dir.join("roco-inferd");
        if sibling.is_file() {
            return Some(sibling);
        }
        // cargo run layout: target/debug/roco next to target/debug/roco-inferd
    }
    // PATH lookup
    if let Ok(path) = std::env::var("PATH") {
        for entry in path.split(':') {
            let cand = PathBuf::from(entry).join("roco-inferd");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Start the local GPU inference daemon.
///
/// Prefers the dedicated `roco-inferd` binary (does not live inside `roco`,
/// so the CLI never links wgpu). Falls back to `roco server` only if
/// `roco-inferd` is missing, with a loud warning — that fallback cannot
/// load a model anymore and will itself try to reach inferd.
pub fn ensure_inference_daemon(roco_exe: &PathBuf, port: u16) -> bool {
    if is_running("server", port) || is_running("inferd", port) {
        return true;
    }
    let _ = std::fs::create_dir_all(pid_dir());

    if let Some(inferd) = find_inferd(roco_exe) {
        let log_file_path = log_path("inferd", port);
        let pid_file_path = pid_path("inferd");
        let _ = std::fs::remove_file(&pid_file_path);
        let log_file = match std::fs::File::create(&log_file_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Warning: failed to create log {}: {e}", log_file_path.display());
                return false;
            }
        };
        let log_clone = match log_file.try_clone() {
            Ok(c) => c,
            Err(_) => return false,
        };
        match Command::new(&inferd)
            .args(["--port", &port.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(log_clone)
            .spawn()
        {
            Ok(child) => {
                let pid = child.id();
                let _ = std::fs::write(&pid_file_path, pid.to_string());
                // Also write server.pid so legacy is_running("server") checks pass.
                let _ = std::fs::write(pid_path("server"), pid.to_string());
                eprintln!(
                    "Started roco-inferd (PID {pid}, log: {})",
                    log_file_path.display()
                );
                return false;
            }
            Err(e) => {
                eprintln!("Warning: failed to spawn roco-inferd: {e}");
            }
        }
    } else {
        eprintln!(
            "error: `roco-inferd` not found next to {} or on PATH.\n             Local GPU inference was split out of the CLI so everyday builds stay fast.\n             Build it with:  cargo build -p roco-inferd\n             Or:             make build-inferd",
            roco_exe.display()
        );
    }
    false
}

/// Start a daemon if not already running. Safe to call from both sync and
/// async contexts. Tries to detect an already-running instance first.
pub fn ensure_daemon(exe: &PathBuf, subcmd: &str, port: u16, extra_args: &[&str]) -> bool {
    // Check if already running. If called from inside a tokio runtime
    // (e.g. gateway daemon), use a dedicated thread to avoid nested block_on.
    let already_running = if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|s| s.spawn(|| is_running(subcmd, port)).join().unwrap_or(false))
    } else {
        is_running(subcmd, port)
    };
    if already_running {
        return true;
    }

    // Ensure pid/log directories exist
    let _ = std::fs::create_dir_all(pid_dir());

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
// Lifecycle: start chain (gateway → server), stop chain (server → gateway)
// ═════════════════════════════════════════════════════════════════════════════

/// Read a PID from a pidfile, returning `None` if the file doesn't exist or
/// is unreadable.
fn read_pid(name: &str) -> Option<u32> {
    let p = pid_path(name);
    let content = std::fs::read_to_string(&p).ok()?;
    content.trim().parse().ok()
}

/// Send a signal to a process by PID. On Unix, sends SIGTERM (15).
fn send_term(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .spawn();
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .spawn();
    }
}

/// Stop the inference server (SIGTERM). Cleans up PID file.
pub fn stop_inference() {
    if let Some(pid) = read_pid("server") {
        eprintln!("Stopping inference server (PID {pid})...");
        send_term(pid);
    }
    let _ = std::fs::remove_file(pid_path("server"));
}

/// Stop the gateway (SIGTERM). Cleans up PID file.
pub fn stop_gateway() {
    if let Some(pid) = read_pid("gateway") {
        eprintln!("Stopping gateway (PID {pid})...");
        send_term(pid);
    }
    let _ = std::fs::remove_file(pid_path("gateway"));
}

/// Stop both daemons: server first, then gateway.
pub fn stop_all() {
    // Server first — gateway depends on it. Give it a moment.
    stop_inference();
    std::thread::sleep(std::time::Duration::from_millis(500));
    stop_gateway();
    // Wait briefly for processes to exit
    std::thread::sleep(std::time::Duration::from_millis(500));
    eprintln!("Stopped.");
}

/// Entry point for the gateway when spawned as a daemon.
/// Ensures the inference server is running before starting.
pub fn run_gateway_with_auto_inference(host: &str, port: u16, target: &str, rate_limit: usize) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    // Ensure inference server is running
    ensure_inference_daemon(&exe, INFERENCE_PORT);

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
    cmd.args(["gateway"]);
    cmd.args(&args);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(log_file);
    cmd.stderr(log_clone);

    // Write PID
    if let Ok(mut child) = cmd.spawn() {
        let pid = child.id();
        std::fs::write(&pid_path, pid.to_string()).ok();
        eprintln!("Gateway started (PID {pid})");
        // Wait for it to finish (child process)
        let _ = child.wait();
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Shared backend resolution — every CLI command uses this instead of loading
// models directly. On first call it auto-starts the daemon chain.
// ═════════════════════════════════════════════════════════════════════════════

/// Return a `RemoteBackend` connected to the gateway, auto-starting the
/// daemon chain (gateway → inference server) on first use.
///
/// Subsequent calls in the same or new processes connect instantly because
/// the daemons stay alive.
pub fn ensure_backend() -> Arc<dyn roco_engine::ModelBackend> {
    use roco_infer_client::RemoteBackend;

    // If there's already a gateway running, connect instantly.
    if is_running("gateway", GATEWAY_PORT) {
        return Arc::new(RemoteBackend::new(format!(
            "http://127.0.0.1:{}",
            GATEWAY_PORT
        )));
    }

    // Start the daemon chain: server first (takes ~25s to load model),
    // then gateway (needs server healthy to pass its own health check).
    let exe = std::env::current_exe().expect("failed to get current exe path");
    eprintln!("Starting background inference service (first load: ~25s)...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime for daemon wait");

    // 1. Start and wait for inference server (roco-inferd)
    ensure_inference_daemon(&exe, INFERENCE_PORT);
    rt.block_on(wait_for_healthy(
        INFERENCE_PORT,
        Duration::from_secs(60),
        "Inference server",
    ))
    .unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    // 2. Start and wait for gateway
    ensure_daemon(&exe, "gateway", GATEWAY_PORT, &["--detach"]);
    rt.block_on(wait_for_healthy(
        GATEWAY_PORT,
        Duration::from_secs(30),
        "Gateway",
    ))
    .unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    Arc::new(RemoteBackend::new(format!(
        "http://127.0.0.1:{}",
        GATEWAY_PORT
    )))
}

/// Backend that wraps RemoteBackend with a dedicated tokio runtime, so it
/// works with synchronous callers (like interact.rs which uses
/// `futures::executor::block_on`).
pub struct TokioBackend {
    inner: Arc<dyn roco_engine::ModelBackend>,
    rt: tokio::runtime::Runtime,
}

impl TokioBackend {
    pub fn new(inner: Arc<dyn roco_engine::ModelBackend>) -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build TokioBackend runtime");
        Self { inner, rt }
    }
}

impl roco_engine::ModelBackend for TokioBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        self.inner.vocab_bytes()
    }

    fn complete(
        &self,
        req: roco_engine::CompletionRequest,
    ) -> futures::future::BoxFuture<
        '_,
        Result<roco_engine::CompletionResponse, roco_engine::EngineError>,
    > {
        let inner = self.inner.clone();
        let rt_handle = self.rt.handle().clone();
        Box::pin(async move {
            // Spawn the actual work on the dedicated tokio runtime so reqwest
            // has a context. Then await the JoinHandle from the caller's
            // executor (which may be futures::executor::block_on).
            rt_handle
                .spawn(async move { inner.complete(req).await })
                .await
                .unwrap_or(Err(roco_engine::EngineError::Backend(
                    "TokioBackend runtime shut down".into(),
                )))
        })
    }

    fn save_state(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<Vec<u8>, roco_engine::EngineError>> {
        self.inner.save_state()
    }

    fn load_state(
        &self,
        state: Vec<u8>,
    ) -> futures::future::BoxFuture<'_, Result<(), roco_engine::EngineError>> {
        self.inner.load_state(state)
    }

    fn feed_eos(
        &self,
        _session: Option<String>,
    ) -> futures::future::BoxFuture<'_, Result<(), roco_engine::EngineError>> {
        Box::pin(async move { Ok(()) })
    }
}

/// Return a backend that works from synchronous code (uses a dedicated tokio
/// runtime so reqwest calls inside `futures::executor::block_on` function).
pub fn ensure_sync_backend() -> Arc<dyn roco_engine::ModelBackend> {
    Arc::new(TokioBackend::new(ensure_backend()))
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
