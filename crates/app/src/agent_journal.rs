//! Agent Journal — a shared log file for monitoring agent actions.
//!
//! Writes structured, timestamped entries to `.roco/agent-journal.md` so
//! the user can `tail -f .roco/agent-journal.md` and see what the agent
//! is doing in real time.
//!
//! # Usage
//!
//! ```ignore
//! use roco_app::AgentJournal;
//!
//! AgentJournal::init(); // once at startup
//! AgentJournal::info("story", "Generating outline...");
//! AgentJournal::action("story", "Written chapter 1 to workspace");
//! AgentJournal::warn("story", "Quality check failed, retrying...");
//! ```
//!
//! The journal file is append-only. Format:
//!
//! ```markdown
//! # Agent Journal — YYYY-MM-DD
//!
//! ## HH:MM:SS
//!
//! ℹ️ **INFO** (story): Generating outline...
//! ```

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global journal instance (locked on write, lazily initialized via `init()`).
fn global_journal() -> &'static Mutex<Option<AgentJournal>> {
    static INSTANCE: OnceLock<Mutex<Option<AgentJournal>>> = OnceLock::new();
    INSTANCE.get_or_init(|| Mutex::new(None))
}

/// A timestamped entry level for the agent journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalLevel {
    Info,
    Action,
    Warn,
    Error,
    Phase,
}

impl JournalLevel {
    fn as_emoji(self) -> &'static str {
        match self {
            JournalLevel::Info => "\u{2139}\u{fe0f}",
            JournalLevel::Action => "\u{2705}",
            JournalLevel::Warn => "\u{26a0}\u{fe0f}",
            JournalLevel::Error => "\u{274c}",
            JournalLevel::Phase => "\u{1f4cc}",
        }
    }
}

/// Agent journal — appends structured entries to `.roco/agent-journal.md`.
///
/// Thread-safe. Multiple components can log concurrently.
pub struct AgentJournal {
    #[allow(dead_code)]
    path: PathBuf,
    file: fs::File,
}

impl AgentJournal {
    /// Open (or create) the agent journal at the default location.
    pub fn open() -> Result<Self, String> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open (or create) the agent journal at a specific path.
    pub fn open_at(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create journal dir: {e}"))?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("cannot open journal: {e}"))?;

        // Write header if file is empty
        let meta = file
            .metadata()
            .map_err(|e| format!("cannot stat journal: {e}"))?;
        if meta.len() == 0 {
            let today = Self::today();
            writeln!(&file, "# Agent Journal — {today}").ok();
            writeln!(&file).ok();
        }

        Ok(Self { path, file })
    }

    /// Write a timestamped entry to the journal.
    pub fn log(&mut self, level: JournalLevel, domain: &str, message: &str) {
        let now = Self::timestamp();
        let emoji = level.as_emoji();
        let level_name = format!("{:?}", level).to_uppercase();

        writeln!(
            &self.file,
            "## {now}\n\n{emoji} **{level_name}** ({domain}): {message}\n"
        )
        .ok();
        let _ = self.file.flush();
    }

    // ── Convenience static methods ────────────────────────────────────

    /// Log an informational message.
    pub fn info(domain: &str, message: &str) {
        Self::log_entry(JournalLevel::Info, domain, message);
    }

    /// Log a completed action.
    pub fn action(domain: &str, message: &str) {
        Self::log_entry(JournalLevel::Action, domain, message);
    }

    /// Log a warning.
    pub fn warn(domain: &str, message: &str) {
        Self::log_entry(JournalLevel::Warn, domain, message);
    }

    /// Log an error.
    pub fn error(domain: &str, message: &str) {
        Self::log_entry(JournalLevel::Error, domain, message);
    }

    /// Log a phase start.
    pub fn phase(domain: &str, message: &str) {
        Self::log_entry(JournalLevel::Phase, domain, message);
    }

    fn log_entry(level: JournalLevel, domain: &str, message: &str) {
        if let Ok(mut guard) = global_journal().lock() {
            if let Some(ref mut journal) = *guard {
                journal.log(level, domain, message);
            }
        }
    }

    /// Get the journal file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the default journal path (`.roco/agent-journal.md` in the cwd).
    pub fn default_path() -> Result<PathBuf, String> {
        let cwd = std::env::current_dir().map_err(|e| format!("cannot get cwd: {e}"))?;
        Ok(cwd.join(".roco").join("agent-journal.md"))
    }

    /// Initialize the global journal singleton. Must be called at least once
    /// before any logging method is used. Idempotent — safe to call multiple
    /// times (subsequent calls are no-ops).
    pub fn init() -> Result<(), String> {
        let mut guard = global_journal()
            .lock()
            .map_err(|e| format!("journal lock error: {e}"))?;
        if guard.is_none() {
            *guard = Some(Self::open()?);
        }
        Ok(())
    }

    /// Check if the journal has been initialized.
    pub fn is_initialized() -> bool {
        global_journal()
            .lock()
            .ok()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    // ── Time helpers (no chrono dependency) ───────────────────────────

    /// Compute today's date as `YYYY-MM-DD` (UTC) using a civil-date
    /// algorithm that avoids chrono and time crates.
    fn today() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days = secs / 86400;
        let z = days as i64 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// Get current time as `HH:MM:SS` (UTC).
    fn timestamp() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_creates_file() {
        let dir = std::env::temp_dir().join("roco_journal_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test-journal.md");
        let mut journal = AgentJournal::open_at(path.clone()).unwrap();

        journal.log(JournalLevel::Info, "test", "test entry");
        journal.log(JournalLevel::Action, "test", "action done");
        journal.log(JournalLevel::Warn, "test", "warning");

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("INFO"));
        assert!(content.contains("ACTION"));
        assert!(content.contains("WARN"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_init_and_static_logging() {
        // Clear global for test
        if let Ok(mut guard) = global_journal().lock() {
            *guard = None;
        }

        let dir = std::env::temp_dir().join("roco_journal_static_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let journal_path = dir.join("static-test.md");
        let j = AgentJournal::open_at(journal_path.clone()).unwrap();

        if let Ok(mut guard) = global_journal().lock() {
            *guard = Some(j);
        }

        AgentJournal::info("test", "static info");
        AgentJournal::action("test", "static action");
        AgentJournal::warn("test", "static warn");
        AgentJournal::error("test", "static error");
        AgentJournal::phase("test", "static phase");

        let content = std::fs::read_to_string(&journal_path).unwrap();
        assert!(content.contains("static info"));
        assert!(content.contains("static action"));
        assert!(content.contains("static warn"));
        assert!(content.contains("static error"));
        assert!(content.contains("static phase"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_today_timestamp() {
        let today = AgentJournal::today();
        assert_eq!(today.len(), 10);
        assert_eq!(today.chars().nth(4), Some('-'));
        assert_eq!(today.chars().nth(7), Some('-'));

        let ts = AgentJournal::timestamp();
        assert_eq!(ts.len(), 8);
        assert_eq!(ts.chars().nth(2), Some(':'));
        assert_eq!(ts.chars().nth(5), Some(':'));
    }
}
