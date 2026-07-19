//! Daemon with lockfile + Unix socket RPC.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let text = std::fs::read_to_string(lockfile_path()).ok()?;
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
        None
    } else {
        Some(LockInfo {
            pid,
            socket,
            started,
        })
    }
}

fn write_lockfile(pid: u32, socket: &str) -> std::io::Result<()> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    std::fs::write(
        lockfile_path(),
        format!("pid={pid}\nsocket={socket}\nstarted={started}\n"),
    )
}

fn remove_lockfile() {
    let _ = std::fs::remove_file(lockfile_path());
}

fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

fn check_daemon() -> Option<LockInfo> {
    let info = read_lockfile()?;
    if is_process_alive(info.pid) {
        Some(info)
    } else {
        eprintln!("Stale lockfile (PID {} dead)", info.pid);
        remove_lockfile();
        None
    }
}

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
    let info = check_daemon().ok_or_else(|| anyhow::anyhow!("daemon not running"))?;
    let req = serde_json::to_string(&RpcRequest {
        id: 1,
        method: method.to_string(),
        args: args.to_vec(),
    })?;
    let mut stream = UnixStream::connect(&info.socket)?;
    stream.write_all(req.as_bytes())?;
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

fn handle_request(req: &RpcRequest) -> RpcResponse {
    let result = match req.method.as_str() {
        "ping" => Ok("pong".to_string()),
        "status" => Ok("daemon running".to_string()),
        "echo" => Ok(req.args.join(" ")),
        "stop" => Ok("shutting down".to_string()),
        other => Err(format!("unknown: {other}")),
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
        let stream = stream?;
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
                    let _ = stream
                        .try_clone()
                        .and_then(|mut s| write!(s, "{}\n", serde_json::to_string(&resp)?));
                    continue;
                }
            };
            let resp = handle_request(&req);
            let _ = stream
                .try_clone()
                .and_then(|mut s| write!(s, "{}\n", serde_json::to_string(&resp)?));
            if req.method == "stop" {
                eprintln!("daemon stopping");
                remove_lockfile();
                let _ = std::fs::remove_file(&sock);
                return Ok(());
            }
        }
    }
    Ok(())
}

fn spawn_daemon() -> anyhow::Result<()> {
    if let Some(info) = check_daemon() {
        eprintln!("daemon already running (PID {})", info.pid);
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let mut child = process::Command::new(exe)
        .arg("__daemon_run")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .stdin(process::Stdio::null())
        .spawn()?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    if check_daemon().is_some() {
        eprintln!("daemon started");
        Ok(())
    } else if let Some(status) = child.try_wait()? {
        anyhow::bail!("daemon exited: {status}")
    } else {
        anyhow::bail!("daemon started but lockfile not found")
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: daemon <start|stop|status|restart|rpc>");
        process::exit(1);
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
                eprintln!("not running");
            }
            Ok(())
        }
        "status" => {
            if let Some(info) = check_daemon() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                println!(
                    "daemon running\n  PID: {}\n  Uptime: {}s",
                    info.pid,
                    now - info.started
                );
            } else {
                eprintln!("not running");
            }
            Ok(())
        }
        "restart" => {
            if check_daemon().is_some() {
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
                Ok(r) => {
                    println!("{r}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1)
                }
            }
        }
        "__daemon_run" => run_daemon(),
        other => {
            eprintln!("Unknown: {other}");
            process::exit(1)
        }
    }
}
