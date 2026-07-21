//! Re-export of the shared daemon lifecycle from `roco_app`.
//!
//! The actual implementation lives in `roco_app::daemon` so that every
//! human-facing surface (`cli`, `tui`, `gui`) shares one backend-resolution
//! and daemon-management path. This file exists only so existing
//! `crate::daemon::*` references in the CLI keep resolving.

pub use roco_app::daemon::*;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Spawn a detached child process for `roco server` or `roco gateway`.
/// The parent redirects stdio to a log file, writes a PID file, and exits.
pub fn spawn_detached(subcmd: &str, extra: &[&str], log_path: &Path, pid_path: &Path) {
    let exe = std::env::current_exe().expect("failed to get current exe path");

    let mut child_args: Vec<String> = Vec::new();
    child_args.push(subcmd.to_string());

    let mut i = 0;
    while i < extra.len() {
        let a = extra[i];
        if a == "--detach" || a == "-d" {
            child_args.push(format!("--_child-{subcmd}"));
        } else if a == "--pid-file" || a == "--log-file" {
            child_args.push(a.to_string());
            if i + 1 < extra.len() {
                child_args.push(extra[i + 1].to_string());
                i += 1;
            }
        } else {
            child_args.push(a.to_string());
        }
        i += 1;
    }

    let log_file = fs::File::create(log_path)
        .unwrap_or_else(|e| panic!("failed to create log file {}: {e}", log_path.display()));
    let log_clone = log_file
        .try_clone()
        .expect("failed to clone log file handle");

    let child = Command::new(&exe)
        .args(&child_args)
        .stdin(fs::File::open("/dev/null").expect("no /dev/null"))
        .stdout(log_file)
        .stderr(log_clone)
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn child: {e}"));

    let pid = child.id();
    fs::write(pid_path, pid.to_string())
        .unwrap_or_else(|e| panic!("failed to write pid file {}: {e}", pid_path.display()));

    println!("roco {subcmd} started (PID {pid})");
    println!("  log:      {}", log_path.display());
    println!("  pidfile:  {}", pid_path.display());
}

/// Compute a default path under `/tmp/roco/` for PID or log files.
pub fn default_detach_path(subcmd: &str, port: u16, ext: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("roco");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{subcmd}_{port}.{ext}"))
}
