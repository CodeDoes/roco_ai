//! Daemon lifecycle — auto-start gateway and inference server as needed.
//!
//! `roco gui` → auto-starts Gateway (if not running)
//! Gateway → auto-starts Inference Server (if not running)
//! All CLI commands use `ensure_backend()` instead of loading models directly.
//!
//! ## Dev vs production mode
//!
//! When running from a cargo workspace (`target/debug/` or `target/release/`),
//! daemons are spawned via `cargo run -p <package>` so code changes are picked
//! up automatically. In production (binary installed system-wide), daemons are
//! assumed pre-installed and spawned directly.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// Detect if we're running from a Cargo workspace (development mode).
///
/// Checks whether `current_exe()` lives under `target/debug/` or
/// `target/release/`. In dev mode, daemon spawning uses `cargo run`
/// instead of direct binary execution, so code changes are reflected
/// on every restart.
fn is_dev_mode() -> bool {
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        path.contains("/target/debug/") || path.contains("/target/release/")
    } else {
        false
    }
}

/// Default ports — using the 18xxx range to avoid conflicts with common
/// services (8000 ← Python http.server, 8080 ← Tomcat/dev proxies).
///
/// Override with `ROCO_INFERD_PORT` / `ROCO_GATEWAY_PORT` env vars.
const DEFAULT_INFERD_PORT: u16 = 18080;
const DEFAULT_GATEWAY_PORT: u16 = 18000;
/// Read a u16 port from an environment variable, returning `default` if unset or invalid.
fn port_from_env(var: &str, default: u16) -> u16 {
    match std::env::var(var) {
        Ok(v) => v.parse::<u16>().unwrap_or_else(|_| {
            eprintln!("Warning: ${var}={v} is not a valid port, using {default}");
            default
        }),
        Err(_) => default,
    }
}

/// Gateway port. Override with `ROCO_GATEWAY_PORT` env var.
pub fn gateway_port() -> u16 {
    port_from_env("ROCO_GATEWAY_PORT", DEFAULT_GATEWAY_PORT)
}

/// Inference daemon port. Override with `ROCO_INFERD_PORT` env var.
pub fn inferd_port() -> u16 {
    port_from_env("ROCO_INFERD_PORT", DEFAULT_INFERD_PORT)
}

/// Legacy constants kept for backwards compat — prefer `gateway_port()` /
/// `inferd_port()` which respect env vars.
pub const GATEWAY_PORT: u16 = 18000;
pub const INFERENCE_PORT: u16 = 18080;
pub const GATEWAY_TARGET: &str = "http://127.0.0.1:18080";

// ═════════════════════════════════════════════════════════════════════════════
// PID file management
// ═════════════════════════════════════════════════════════════════════════════

