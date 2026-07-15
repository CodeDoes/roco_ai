use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::types::{HistoryEntry, HistoryKind, SessionMeta, SessionStatus};

/// File-backed session store under a `.roco/` root.
///
/// Layout:
/// ```
/// .roco/
/// ├── trace.log                      ← global trace, ALL sessions
/// └── sessions/
///     └── {session_id}/
///         ├── session.log            ← conversation turns
///         ├── trace.txt              ← raw I/O transcript
///         ├── meta.json              ← config + parent ref
///         └── history-{branch}.jsonl ← branch checkpoints
/// ```
pub struct SessionStore {
    base_path: PathBuf,
    session_dir: PathBuf,
}

impl SessionStore {
    /// Create a new store rooted at `base/.roco/`. Creates both dirs if absent.
    pub fn new<P: AsRef<Path>>(base: P) -> Result<Self, SessionError> {
        let roco = base.as_ref().join(".roco");
        let sessions = roco.join("sessions");
        fs::create_dir_all(&sessions).map_err(|e| SessionError(format!("creating sessions dir: {e}")))?;
        Ok(Self {
            base_path: roco,
            session_dir: sessions,
        })
    }

    /// Open an existing session by ID.
    pub fn open(&self, id: &str) -> Result<SessionHandle, SessionError> {
        let meta = self.read_meta(id)?;
        Ok(SessionHandle::new(self.base_path.clone(), meta))
    }

    // ── Creation ─────────────────────────────────────────────────────

    /// Create a top-level root session. Writes meta.json and initial empty log/trace.
    pub fn create_root(&self, id: &str) -> Result<(), SessionError> {
        self.ensure_dir(id)?;
        let meta = SessionMeta::new_root(id);
        self.write_meta(&meta)
    }

    /// Spawn a sub-session. Creates the child dir/meta, records a spawn in the
    /// parent's history, opens the child as the new active context, and writes
    /// the agent-switch line to both traces.
    pub fn spawn_sub<PId: AsRef<str>, SId: AsRef<str>>(
        &self,
        parent_id: PId,
        child_id: SId,
    ) -> Result<SessionHandle, SessionError> {
        // 1. Ensure child directory exists
        self.ensure_dir(child_id.as_ref())?;

        // 2. Child meta: parent points up, no children field
        let meta = SessionMeta::sub(child_id.as_ref(), parent_id.as_ref());
        self.write_meta(&meta)?;

        // 3. Parent history: record the spawn event
        self.record_spawn(parent_id.as_ref(), child_id.as_ref(), None)?;

        // 4. Both sides log the agent switch
        let switch = format!("\n--- agent_switch: {} ---\n", child_id.as_ref());
        self.append_trace(parent_id.as_ref(), &switch)?;
        self.append_trace(child_id.as_ref(), &switch)?;

        // 5. Return handle scoped to child (next write ops target child)
        Ok(SessionHandle::new(self.base_path.clone(), meta))
    }

    /// Join a child sub-session back into its parent. Records the join in the
    /// parent's history, closes the child meta, and logs the join marker.
    pub fn join_back<SId: AsRef<str>, Pid: AsRef<str>>(
        &self,
        child_id: SId,
        parent_id: Pid,
        summary: &str,
    ) -> Result<(), SessionError> {
        // 1. Record join in parent's history
        self.record_join(parent_id.as_ref(), child_id.as_ref(), summary)?;

        // 2. Mark child as finished
        self.update_meta(child_id.as_ref(), |m| m.finish())?;

        // 3. Both sides log the agent join
        let join = format!(
            "\n\n=== JOIN BACK from {} ===\n{}",
            child_id.as_ref(),
            summary.split('\n').nth(0).unwrap_or(summary)
        );
        self.append_trace(parent_id.as_ref(), &join)?;
        self.append_trace(child_id.as_ref(), &join)?;

        Ok(())
    }

    // ── Logging ──────────────────────────────────────────────────────

    /// Append a conversation turn to a session's `session.log`.
    /// Updated after every message generation or ingestion.
    pub fn log_conversation<S: AsRef<str>>(&self, session_id: S, text: &str) -> Result<(), SessionError> {
        let sess = self.ensure_dir(session_id.as_ref())?;
        let path = sess.join("session.log");
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing session.log: {e}")))?;
        writeln!(f, "{text}")
            .map_err(|e| SessionError(format!("flushing session.log: {e}")))?;
        Ok(())
    }

