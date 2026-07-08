//! Command-execution sandbox + guard policy for RoCo AI tool-use.
//!
//! A pragmatic, dependency-free sandbox: it bounds execution time, isolates
//! output size, and — crucially — applies a [`GuardPolicy`] that can
//! *intercept* a command **before** it runs. This mirrors the rwkv-harness
//! evals `sandbox_guard_intercept_handling` and `can_bypass_loose_sandbox_guard`:
//! the harness gates dangerous commands at the policy layer rather than via a
//! kernel sandbox. (It is a policy gate, not OS-level isolation.)
//!
//! Commands run through a shell (`/bin/sh -c`) by default, matching the
//! harness `bash` tool; direct binary execution is also supported.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("command denied by sandbox guard: {0}")]
    Denied(String),
    #[error("command timed out after {0:?}")]
    Timeout(Duration),
    #[error("failed to spawn command: {0}")]
    Spawn(String),
}

/// Verdict from the guard before a command runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardVerdict {
    /// Safe to execute.
    Allow,
    /// Blocked by policy; `reason` explains why.
    Deny(String),
}

/// Policy applied to every command string before execution.
#[derive(Debug, Clone)]
pub enum GuardPolicy {
    /// Allow everything (execution is still timeout-bounded).
    Permissive,
    /// Only allow if the first token (the invoked binary) is in the list.
    AllowList(Vec<String>),
    /// Block any command whose text contains one of the forbidden substrings.
    DenyList(Vec<String>),
}

impl GuardPolicy {
    /// Inspect a command string and return a verdict.
    pub fn check(&self, command: &str) -> GuardVerdict {
        match self {
            GuardPolicy::Permissive => GuardVerdict::Allow,
            GuardPolicy::DenyList(patterns) => {
                for p in patterns {
                    if command.contains(p) {
                        return GuardVerdict::Deny(format!("matches blocked pattern '{p}'"));
                    }
                }
                GuardVerdict::Allow
            }
            GuardPolicy::AllowList(allowed) => {
                let first = command.split_whitespace().next().unwrap_or("");
                if allowed.iter().any(|a| a == first) {
                    GuardVerdict::Allow
                } else {
                    GuardVerdict::Deny(format!("'{first}' is not in the allowlist"))
                }
            }
        }
    }
}

/// Captured result of a sandboxed command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
    pub truncated: bool,
}

/// A timeout-bounded, policy-gated command runner.
pub struct Sandbox {
    policy: GuardPolicy,
    timeout: Duration,
    max_output: usize,
    cwd: Option<PathBuf>,
}

impl Default for Sandbox {
    fn default() -> Self {
        Self {
            policy: GuardPolicy::Permissive,
            timeout: Duration::from_secs(30),
            max_output: 8_000,
            cwd: None,
        }
    }
}

impl Sandbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(mut self, policy: GuardPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_output(mut self, max_output: usize) -> Self {
        self.max_output = max_output;
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Run a shell command (`/bin/sh -c`), applying the guard first.
    pub fn run_shell(&self, command: &str) -> Result<CommandOutput, SandboxError> {
        if let GuardVerdict::Deny(reason) = self.policy.check(command) {
            return Err(SandboxError::Denied(reason));
        }
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg(command);
        self.execute(cmd)
    }

    /// Run a binary directly with arguments, applying the guard to the
    /// joined command line (allowlist matches the binary basename).
    pub fn run(&self, bin: &str, args: &[&str]) -> Result<CommandOutput, SandboxError> {
        let joined = {
            let mut s = bin.to_string();
            for a in args {
                s.push(' ');
                s.push_str(a);
            }
            s
        };
        if let GuardVerdict::Deny(reason) = self.policy.check(&joined) {
            return Err(SandboxError::Denied(reason));
        }
        let mut cmd = Command::new(bin);
        cmd.args(args);
        self.execute(cmd)
    }

    /// Spawn, enforce timeout, and capture stdout/stderr.
    fn execute(&self, mut cmd: Command) -> Result<CommandOutput, SandboxError> {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| SandboxError::Spawn(e.to_string()))?;

        let start = Instant::now();
        let mut timed_out = false;
        loop {
            match child.try_wait().map_err(|e| SandboxError::Spawn(e.to_string()))? {
                Some(_) => break,
                None => {
                    if start.elapsed() >= self.timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        timed_out = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut stdout);
        }
        if let Some(mut err) = child.stderr.take() {
            let _ = err.read_to_string(&mut stderr);
        }
        let exit_code = if timed_out {
            -1
        } else {
            child
                .wait()
                .map(|s| s.code().unwrap_or(-1))
                .unwrap_or(-1)
        };

        let truncated = stdout.len() > self.max_output || stderr.len() > self.max_output;
        if stdout.len() > self.max_output {
            stdout.truncate(self.max_output);
        }
        if stderr.len() > self.max_output {
            stderr.truncate(self.max_output);
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
            timed_out,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_shell_runs_and_captures() {
        let sb = Sandbox::new();
        let out = sb.run_shell("echo hello-world").unwrap();
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("hello-world"));
        assert!(!out.timed_out);
    }

    #[test]
    fn denylist_intercepts_dangerous_command() {
        // Mirrors sandbox_guard_intercept_handling: the guard blocks before spawn.
        let sb = Sandbox::new().with_policy(GuardPolicy::DenyList(vec![
            "rm -rf /".to_string(),
            "sudo".to_string(),
            "mkfs".to_string(),
        ]));
        let err = sb.run_shell("rm -rf / --no-preserve-root").unwrap_err();
        assert!(matches!(err, SandboxError::Denied(_)));
    }

    #[test]
    fn allowlist_permits_approved_binary_only() {
        let sb = Sandbox::new().with_policy(GuardPolicy::AllowList(vec!["echo".to_string()]));
        // echo (via sh -c) is allowed
        assert!(matches!(sb.run_shell("echo ok").unwrap().exit_code, 0));
        // cat is not in the allowlist -> denied
        let err = sb.run_shell("cat /etc/hostname").unwrap_err();
        assert!(matches!(err, SandboxError::Denied(_)));
    }

    #[test]
    fn timeout_kills_long_running_command() {
        let sb = Sandbox::new()
            .with_timeout(Duration::from_millis(200))
            .with_policy(GuardPolicy::Permissive);
        let out = sb.run_shell("sleep 5").unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, -1);
    }

    #[test]
    fn output_is_truncated_to_max() {
        let sb = Sandbox::new()
            .with_max_output(500)
            .with_policy(GuardPolicy::Permissive);
        // 20000 'x' bytes via /dev/zero + tr; must be truncated to <= 500.
        let out = sb
            .run_shell("head -c 20000 /dev/zero | tr '\\0' 'x'")
            .unwrap();
        assert!(out.truncated);
        assert!(out.stdout.len() <= 500);
    }

    #[test]
    fn direct_binary_execution_respects_allowlist() {
        let sb = Sandbox::new().with_policy(GuardPolicy::AllowList(vec!["echo".to_string()]));
        let out = sb.run("echo", &["direct"]).unwrap();
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.contains("direct"));
    }

    #[test]
    fn nonzero_exit_is_captured_not_error() {
        // `false` exits 1; we still return output rather than erroring.
        let sb = Sandbox::new().with_policy(GuardPolicy::Permissive);
        let out = sb.run_shell("false").unwrap();
        assert_eq!(out.exit_code, 1);
    }
}
