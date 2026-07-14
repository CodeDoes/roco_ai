//! Daemon with lockfile + Unix socket RPC.
//!
//! ```bash
//! # Start the daemon in background
//! cargo run -p roco-core --features local-rwkv --example daemon -- start
//!
//! # Check status
//! cargo run -p roco-core --features local-rwkv --example daemon -- status
//!
//! # Send an RPC command
//! cargo run -p roco-core --features local-rwkv --example daemon -- rpc ping
//!
//! # Stop the daemon
//! cargo run -p roco-core --features local-rwkv --example daemon -- stop
//! ```

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Lockfile
// ---------------------------------------------------------------------------

fn lockfile_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("roco-daemon.lock");
    p
}

struct LockInfo {
    pid: u32,
    socket: String,
    started: u64,
}

fn read_lockfile() -> Option<LockInfo> {
    let path = lockfile_path();
    let text = std::fs::read_to_string(&path).ok()?;
    let mut pid = 0u32;
    let mut socket = String::new();
    let mut started = 0u64;
    for line in text.lines() {
        if let Some(val) = line.strip_prefix("pid=") {
            pid = val.parse().ok()?;
        } else if let Some(val) = line.strip_prefix("socket=") {
            socket = val.to_string();
        } else if let Some(val) = line.strip_prefix("started=") {
            started = val.parse().ok().unwrap_or(0);
        }
    }
    if pid == 0 {
        return None;
    }
    Some(LockInfo { pid, socket, started })
}

fn write_lockfile(pid: u32, socket: &str) -> std::io::Result<()> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let content = format!("pid={pid}\nsocket={socket}\nstarted={started}\n");
    std::fs::write(lockfile_path(), content)
}

fn remove_lockfile() {
    let _ = std::fs::remove_file(lockfile_path());
}

fn is_process_alive(pid: u32) -> bool {
    // Linux: check /proc/<pid>
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    // Fallback: assume alive (lockfile will be cleaned up on next stale check)
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

/// Check if a daemon is already running. Returns lock info if alive.
fn check_daemon() -> Option<LockInfo> {
    let info = read_lockfile()?;
    if is_process_alive(info.pid) {
        Some(info)
    } else {
        // Stale lockfile — clean up
        eprintln!("Stale lockfile (PID {} dead), cleaning up", info.pid);
        remove_lockfile();
        None
    }
}

// ---------------------------------------------------------------------------
// RPC protocol (simple line-based JSON)
// ---------------------------------------------------------------------------

fn socket_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("roco-daemon.sock");
    p
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RpcRequest {
    id: u64,
    method: String,
    args: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RpcResponse {
    id: u64,
    ok: bool,
    result: String,
}

fn rpc_call(method: &str, args: &[String]) -> anyhow::Result<String> {
    let info = check_daemon().ok_or_else(|| {
        anyhow::anyhow!("daemon not running — start with `daemon start`")
    })?;

    let req = RpcRequest {
        id: 1,
        method: method.to_string(),
        args: args.to_vec(),
    };
    let req_json = serde_json::to_string(&req)?;

    let mut stream = UnixStream::connect(&info.socket).map_err(|e| {
        anyhow::anyhow!("cannot connect to daemon at {}: {e}", info.socket)
    })?;

    stream.write_all(req_json.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: RpcResponse = serde_json::from_str(&line)?;
    if resp.ok {
        Ok(resp.result)
    } else {
        Err(anyhow::anyhow!("daemon error: {}", resp.result))
    }
}

// ---------------------------------------------------------------------------
// Daemon server
// ---------------------------------------------------------------------------

fn handle_request(req: &RpcRequest) -> RpcResponse {
    let result = match req.method.as_str() {
        "ping" => Ok("pong".to_string()),
        "status" => Ok(format!("daemon running since {}", req.id)),
        "echo" => Ok(req.args.join(" ")),
        "uptime" => {
            let started = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            Ok(format!("uptime: {}s", started))
        }
        "stop" => {
            // Signal the daemon to shut down
            Ok("shutting down".to_string())
        }
        other => Err(format!("unknown method: {other}")),
    };
    RpcResponse {
        id: req.id,
        ok: result.is_ok(),
        result: result.unwrap_or_else(|e| e),
    }
}

fn run_daemon() -> anyhow::Result<()> {
    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);

    let listener = std::os::unix::net::UnixListener::bind(&sock)?;
    write_lockfile(process::id(), sock.to_str().unwrap())?;

    eprintln!(
        "daemon started (PID {}, socket {})",
        process::id(),
        sock.display()
    );

    for stream in listener.incoming() {
        let mut stream = stream?;
        let reader = BufReader::new(stream.try_clone()?);
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                break;
            }
            let req: RpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let resp = RpcResponse {
                        id: 0,
                        ok: false,
                        result: format!("parse error: {e}"),
                    };
                    let _ = stream.write_all(format!("{}\n", serde_json::to_string(&resp)?).as_bytes());
                    continue;
                }
            };

            let resp = handle_request(&req);
            let resp_json = serde_json::to_string(&resp)?;
            let _ = stream.write_all(format!("{resp_json}\n").as_bytes());

            if req.method == "stop" {
                eprintln!("daemon stopping on request");
                remove_lockfile();
                let _ = std::fs::remove_file(&sock);
                return Ok(());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Background spawning
// ---------------------------------------------------------------------------

fn spawn_daemon() -> anyhow::Result<()> {
    if let Some(info) = check_daemon() {
        eprintln!("daemon already running (PID {}, socket {})", info.pid, info.socket);
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    let mut child = process::Command::new(exe)
        .arg("__daemon_run")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()?;

    // Give the daemon a moment to start up
    std::thread::sleep(std::time::Duration::from_millis(500));

    if let Some(info) = check_daemon() {
        eprintln!("daemon started (PID {}, socket {})", info.pid, info.socket);
        Ok(())
    } else {
        // Check if child exited
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("daemon exited immediately with status: {status}");
        }
        anyhow::bail!("daemon started but lockfile not found");
    }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: daemon <start|stop|status|restart|rpc <method> [args...]>");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "start" => spawn_daemon(),
        "stop" => {
            if let Some(info) = check_daemon() {
                let _ = rpc_call("stop", &[]);
                std::thread::sleep(std::time::Duration::from_millis(200));
                remove_lockfile();
                eprintln!("daemon stopped (was PID {})", info.pid);
            } else {
                eprintln!("daemon not running");
            }
            Ok(())
        }
        "status" => {
            if let Some(info) = check_daemon() {
                let started = info.started;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let uptime = now - started;
                println!("daemon running");
                println!("  PID:     {}", info.pid);
                println!("  Socket:  {}", info.socket);
                println!("  Uptime:  {}s", uptime);
                Ok(())
            } else {
                eprintln!("daemon not running");
                Ok(())
            }
        }
        "restart" => {
            if let Some(_info) = check_daemon() {
                let _ = rpc_call("stop", &[]);
                std::thread::sleep(std::time::Duration::from_millis(300));
                remove_lockfile();
            }
            spawn_daemon()
        }
        "rpc" => {
            let method = args.get(2).cloned().unwrap_or_else(|| "ping".to_string());
            let rpc_args: Vec<String> = args.iter().skip(3).cloned().collect();
            match rpc_call(&method, &rpc_args) {
                Ok(result) => {
                    println!("{result}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "__daemon_run" => run_daemon(),
        other => {
            eprintln!("Unknown command: {other}");
            std::process::exit(1);
        }
    }
}