    /// Stream a line into a session's `trace.txt`.
    /// Written as close to pure input/output as possible — what was sent and received.
    pub fn log_trace<S: AsRef<str>>(&self, session_id: S, text: &str) -> Result<(), SessionError> {
        let sess = self.ensure_dir(session_id.as_ref())?;
        let path = sess.join("trace.txt");
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing trace.txt: {e}")))?;
        writeln!(f, "{text}")
            .map_err(|e| SessionError(format!("flushing trace.txt: {e}")))?;
        Ok(())
    }

    /// Write an event to the global trace log (`trace.log`).
    /// Appended alongside each session's own trace.
    pub fn log_global<E: Serialize>(&self, event: &E) -> Result<(), SessionError> {
        let path = self.base_path.join("trace.log");
        let line = serde_json::to_string(event).unwrap_or_default();
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing global trace.log: {e}")))?;
        writeln!(f, "{line}")
            .map_err(|e| SessionError(format!("flushing trace.log: {e}")))?;
        Ok(())
    }

    /// Switch the active agent context. Logs the switch in both source and
    /// destination traces. This is called internally by `spawn_sub` but is
    /// also exposed for cases like agent restart mid-session.
    pub fn switch_agent<SFrom: AsRef<str>, SDest: AsRef<str>>(
        &self,
        from: SFrom,
        dest: SDest,
    ) -> Result<(), SessionError> {
        let switch = format!("\n--- agent_switch: {} ---\n", dest.as_ref());
        self.append_trace(from.as_ref(), &switch)?;
        self.append_trace(dest.as_ref(), &switch)?;
        Ok(())
    }

    // ── Branches / History ───────────────────────────────────────────

    /// Take a snapshot of a session at the current point and start a new branch.
    /// Writes a checkpoint to `history-{branch}.jsonl` and sets active_branch.
    pub fn branch<S: AsRef<str>, B: AsRef<str>>(
        &self,
        session_id: S,
        branch: B,
        child_session: Option<&str>,
    ) -> Result<(), SessionError> {
        let sess = self.ensure_dir(session_id.as_ref())?;
        let hist = sess.join(format!("history-{}.jsonl", branch.as_ref()));

        let entry = HistoryEntry {
            ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            kind: HistoryKind::Spawned,
            child_session: child_session.map(String::from),
            merge_info: None,
        };

        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&hist)
            .map_err(|e| SessionError(format!("writing history: {e}")))?;
        writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default())
            .map_err(|e| SessionError(format!("flushing history: {e}")))?;

        // Update active branch in meta
        self.update_meta(session_id.as_ref(), |m| {
            m.active_branch = Some(branch.as_ref().to_string());
            m.updated();
        })?;

        Ok(())
    }

    /// Record that a branch has joined back into the parent.
    pub fn record_merge<S: AsRef<str>, B: AsRef<str>>(
        &self,
        session_id: S,
        branch: B,
        summary: &str,
    ) -> Result<(), SessionError> {
        let sess = self.ensure_dir(session_id.as_ref())?;
        let hist = sess.join(format!("history-{}.jsonl", branch.as_ref()));

        let entry = HistoryEntry {
            ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            kind: HistoryKind::Joined,
            child_session: None,
            merge_info: Some(summary.to_string()),
        };

        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&hist)
            .map_err(|e| SessionError(format!("writing history merge: {e}")))?;
        writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default())
            .map_err(|e| SessionError(format!("flushing history merge: {e}")))?;

        // Clear active branch
        self.update_meta(session_id.as_ref(), |m| {
            m.active_branch = None;
            m.status = SessionStatus::Joined;
            m.updated();
        })?;

        Ok(())
    }

    // ── Helpers ──────────────────────────────────────────────────────

    fn ensure_dir(&self, id: &str) -> Result<PathBuf, SessionError> {
        let dir = self.session_dir.join(id);
        fs::create_dir_all(&dir).map_err(|e| SessionError(format!("ensuring session dir: {e}")))?;
        Ok(dir)
    }

    fn read_meta(&self, id: &str) -> Result<SessionMeta, SessionError> {
        let path = self.session_dir.join(id).join("meta.json");
        let content = std::fs::read_to_string(&path).map_err(|e| SessionError(format!("reading meta.json: {e}")))?;
        serde_json::from_str(&content).map_err(|e| SessionError(format!("parsing meta.json: {e}")))
    }

    fn write_meta(&self, meta: &SessionMeta) -> Result<(), SessionError> {
        let path = self.session_dir.join(&meta.session_id).join("meta.json");
        let content = serde_json::to_string_pretty(meta).unwrap_or_default();
        fs::write(&path, content)
            .map_err(|e| SessionError(format!("writing meta.json: {e}")))?;
        Ok(())
    }

    fn update_meta<S: AsRef<str>, F: FnOnce(&mut SessionMeta)>(&self, id: S, f: F) -> Result<(), SessionError> {
        let mut meta = self.read_meta(id.as_ref())?;
        f(&mut meta);
        self.write_meta(&meta)
    }

    fn append_trace<S: AsRef<str>>(&self, id: S, text: &str) -> Result<(), SessionError> {
        let dir = self.ensure_dir(id.as_ref())?;
        let path = dir.join("trace.txt");
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing trace: {e}")))?;
        write!(f, "{text}").map_err(|e| SessionError(format!("flushing trace: {e}")))?;
        Ok(())
    }

    fn record_spawn(&self, parent_id: &str, child_id: &str, merge: Option<&str>) -> Result<(), SessionError> {
        let entry = HistoryEntry {
            ts: current_ts(),
            kind: HistoryKind::Spawned,
            child_session: Some(child_id.to_string()),
            merge_info: merge.map(String::from),
        };
        let path = self.session_dir.join(parent_id).join(format!("history-{}.jsonl", child_id));
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing spawn history: {e}")))?;
        writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default())
            .map_err(|e| SessionError(format!("flushing spawn history: {e}")))?;
        Ok(())
    }

    fn record_join(&self, parent_id: &str, child_id: &str, info: &str) -> Result<(), SessionError> {
        let entry = HistoryEntry {
            ts: current_ts(),
            kind: HistoryKind::Joined,
            child_session: Some(child_id.to_string()),
            merge_info: Some(info.to_string()),
        };
        let path = self.session_dir.join(parent_id).join(format!("history-{}.jsonl", child_id));
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| SessionError(format!("writing join history: {e}")))?;
        writeln!(f, "{}", serde_json::to_string(&entry).unwrap_or_default())
            .map_err(|e| SessionError(format!("flushing join history: {e}")))?;
        Ok(())
    }
}