fn pid_dir() -> PathBuf {
    match std::env::var("ROCO_PID_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => std::env::temp_dir().join("roco"),
    }
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

    // Append to existing log (don't truncate on re-start)
    let log_file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)
        .unwrap_or_else(|e| panic!("failed to open log {}: {e}", log_path.display()));
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

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Check if a daemon is running via health endpoint or PID process check.
pub fn is_running(name: &str, port: u16) -> bool {
    let pid_file = pid_path(name);
    if !pid_file.exists() {
        return false;
    }

    if let Some(pid) = read_pid(name) {
        if !is_pid_alive(pid) {
            // Process is dead; clean up stale PID file.
            let _ = std::fs::remove_file(&pid_file);
            return false;
        }
    }

    let url = format!("http://127.0.0.1:{}/health", port);

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return true,
    };
    
    // Process is alive. Require a successful health response within 3s.
    // A failing health check means it's either a different process that
    // reused the PID, or our daemon has crashed after the fork.
    let healthy = rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .ok()?;
        let resp = client.get(&url).send().await.ok()?;
        Some(resp.status().is_success())
    });

    healthy.unwrap_or(false)
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
        // Append to existing log (don't truncate on re-start)
    let log_file = match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&log_file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Warning: failed to open log {}: {e}",
                log_file_path.display()
            );
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
                return true;
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

    // stdout/stderr → log file (append, don't truncate on re-start)
    let log_file = match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&log_file_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Warning: failed to open log file {}: {e}",
                log_file_path.display()
            );
            return false;
        }
    };
    let log_clone = match log_file.try_clone() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let cmd_result = if is_dev_mode() {
        // Dev mode: use cargo run --bin roco -- <subcmd> <args>
        // This picks up code changes automatically.
        let mut cargo_args: Vec<&str> = vec!["run", "--bin", "roco", "--"];
        cargo_args.extend(args.iter().map(|s| s.as_str()));
        eprintln!(
            "Dev mode: building + starting gateway via cargo run --bin roco -- {} ...",
            args.join(" ")
        );
        Command::new("cargo")
            .args(&cargo_args)
            .stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(log_clone)
            .spawn()
    } else {
        Command::new(exe)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(log_clone)
            .spawn()
    };

    match cmd_result {
        Ok(child) => {
            let pid = child.id();
            if let Err(e) = std::fs::write(&pid_file_path, pid.to_string()) {
                eprintln!("Warning: failed to write PID file: {e}");
            }
            eprintln!(
                "Started {} ({} PID {pid}, log: {})",
                subcmd,
                if is_dev_mode() { "cargo" } else { "process" },
                log_file_path.display()
            );
            true
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
    if let Some(pid) = read_pid("inferd").or_else(|| read_pid("server")) {
        eprintln!("Stopping inference server (PID {pid})...");
        send_term(pid);
    }
    let _ = std::fs::remove_file(pid_path("inferd"));
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
    let had_inferd = read_pid("inferd").or_else(|| read_pid("server")).is_some();
    let had_gateway = read_pid("gateway").is_some();
    // Server first — gateway depends on it. Give it a moment.
    stop_inference();
    std::thread::sleep(std::time::Duration::from_millis(500));
    stop_gateway();
    // Wait briefly for processes to exit
    std::thread::sleep(std::time::Duration::from_millis(500));
    if had_inferd || had_gateway {
        eprintln!("Stopped.");
    } else {
        eprintln!("No daemons were running.");
    }
}

/// Reload the inference daemon by stopping any running process and starting a new one.
pub fn reload_inference_daemon(roco_exe: &PathBuf, port: u16) -> bool {
    stop_inference();
    std::thread::sleep(std::time::Duration::from_millis(500));
    ensure_inference_daemon(roco_exe, port)
}

/// Reload the gateway daemon by stopping any running process and starting a new one.
pub fn reload_gateway_daemon(_roco_exe: &PathBuf, port: u16) -> bool {
    stop_gateway();
    std::thread::sleep(std::time::Duration::from_millis(500));
    let log_path = log_path("gateway", port);
    let pid_path = pid_path("gateway");
    spawn_detached("gateway", &[], &log_path, &pid_path);
    true
}

/// Entry point for the gateway when spawned as a daemon.
/// Ensures the inference server is running before starting.
pub fn run_gateway_with_auto_inference(host: &str, port: u16, target: &str, rate_limit: usize) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    // Ensure inference server is running (respects $ROCO_INFERD_PORT)
    ensure_inference_daemon(&exe, inferd_port());

    // Build args for the gateway (without --detach, as we're already the child)
    let args = vec![
        format!("--host={}", host),
        format!("--port={}", port),
        format!("--target={}", target),
        format!("--rate-limit={}", rate_limit),
    ];

    let log_path = log_path("gateway", port);
    let pid_path = pid_path("gateway");

    // Redirect stdio (append, don't truncate on re-start)
    let log_file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&log_path)
        .unwrap_or_else(|e| panic!("failed to open log file {}: {e}", log_path.display()));
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

    if std::env::var("ROCO_USE_MOCK_BACKEND").is_ok() {
        return Arc::new(roco_engine::MockBackend::default());
    }

    let gp = gateway_port();
    // If there's already a gateway running, connect instantly.
    if is_running("gateway", gp) {
        return Arc::new(RemoteBackend::new(format!(
            "http://127.0.0.1:{}",
            gp
        )));
    }

    // Client starts Gateway; Gateway auto-starts inferd internally.
    let exe = std::env::current_exe().expect("failed to get current exe path");
    eprintln!("Starting Gateway & background inference service (first load: ~25s)...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime for daemon wait");

    // Start and wait for Gateway
    ensure_daemon(&exe, "gateway", gp, &["--detach"]);
    rt.block_on(wait_for_healthy(
        gp,
        Duration::from_secs(90),
        "Gateway",
    ))
    .unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    Arc::new(RemoteBackend::new(format!(
        "http://127.0.0.1:{}",
        gp
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
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
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
        // `futures::executor::block_on` parks the calling thread and polls the
        // returned future inline — it provides NO tokio reactor on that thread.
        // Awaiting a `JoinHandle` inside an `async move {}` polled by block_on
        // therefore deadlocks: the join handle needs a reactor to get woken up,
        // but block_on never installs one.
        //
        // Fix: do the actual async work synchronously on the dedicated tokio
        // runtime via `block_on` (which DOES run a reactor), then return the
        // result wrapped in a ready future.  This is safe because block_on is
        // called on a fresh thread-scope thread, never inside the rt itself.
        let result = std::thread::scope(|s| {
            s.spawn(|| rt_handle.block_on(inner.complete(req)))
                .join()
                .unwrap_or(Err(roco_engine::EngineError::Backend(
                    "TokioBackend thread panicked".into(),
                )))
        });
        Box::pin(futures::future::ready(result))
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

    fn bake_state<'a>(
        &'a self,
        session_id: &'a str,
        system: &'a str,
        few_shots: &'a [(&'a str, &'a str)],
    ) -> futures::future::BoxFuture<'a, Result<String, roco_engine::EngineError>> {
        let inner = self.inner.clone();
        let rt_handle = self.rt.handle().clone();
        let session_id = session_id.to_string();
        let system = system.to_string();
        let few_shots: Vec<(String, String)> = few_shots
            .iter()
            .map(|(u, a)| (u.to_string(), a.to_string()))
            .collect();
        // Same fix as complete(): run synchronously on the dedicated runtime
        // via block_on on a scoped thread to avoid the JoinHandle deadlock.
        let result = std::thread::scope(|s| {
            s.spawn(|| {
                rt_handle.block_on(inner.bake_state(
                    &session_id,
                    &system,
                    &few_shots
                        .iter()
                        .map(|(u, a)| (u.as_str(), a.as_str()))
                        .collect::<Vec<_>>(),
                ))
            })
            .join()
            .unwrap_or(Err(roco_engine::EngineError::Backend(
                "TokioBackend bake_state thread panicked".into(),
            )))
        });
        Box::pin(futures::future::ready(result))
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
    use roco_engine::ModelBackend;
    use std::time::Duration;

    #[test]
    fn test_pid_paths() {
        let p = pid_path("gateway");
        assert!(p.to_string_lossy().contains("gateway.pid"));
    }

    #[test]
    fn test_log_paths() {
        let p = log_path("server", 18080);
        assert!(p.to_string_lossy().contains("server_18080.log"));
    }

    #[test]
    fn test_constants() {
        assert_eq!(GATEWAY_PORT, 18000);
        assert_eq!(INFERENCE_PORT, 18080);
        assert_eq!(GATEWAY_TARGET, "http://127.0.0.1:18080");
    }

    // ── TokioBackend deadlock regression tests ────────────────────────────────
    //
    // These tests exist specifically to catch the class of bug where
    // `TokioBackend::complete` deadlocks when called from
    // `futures::executor::block_on` (no surrounding tokio reactor on the
    // calling thread).  The old implementation did:
    //
    //   Box::pin(async move { rt_handle.spawn(work).await })
    //
    // `futures::executor::block_on` polls that future inline on the calling
    // thread with no tokio reactor installed.  `JoinHandle::await` needs a
    // reactor to receive its wakeup, so it never completes — infinite hang.
    //
    // The fix: run the work synchronously on a scoped thread via
    // `rt_handle.block_on(...)`, then return `futures::future::ready(result)`.
    // A ready future needs no reactor; block_on polls it once and returns.
    //
    // Every test below uses a 2-second timeout enforced by a watcher thread.
    // If any call hangs the watcher kills the process, making the failure
    // visible as a timeout rather than an infinite hang.

    /// Run `f` on a fresh thread; panic if it doesn't finish within `timeout`.
    /// This is the regression harness — if the deadlock regresses, the test
    /// fails with "TokioBackend deadlocked" rather than hanging the suite.
    fn assert_completes_within<F, R>(label: &str, timeout: Duration, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = f();
            let _ = tx.send(result);
        });
        rx.recv_timeout(timeout)
            .unwrap_or_else(|_| panic!("{label}: TokioBackend deadlocked (exceeded {timeout:?})"))
    }

    fn make_backend() -> TokioBackend {
        TokioBackend::new(Arc::new(roco_engine::MockBackend::default()))
    }

    // ── Scenario 1: called from futures::executor::block_on (no tokio runtime) ─
    // This is the exact production call path: router.rs calls
    // `futures::executor::block_on(backend.complete(req))` on the main thread.

    #[test]
    fn tokio_backend_complete_via_futures_block_on_no_surrounding_runtime() {
        assert_completes_within(
            "complete via futures::executor::block_on",
            Duration::from_secs(2),
            || {
                let backend = make_backend();
                let req = roco_engine::CompletionRequest::new("sys", "hello");
                let res = futures::executor::block_on(backend.complete(req));
                assert!(res.is_ok(), "expected Ok, got {res:?}");
                res.unwrap().text
            },
        );
    }

    // ── Scenario 2: called from inside a tokio::test runtime ─────────────────
    // Guards against the inverse: TokioBackend must also work when the caller
    // IS inside a tokio runtime (e.g. async agent code calling complete).
    // We construct and run the backend on a blocking thread so that
    // TokioBackend's inner runtime is dropped outside the async context
    // (tokio forbids dropping a Runtime from within an async context).

    #[tokio::test]
    async fn tokio_backend_complete_from_inside_tokio_runtime() {
        let res = tokio::task::spawn_blocking(|| {
            let backend = make_backend();
            let req = roco_engine::CompletionRequest::new("sys", "hello from tokio");
            futures::executor::block_on(backend.complete(req))
        })
        .await
        .expect("blocking task panicked")
        .expect("completion failed");
        assert!(!res.text.is_empty());
    }

    // ── Scenario 3: concurrent calls from futures::executor::block_on ─────────
    // Verifies the scoped-thread approach doesn't serialise badly or deadlock
    // under concurrent load.

    #[test]
    fn tokio_backend_complete_concurrent_block_on_calls() {
        let backend = Arc::new(make_backend());
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let b = backend.clone();
                std::thread::spawn(move || {
                    let req = roco_engine::CompletionRequest::new("sys", &format!("msg {i}"));
                    futures::executor::block_on(b.complete(req))
                })
            })
            .collect();
        for h in handles {
            let res = h
                .join()
                .expect("thread panicked")
                .expect("completion failed");
            assert!(!res.text.is_empty());
        }
    }

    // ── Scenario 4: bake_state via futures::executor::block_on ───────────────
    // bake_state had the same spawn().await bug.

    #[test]
    fn tokio_backend_bake_state_via_futures_block_on_no_surrounding_runtime() {
        assert_completes_within(
            "bake_state via futures::executor::block_on",
            Duration::from_secs(2),
            || {
                let backend = make_backend();
                let res = futures::executor::block_on(
                    backend.bake_state("sess-1", "system prompt", &[("hi", "hello")]),
                );
                // MockBackend returns Ok for bake_state; we just care it doesn't hang.
                let _ = res; // Ok or Err, not deadlocked
            },
        );
    }

    // ── Scenario 5: ensure_sync_backend() round-trip ─────────────────────────
    // ensure_sync_backend() is the function every CLI command calls.
    // In test mode (ROCO_USE_MOCK_BACKEND=1) it returns MockBackend directly;
    // verify it completes without deadlock via block_on either way.

    #[test]
    fn ensure_sync_backend_mock_completes_via_block_on() {
        // Force mock mode so this test never tries to spawn a real daemon.
        std::env::set_var("ROCO_USE_MOCK_BACKEND", "1");
        let backend = ensure_sync_backend();
        let req = roco_engine::CompletionRequest::new("sys", "smoke test");
        assert_completes_within(
            "ensure_sync_backend via block_on",
            Duration::from_secs(2),
            move || {
                let res = futures::executor::block_on(backend.complete(req));
                assert!(res.is_ok(), "expected Ok from mock sync backend");
            },
        );
    }

    // ── Scenario 6: the old broken implementation would deadlock here ─────────
    // This is the minimal reproduction of the original bug.
    // The old code: Box::pin(async move { rt_handle.spawn(work).await })
    // When polled by futures::executor::block_on:
    //   - block_on parks the calling thread
    //   - spawn queues work on the background runtime (fine)
    //   - JoinHandle::await needs a tokio waker; block_on provides none
    //   - JoinHandle::await never wakes up -> infinite block
    //
    // The fixed code returns futures::future::ready(result) which polls
    // to completion in a single synchronous step, needing no reactor.

    #[test]
    fn tokio_backend_result_is_immediately_ready_no_reactor_needed() {
        use std::future::Future;
        let backend = make_backend();
        let req = roco_engine::CompletionRequest::new("sys", "ready check");
        // complete() must return a future that is Poll::Ready on the very
        // first poll — no waker, no reactor involvement required.
        let mut future = backend.complete(req);
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        let pin = std::pin::Pin::new(&mut future);
        match Future::poll(pin, &mut cx) {
            std::task::Poll::Ready(res) => {
                assert!(res.is_ok(), "expected Ok on first poll, got {res:?}");
            }
            std::task::Poll::Pending => {
                panic!("TokioBackend::complete returned Pending — will deadlock under futures::executor::block_on (regression of the spawn().await bug)");
            }
        }
    }
}
