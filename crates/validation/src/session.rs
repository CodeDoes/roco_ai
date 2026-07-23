//! Story session management — session lifecycle, persistence, and resumption.
//!
//! # Architecture
//!
//! `StorySession` — a single session pinned to one story workspace.
//! `StorySessionManager` — manages the active session, history, and persistence.
//!
//! Sessions are persisted to `~/.roco/story_sessions.json` so they survive
//! across CLI restarts.
//!
//! # Usage
//!
//! ```ignore
//! let mut manager = StorySessionManager::new();
//! manager.lock("my-fantasy-story")?;
//!
//! if let Some(session) = manager.active_session() {
//!     println!("Working on: {}", session.story_name);
//! }
//!
//! manager.unlock();
//! manager.save()?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::tool_set::StoryToolSet;

// ═════════════════════════════════════════════════════════════════════════════
// StorySession
// ═════════════════════════════════════════════════════════════════════════════

/// A session pinned to a specific story workspace.
///
/// Owns the `StoryToolSet` for filesystem operations and caches chapter/wiki
/// content to avoid repeated disk reads. The cache is invalidated on write.
#[derive(Debug, Clone)]
pub struct StorySession {
    /// Name of this story (used for display and session lookup).
    pub story_name: String,
    /// Path to the workspace directory.
    workspace_path: PathBuf,
    /// Filesystem tool set for this workspace.
    pub tool_set: StoryToolSet,
    /// Snapshot of the outline at session start (for diffing).
    pub outline_snapshot: Option<String>,
    /// Cached chapter content (chapter_num → content).
    chapter_cache: HashMap<usize, String>,
    /// Cached wiki content.
    wiki_cache: String,
    /// When this session was created.
    created_at: u64,
    /// When this session was last active.
    last_active: u64,
}

impl StorySession {
    /// Create a new session for a story workspace.
    ///
    /// The `story_name` is typically the directory name of the workspace.
    /// The `workspace_path` should point to a `.roco/workspaces/<name>/` directory.
    pub fn new(story_name: String, workspace_path: PathBuf) -> Result<Self, String> {
        if !workspace_path.exists() {
            return Err(format!(
                "Workspace path does not exist: {}",
                workspace_path.display()
            ));
        }

        let tool_set = StoryToolSet::new(&workspace_path);
        let outline_snapshot = tool_set.read_outline().ok();
        let wiki_cache = tool_set.read_wiki().unwrap_or_default();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(Self {
            story_name,
            workspace_path,
            tool_set,
            outline_snapshot,
            chapter_cache: HashMap::new(),
            wiki_cache,
            created_at: now,
            last_active: now,
        })
    }

    /// Get the total word count across all chapters.
    pub fn total_word_count(&self) -> usize {
        self.tool_set.total_word_count()
    }

    /// Get the number of chapters.
    pub fn chapter_count(&self) -> usize {
        self.tool_set.count_chapters()
    }

    /// Mark this session as recently active.
    pub fn touch(&mut self) {
        self.last_active = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get the session age in seconds.
    pub fn age_seconds(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.created_at)
    }