/// A handle scoped to a specific session. All write operations target
/// that session's directory — convenient when the agent is executing
/// inside a sub-session context.
#[derive(Debug)]
pub struct SessionHandle {
    base_path: PathBuf,
    meta: SessionMeta,
}

impl SessionHandle {
    fn new(base_path: PathBuf, meta: SessionMeta) -> Self {
        Self { base_path, meta }
    }

    /// Returns the session metadata.
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// Returns the path to this session's directory.
    pub fn path(&self) -> PathBuf {
        self.base_path.join("sessions").join(&self.meta.session_id)
    }

    /// Convenience: record a conversation turn.
    pub fn log_conversation(&self, text: &str) -> Result<(), SessionError> {
        let guard = SessionStore::global();
        guard.as_ref().unwrap().log_conversation(&self.meta.session_id, text)
    }

    /// Convenience: stream a trace line.
    pub fn log_trace(&self, text: &str) -> Result<(), SessionError> {
        let guard = SessionStore::global();
        guard.as_ref().unwrap().log_trace(&self.meta.session_id, text)
    }
}

// Thread-safe singleton for global log operations.
// Only ever used when one agent is active — no locking needed.
static GLOBAL_STORE: std::sync::LazyLock<std::sync::Mutex<Option<SessionStore>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

impl SessionStore {
    /// Initialize the global store once. Called early during agent startup.
    pub fn init_global<P: AsRef<Path>>(base: P) -> Result<(), SessionError> {
        let store = SessionStore::new(base)?;
        *GLOBAL_STORE.lock().unwrap() = Some(store);
        Ok(())
    }

    /// Borrow the global store for writing events to `trace.log`.
    pub(crate) fn global() -> std::sync::MutexGuard<'static, Option<SessionStore>> {
        GLOBAL_STORE.lock().expect("global store poisoned")
    }
}

fn current_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct SessionError(pub String);

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session error: {}", self.0)
    }
}

impl std::error::Error for SessionError {}
