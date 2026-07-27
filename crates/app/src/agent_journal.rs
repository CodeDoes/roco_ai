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

/// Rotate the journal once it exceeds this size (8 MiB).
///
/// The journal is append-only and every `roco` invocation writes to it, so
/// without rotation `.roco/agent-journal.md` grows without bound — a genuine
/// storage leak on a machine used daily. Override with `$ROCO_JOURNAL_MAX_MB`.
const DEFAULT_MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;

/// Longest single message written verbatim. Longer entries are truncated with
/// an ellipsis, so one runaway prompt dump cannot add megabytes in a single
/// call.
const MAX_ENTRY_CHARS: usize = 4_000;

/// Number of rotated journals kept (`agent-journal.md.1`).
const ROTATION_KEEP: u32 = 1;

fn max_journal_bytes() -> u64 {
    std::env::var("ROCO_JOURNAL_MAX_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|mb| *mb > 0)
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(DEFAULT_MAX_JOURNAL_BYTES)
}

/// Agent journal — appends structured entries to `.roco/agent-journal.md`.
///
/// Thread-safe. Multiple components can log concurrently. The file is rotated
/// once it passes [`max_journal_bytes`] so it cannot grow indefinitely.
pub struct AgentJournal {
    path: PathBuf,
    file: fs::File,
    /// Bytes written since the last size check, to avoid `stat`-ing on every
    /// entry (the journal is on the hot path of every CLI command).
    since_check: u64,
    max_bytes: u64,
}

impl AgentJournal {
    /// Open (or create) the agent journal at the default location.
    pub fn open() -> Result<Self, String> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open (or create) the agent journal at a specific path.
    pub fn open_at(path: PathBuf) -> Result<Self, String> {
        Self::open_with_limit(path, max_journal_bytes())
    }

    /// Open with an explicit size limit (used by tests).
    pub fn open_with_limit(path: PathBuf, max_bytes: u64) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot create journal dir: {e}"))?;
        }

        let file = Self::open_append(&path)?;

        // Write header if file is empty
        let meta = file
            .metadata()
            .map_err(|e| format!("cannot stat journal: {e}"))?;
        if meta.len() == 0 {
            let today = Self::today();
            writeln!(&file, "# Agent Journal — {today}").ok();
            writeln!(&file).ok();
        }

        Ok(Self {
            path,
            file,
            since_check: 0,
            max_bytes: max_bytes.max(1),
        })
    }

    fn open_append(path: &PathBuf) -> Result<fs::File, String> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("cannot open journal: {e}"))
    }

    /// Write a timestamped entry to the journal.
    pub fn log(&mut self, level: JournalLevel, domain: &str, message: &str) {
        let now = Self::timestamp();
        let emoji = level.as_emoji();
        let level_name = format!("{:?}", level).to_uppercase();
        let message = Self::truncate(message);

        let entry = format!("## {now}\n\n{emoji} **{level_name}** ({domain}): {message}\n\n");
        if write!(&self.file, "{entry}").is_ok() {
            self.since_check += entry.len() as u64;
        }
        let _ = self.file.flush();

        // Only stat once per ~64 KiB written.
        if self.since_check >= 64 * 1024 {
            self.since_check = 0;
            self.rotate_if_needed();
        }
    }

    /// Clamp an entry so a single call can't append megabytes.
    fn truncate(message: &str) -> std::borrow::Cow<'_, str> {
        if message.chars().count() <= MAX_ENTRY_CHARS {
            return std::borrow::Cow::Borrowed(message);
        }
        let kept: String = message.chars().take(MAX_ENTRY_CHARS).collect();
        std::borrow::Cow::Owned(format!("{kept}… [truncated]"))
    }

    /// Rename the journal aside and start a fresh one when it gets too big.
    pub fn rotate_if_needed(&mut self) -> bool {
        let too_big = self
            .file
            .metadata()
            .map(|m| m.len() > self.max_bytes)
            .unwrap_or(false);
        if !too_big {
            return false;
        }

        let rotated = self.path.with_extension("md.1");
        // Best-effort: a failed rotation must never break logging.
        let _ = fs::remove_file(&rotated);
        if fs::rename(&self.path, &rotated).is_err() {
            return false;
        }
        // Drop older generations beyond what we keep.
        for n in (ROTATION_KEEP + 1)..(ROTATION_KEEP + 4) {
            let _ = fs::remove_file(self.path.with_extension(format!("md.{n}")));
        }

        match Self::open_append(&self.path) {
            Ok(file) => {
                self.file = file;
                let today = Self::today();
                writeln!(&self.file, "# Agent Journal — {today}").ok();
                writeln!(
                    &self.file,
                    "\n_(rotated; previous log: {})_\n",
                    rotated.display()
                )
                .ok();
                let _ = self.file.flush();
                true
            }
            Err(_) => false,
        }
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

    // ── Rotation (storage leak regression) ───────────────────────────────

    #[test]
    fn journal_rotates_instead_of_growing_forever() {
        let dir = std::env::temp_dir().join("roco_journal_rotate_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rot.md");

        // 4 KiB limit so the test is fast.
        let mut journal = AgentJournal::open_with_limit(path.clone(), 4 * 1024).unwrap();
        for i in 0..2_000 {
            journal.log(JournalLevel::Info, "test", &format!("entry number {i}"));
        }

        let live = std::fs::metadata(&path).unwrap().len();
        assert!(
            live <= 4 * 1024 + 64 * 1024,
            "journal grew unbounded: {live} bytes"
        );
        assert!(
            path.with_extension("md.1").exists(),
            "a rotated journal should exist"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_one_rotated_generation_is_kept() {
        let dir = std::env::temp_dir().join("roco_journal_gen_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gen.md");

        let mut journal = AgentJournal::open_with_limit(path.clone(), 2 * 1024).unwrap();
        for i in 0..5_000 {
            journal.log(JournalLevel::Info, "test", &format!("e{i}"));
        }

        assert!(!path.with_extension("md.2").exists(), "no .2 generation");
        assert!(!path.with_extension("md.3").exists(), "no .3 generation");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_entries_are_truncated() {
        let huge = "x".repeat(MAX_ENTRY_CHARS * 5);
        let out = AgentJournal::truncate(&huge);
        assert!(out.chars().count() < MAX_ENTRY_CHARS + 32);
        assert!(out.ends_with("[truncated]"));

        // Short messages are borrowed untouched.
        assert_eq!(AgentJournal::truncate("hi").as_ref(), "hi");
    }

    #[test]
    fn multibyte_entries_truncate_on_char_boundaries() {
        let huge = "é".repeat(MAX_ENTRY_CHARS * 2);
        // Would panic if we sliced by bytes.
        let out = AgentJournal::truncate(&huge);
        assert!(out.ends_with("[truncated]"));
    }

    #[test]
    fn max_journal_bytes_respects_the_env_override() {
        std::env::set_var("ROCO_JOURNAL_MAX_MB", "3");
        assert_eq!(max_journal_bytes(), 3 * 1024 * 1024);
        std::env::set_var("ROCO_JOURNAL_MAX_MB", "not-a-number");
        assert_eq!(max_journal_bytes(), DEFAULT_MAX_JOURNAL_BYTES);
        std::env::set_var("ROCO_JOURNAL_MAX_MB", "0");
        assert_eq!(max_journal_bytes(), DEFAULT_MAX_JOURNAL_BYTES);
        std::env::remove_var("ROCO_JOURNAL_MAX_MB");
    }
}