    /// Invalidate all caches (called after writes).
    pub fn invalidate_cache(&mut self) {
        self.chapter_cache.clear();
        self.wiki_cache = self.tool_set.read_wiki().unwrap_or_default();
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Persistent session data (for serialization)
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistentSession {
    story_name: String,
    workspace_path: String,
    created_at: u64,
    last_active: u64,
}

// ═════════════════════════════════════════════════════════════════════════════
// StorySessionManager
// ═════════════════════════════════════════════════════════════════════════════

/// Manages the active story session and session history.
///
/// Persists session data to `~/.roco/story_sessions.json` so the last
/// active session can be resumed across CLI restarts.
pub struct StorySessionManager {
    /// The currently active session (None = default mode).
    active_session: Option<StorySession>,
    /// History of recent sessions (most recent first).
    session_history: Vec<(String, u64)>, // (story_name, last_active_timestamp)
    /// Path to the persistence file.
    persistence_path: PathBuf,
}

impl Default for StorySessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StorySessionManager {
    /// Create a new session manager.
    ///
    /// Automatically loads persisted sessions from `~/.roco/story_sessions.json`.
    pub fn new() -> Self {
        let persistence_path = Self::default_persistence_path();
        let history = Self::load_history(&persistence_path);

        Self {
            active_session: None,
            session_history: history,
            persistence_path,
        }
    }

    /// Create a session manager with a custom persistence path.
    pub fn with_persistence_path(path: PathBuf) -> Self {
        let history = Self::load_history(&path);
        Self {
            active_session: None,
            session_history: history,
            persistence_path: path,
        }
    }

    /// Get the default persistence path.
    fn default_persistence_path() -> PathBuf {
        let mut path = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".roco");
        path.push("story_sessions.json");
        path
    }

    /// Load session history from disk.
    fn load_history(path: &Path) -> Vec<(String, u64)> {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(sessions) = serde_json::from_str::<Vec<PersistentSession>>(&content) {
                    let mut history: Vec<(String, u64)> = sessions
                        .into_iter()
                        .map(|s| (s.story_name, s.last_active))
                        .collect();
                    history.sort_by(|a, b| b.1.cmp(&a.1)); // Most recent first
                    return history;
                }
            }
        }
        Vec::new()
    }

    /// Save session history to disk.
    pub fn save(&self) -> Result<(), String> {
        // Collect all persistent sessions from history + active
        let mut sessions: Vec<PersistentSession> = self
            .session_history
            .iter()
            .map(|(name, last_active)| {
                // Check if we have a workspace path for this session
                let workspace_path = if let Some(ref active) = self.active_session {
                    if active.story_name == *name {
                        active.workspace_path.to_string_lossy().to_string()
                    } else {
                        Self::default_workspace_path(name)
                    }
                } else {
                    Self::default_workspace_path(name)
                };

                PersistentSession {
                    story_name: name.clone(),
                    workspace_path,
                    created_at: 0, // Reconstructed on load
                    last_active: *last_active,
                }
            })
            .collect();

        // Include active session if not in history
        if let Some(ref active) = self.active_session {
            if !sessions.iter().any(|s| s.story_name == active.story_name) {
                sessions.push(PersistentSession {
                    story_name: active.story_name.clone(),
                    workspace_path: active.workspace_path.to_string_lossy().to_string(),
                    created_at: active.created_at,
                    last_active: active.last_active,
                });
            }
        }

        // Ensure directory exists
        if let Some(parent) = self.persistence_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create persistence directory: {e}"))?;
        }

        let content = serde_json::to_string_pretty(&sessions)
            .map_err(|e| format!("Failed to serialize sessions: {e}"))?;

        std::fs::write(&self.persistence_path, &content)
            .map_err(|e| format!("Failed to write sessions: {e}"))?;

        Ok(())
    }

    /// Default workspace path for a story name.
    fn default_workspace_path(name: &str) -> String {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(".roco")
            .join("workspaces")
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    /// Lock into a story workspace by name.
    ///
    /// Creates a new session for the named story. The workspace directory
    /// must exist.
    pub fn lock(&mut self, name: &str) -> Result<(), String> {
        let workspace_path = Self::default_workspace_path(name);
        let session = StorySession::new(name.to_string(), PathBuf::from(&workspace_path))?;

        // Update history: move this story to front
        self.session_history.retain(|(n, _)| n != name);
        self.session_history
            .insert(0, (name.to_string(), session.last_active));

        self.active_session = Some(session);
        self.save().ok();
        Ok(())
    }

    /// Unlock from the current story and return to default mode.
    pub fn unlock(&mut self) {
        if let Some(ref session) = self.active_session {
            // Update history with last active time
            self.session_history
                .retain(|(n, _)| n != &session.story_name);
            self.session_history
                .insert(0, (session.story_name.clone(), session.last_active));
        }
        self.active_session = None;
        self.save().ok();
    }

    /// Switch to a different story workspace.
    pub fn switch(&mut self, name: &str) -> Result<(), String> {
        self.unlock();
        self.lock(name)
    }

    /// Resume the last active story.
    ///
    /// Returns `None` if no previous session exists or the workspace
    /// directory is missing.
    pub fn resume_last(&mut self) -> Option<&StorySession> {
        let name = self.session_history.first().map(|(n, _)| n.clone())?;
        self.lock(&name).ok()?;
        self.active_session.as_ref()
    }

    /// Get the active session, if any.
    pub fn active_session(&self) -> Option<&StorySession> {
        self.active_session.as_ref()
    }

    /// Get a mutable reference to the active session.
    pub fn active_session_mut(&mut self) -> Option<&mut StorySession> {
        self.active_session.as_mut()
    }

    /// Get the name of the active story, if any.
    pub fn active_session_name(&self) -> Option<&str> {
        self.active_session.as_ref().map(|s| s.story_name.as_str())
    }

    /// List all known story names (from history).
    pub fn list_stories(&self) -> Vec<String> {
        let mut stories: Vec<String> = self
            .session_history
            .iter()
            .map(|(n, _)| n.clone())
            .collect();
        stories.sort();
        stories.dedup();
        stories
    }

    /// Get session history (most recent first).
    pub fn history(&self) -> &[(String, u64)] {
        &self.session_history
    }

    /// Whether a session is currently active.
    pub fn has_active_session(&self) -> bool {
        self.active_session.is_some()
    }

    /// Touch the active session (update last_active timestamp).
    pub fn touch_active(&mut self) {
        if let Some(ref mut session) = self.active_session {
            session.touch();
        }
    }
}

