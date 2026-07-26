//! Test harness and automation tools for running CLI subcommands and interactive TUI
//! sessions against the deterministic `MockBackend`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use roco_engine::MockBackend;
use roco_protocol::ConversationState;
use tempfile::TempDir;

/// Output and result of a CLI command run under the test harness.
#[derive(Debug)]
pub struct CliTestResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub temp_dir: Arc<TempDir>,
}

impl CliTestResult {
    /// Assert that the command completed successfully (exit code 0).
    pub fn assert_success(&self) -> &Self {
        assert_eq!(
            self.exit_code, 0,
            "Expected exit status 0, got {}\nSTDOUT:\n{}\nSTDERR:\n{}",
            self.exit_code, self.stdout, self.stderr
        );
        self
    }

    /// Assert that stdout contains the given substring.
    pub fn assert_stdout_contains(&self, expected: &str) -> &Self {
        assert!(
            self.stdout.contains(expected),
            "Expected stdout to contain {:?}\nActual STDOUT:\n{}",
            expected,
            self.stdout
        );
        self
    }

    /// Assert that stderr contains the given substring.
    pub fn assert_stderr_contains(&self, expected: &str) -> &Self {
        assert!(
            self.stderr.contains(expected),
            "Expected stderr to contain {:?}\nActual STDERR:\n{}",
            expected,
            self.stderr
        );
        self
    }

    /// Path to the isolated `.roco` directory in the test environment.
    pub fn roco_dir(&self) -> PathBuf {
        self.temp_dir.path().join(".roco")
    }

    /// Path to saved sessions in the test environment.
    pub fn sessions_dir(&self) -> PathBuf {
        self.roco_dir().join("sessions")
    }

    /// Path to stories directory in the test environment.
    pub fn stories_dir(&self) -> PathBuf {
        self.roco_dir().join("stories")
    }

    /// Path to workspaces directory in the test environment.
    pub fn workspaces_dir(&self) -> PathBuf {
        self.roco_dir().join("workspaces")
    }

    /// Load saved conversation sessions from disk.
    pub fn load_sessions(&self) -> Vec<ConversationState> {
        let dir = self.sessions_dir();
        if !dir.exists() {
            return Vec::new();
        }
        let mut sessions = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "json") {
                    if let Ok(state) = ConversationState::load(&entry.path()) {
                        sessions.push(state);
                    }
                }
            }
        }
        sessions
    }

    /// Assert at least one session JSON was saved and return the most recent one.
    pub fn assert_latest_session(&self) -> ConversationState {
        let sessions = self.load_sessions();
        assert!(
            !sessions.is_empty(),
            "Expected at least one session file saved in {:?}",
            self.sessions_dir()
        );
        sessions.into_iter().last().unwrap()
    }

    /// List generated story `.md` files in `.roco/stories/`.
    pub fn list_stories(&self) -> Vec<PathBuf> {
        let dir = self.stories_dir();
        if !dir.exists() {
            return Vec::new();
        }
        fs::read_dir(dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Automated runner for testing CLI subcommands against `MockBackend`.
#[derive(Clone)]
pub struct MockCliRunner {
    temp_dir: Arc<TempDir>,
    custom_working_dir: Option<PathBuf>,
    env_vars: HashMap<String, String>,
    stdin_script: Vec<String>,
}

impl Default for MockCliRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCliRunner {
    /// Create a new CLI test runner with an isolated temp directory.
    pub fn new() -> Self {
        let temp_dir =
            Arc::new(TempDir::new().expect("failed to create temp dir for test runner"));
        let mut env_vars = HashMap::new();
        env_vars.insert("ROCO_USE_MOCK_BACKEND".to_string(), "1".to_string());
        env_vars.insert("RWKV_MODEL".to_string(), "mock-model".to_string());

        Self {
            temp_dir,
            custom_working_dir: None,
            env_vars,
            stdin_script: Vec::new(),
        }
    }

    /// Add or override an environment variable.
    pub fn with_env<K: Into<String>, V: Into<String>>(mut self, key: K, val: V) -> Self {
        self.env_vars.insert(key.into(), val.into());
        self
    }

    /// Script stdin lines to be fed into interactive CLI / REPL prompts.
    pub fn with_stdin_lines<S: AsRef<str>>(mut self, lines: &[S]) -> Self {
        self.stdin_script = lines.iter().map(|s| s.as_ref().to_string()).collect();
        self
    }

    /// Set a custom working directory instead of the default temp dir.
    pub fn with_working_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.custom_working_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Root working directory of the test workspace.
    pub fn working_dir(&self) -> &Path {
        self.custom_working_dir
            .as_deref()
            .unwrap_or_else(|| self.temp_dir.path())
    }

    /// Get a fresh MockBackend instance with default config.
    pub fn mock_backend(&self) -> MockBackend {
        MockBackend::default()
    }

    /// Locate the `roco` executable path.
    pub fn locate_roco_bin() -> PathBuf {
        if let Ok(path) = std::env::var("CARGO_BIN_EXE_roco") {
            return PathBuf::from(path);
        }
        if let Ok(path) = std::env::var("ROCO_BIN_PATH") {
            return PathBuf::from(path);
        }
        if let Ok(current) = std::env::current_exe() {
            if let Some(dir) = current.parent() {
                let candidate = dir.join("roco");
                if candidate.exists() {
                    return candidate;
                }
                if let Some(parent) = dir.parent() {
                    let candidate = parent.join("roco");
                    if candidate.exists() {
                        return candidate;
                    }
                }
            }
        }
        PathBuf::from("target/debug/roco")
    }

    /// Execute the compiled binary (`target/debug/roco`) with the given arguments.
    pub fn run_binary<I, S>(&self, args: I) -> CliTestResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let exe = Self::locate_roco_bin();
        let mut cmd = Command::new(&exe);
        cmd.current_dir(self.working_dir());
        cmd.args(args);

        for (k, v) in &self.env_vars {
            cmd.env(k, v);
        }

        let stdin_input = if self.stdin_script.is_empty() {
            String::new()
        } else {
            let mut s = self.stdin_script.join("\n");
            s.push('\n');
            s
        };

        if !stdin_input.is_empty() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().expect("failed to spawn roco binary");

        if !stdin_input.is_empty() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(stdin_input.as_bytes());
            }
        }

        let output = child
            .wait_with_output()
            .expect("failed to wait for roco output");

        CliTestResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            temp_dir: self.temp_dir.clone(),
        }
    }
}

/// Specialized helper to test interactive TUI / REPL sessions.
#[derive(Clone, Default)]
pub struct ScriptedTuiSession {
    runner: MockCliRunner,
    commands: Vec<String>,
}

impl ScriptedTuiSession {
    pub fn new() -> Self {
        Self {
            runner: MockCliRunner::new(),
            commands: Vec::new(),
        }
    }

    pub fn with_runner(runner: MockCliRunner) -> Self {
        Self {
            runner,
            commands: Vec::new(),
        }
    }

    pub fn type_line<S: Into<String>>(mut self, input: S) -> Self {
        self.commands.push(input.into());
        self
    }

    pub fn type_lines<S: AsRef<str>>(mut self, lines: &[S]) -> Self {
        for l in lines {
            self.commands.push(l.as_ref().to_string());
        }
        self
    }

    pub fn run_subcommand(&self, subcmd: &str, extra_args: &[&str]) -> CliTestResult {
        let mut args = vec![subcmd];
        args.extend(extra_args.iter().copied());
        self.runner
            .clone()
            .with_stdin_lines(&self.commands)
            .run_binary(args)
    }
}