impl Drop for StorySessionManager {
    fn drop(&mut self) {
        self.save().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_workspace(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let ws_path = dir.path().join(name);
        fs::create_dir_all(&ws_path).unwrap();
        fs::write(ws_path.join("outline.md"), "Title: Test").unwrap();
        fs::create_dir_all(ws_path.join("chapters")).unwrap();
        fs::write(ws_path.join("chapters/01-chapter.md"), "Chapter 1").unwrap();
        dir
    }

    #[test]
    fn test_session_new() {
        let dir = setup_test_workspace("test-story");
        let ws_path = dir.path().join("test-story");
        let session = StorySession::new("test-story".to_string(), ws_path).unwrap();
        assert_eq!(session.story_name, "test-story");
    }

    #[test]
    fn test_session_new_nonexistent() {
        let result = StorySession::new("nope".to_string(), PathBuf::from("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_session_word_count() {
        let dir = setup_test_workspace("test");
        let ws_path = dir.path().join("test");
        let session = StorySession::new("test".to_string(), ws_path).unwrap();
        assert_eq!(session.total_word_count(), 2); // "Chapter 1" = 2 words
    }

    #[test]
    fn test_manager_lock_and_unlock() {
        let dir = setup_test_workspace("fantasy");
        let ws_path = dir.path().join("fantasy");

        // Override default path resolution by creating a manager
        // that uses a temp persistence file
        let tmp_persistence = dir.path().join("sessions.json");
        let mut manager = StorySessionManager::with_persistence_path(tmp_persistence);

        // Manually create and set session
        let session = StorySession::new("fantasy".to_string(), ws_path.clone()).unwrap();
        manager.active_session = Some(session);
        assert!(manager.has_active_session());

        manager.unlock();
        assert!(!manager.has_active_session());
    }

    #[test]
    fn test_manager_history() {
        let manager = StorySessionManager::new();
        assert!(manager.list_stories().is_empty());
    }

    #[test]
    fn test_session_touch() {
        let dir = setup_test_workspace("test");
        let ws_path = dir.path().join("test");
        let mut session = StorySession::new("test".to_string(), ws_path).unwrap();
        let old = session.last_active;
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();
        assert!(session.last_active >= old);
    }

    #[test]
    fn test_session_cache_invalidation() {
        let dir = setup_test_workspace("test");
        let ws_path = dir.path().join("test");
        let mut session = StorySession::new("test".to_string(), ws_path).unwrap();
        // Write directly to change wiki
        fs::write(session.tool_set.wiki_path(), "New wiki content").unwrap();
        session.invalidate_cache();
        assert_eq!(session.wiki_cache, "New wiki content");
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = setup_test_workspace("persist-test");
        let ws_path = dir.path().join("persist-test");
        let tmp_persistence = dir.path().join("session_persist.json");

        let mut manager = StorySessionManager::with_persistence_path(tmp_persistence.clone());

        // Manually create and set session
        let session = StorySession::new("persist-test".to_string(), ws_path).unwrap();
        manager.active_session = Some(session);
        manager.save().unwrap();

        // Reload
        let loaded_manager = StorySessionManager::with_persistence_path(tmp_persistence);
        let stories = loaded_manager.list_stories();
        assert!(stories.contains(&"persist-test".to_string()));
    }
}
