//! Reversibility — undo/redo and version control for agent actions.
//!
//! Every action the agent takes is reversible:
//! - Workspace snapshots before file changes
//! - Action history with undo/redo support
//! - Rollback to any previous state
//! - Git-like versioning for story state

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Version Control
// ═════════════════════════════════════════════════════════════════════════════

/// A snapshot of the workspace at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub snapshot_id: String,
    pub timestamp: u64,
    pub description: String,
    pub files: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
}

/// An action that can be undone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReversibleAction {
    pub action_id: String,
    pub timestamp: u64,
    pub action_type: String,
    pub description: String,
    pub forward_payload: String,
    pub backward_payload: String,
    pub snapshot_before: Option<String>,
    pub snapshot_after: Option<String>,
}

/// Version control system for workspace
pub struct VersionControl {
    /// All snapshots
    snapshots: Mutex<Vec<Snapshot>>,
    /// Action history (for undo/redo)
    action_history: Mutex<Vec<ReversibleAction>>,
    /// Current position in action history
    current_position: Mutex<usize>,
    /// Workspace root
    workspace_root: PathBuf,
    /// Snapshot storage directory
    snapshot_dir: PathBuf,
}

impl VersionControl {
    /// Create a new version control system
    pub fn new(workspace_root: PathBuf) -> Self {
        let snapshot_dir = workspace_root.join(".snapshots");
        std::fs::create_dir_all(&snapshot_dir).ok();

        Self {
            snapshots: Mutex::new(Vec::new()),
            action_history: Mutex::new(Vec::new()),
            current_position: Mutex::new(0),
            workspace_root,
            snapshot_dir,
        }
    }

    /// Take a snapshot of the current workspace state
    pub fn snapshot(&self, description: &str) -> Result<String, String> {
        // Use a monotonic counter + nanosecond timestamp so two snapshots
        // taken in the same wall-clock second get distinct IDs and don't
        // overwrite each other on disk.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let snapshot_id = format!("snap_{}_{}", nanos, n);
        let mut files = HashMap::new();

        // Read all files in workspace
        for entry in std::fs::read_dir(&self.workspace_root)
            .map_err(|e| format!("failed to read workspace: {e}"))?
        {
            let entry = entry.map_err(|e| format!("failed to read entry: {e}"))?;
            let path = entry.path();

            if path.is_file() {
                let relative = path
                    .strip_prefix(&self.workspace_root)
                    .map_err(|e| format!("failed to get relative path: {e}"))?;
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("failed to read file: {e}"))?;
                files.insert(relative.to_string_lossy().to_string(), content);
            }
        }

        let snapshot = Snapshot {
            snapshot_id: snapshot_id.clone(),
            timestamp: now(),
            description: description.to_string(),
            files,
            metadata: HashMap::new(),
        };

        // Save snapshot to disk
        let path = self.snapshot_dir.join(format!("{}.json", snapshot_id));
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| format!("failed to serialize snapshot: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("failed to write snapshot: {e}"))?;

        // Add to history
        self.snapshots.lock().unwrap().push(snapshot);

        Ok(snapshot_id)
    }

    /// Record a reversible action
    pub fn record_action(
        &self,
        action_type: &str,
        description: &str,
        forward_payload: &str,
        backward_payload: &str,
    ) -> Result<String, String> {
        let action_id = format!("action_{}", now());

        let action = ReversibleAction {
            action_id: action_id.clone(),
            timestamp: now(),
            action_type: action_type.to_string(),
            description: description.to_string(),
            forward_payload: forward_payload.to_string(),
            backward_payload: backward_payload.to_string(),
            snapshot_before: None,
            snapshot_after: None,
        };

        let mut history = self.action_history.lock().unwrap();
        let mut position = self.current_position.lock().unwrap();

        // Truncate any actions after current position (they're now invalid)
        history.truncate(*position);

        // Add new action
        history.push(action);
        *position = history.len();

        Ok(action_id)
    }

    /// Undo the last action
    pub fn undo(&self) -> Result<Option<ReversibleAction>, String> {
        let history = self.action_history.lock().unwrap();
        let mut position = self.current_position.lock().unwrap();

        if *position == 0 {
            return Ok(None);
        }

        *position -= 1;
        let action = history[*position].clone();

        // Apply backward payload
        self.apply_payload(&action.backward_payload)?;

        Ok(Some(action))
    }

    /// Redo the last undone action
    pub fn redo(&self) -> Result<Option<ReversibleAction>, String> {
        let history = self.action_history.lock().unwrap();
        let mut position = self.current_position.lock().unwrap();

        if *position >= history.len() {
            return Ok(None);
        }

        let action = history[*position].clone();
        *position += 1;

        // Apply forward payload
        self.apply_payload(&action.forward_payload)?;

        Ok(Some(action))
    }

    /// Rollback to a specific snapshot
    pub fn rollback(&self, snapshot_id: &str) -> Result<(), String> {
        let snapshots = self.snapshots.lock().unwrap();
        let snapshot = snapshots
            .iter()
            .find(|s| s.snapshot_id == snapshot_id)
            .ok_or_else(|| format!("snapshot {} not found", snapshot_id))?;

        // Clear workspace
        for entry in std::fs::read_dir(&self.workspace_root)
            .map_err(|e| format!("failed to read workspace: {e}"))?
        {
            let entry = entry.map_err(|e| format!("failed to read entry: {e}"))?;
            let path = entry.path();
            if path.is_file() {
                std::fs::remove_file(&path).map_err(|e| format!("failed to remove file: {e}"))?;
            }
        }

        // Restore files from snapshot
        for (relative, content) in &snapshot.files {
            let path = self.workspace_root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create directory: {e}"))?;
            }
            std::fs::write(&path, content).map_err(|e| format!("failed to write file: {e}"))?;
        }

        Ok(())
    }

    /// Get list of all snapshots
    pub fn list_snapshots(&self) -> Vec<SnapshotSummary> {
        self.snapshots
            .lock()
            .unwrap()
            .iter()
            .map(|s| SnapshotSummary {
                snapshot_id: s.snapshot_id.clone(),
                timestamp: s.timestamp,
                description: s.description.clone(),
                file_count: s.files.len(),
            })
            .collect()
    }

    /// Get action history
    pub fn action_history(&self) -> Vec<ReversibleAction> {
        self.action_history.lock().unwrap().clone()
    }

    /// Get current position in history
    pub fn current_position(&self) -> usize {
        *self.current_position.lock().unwrap()
    }

    /// Apply a payload (forward or backward)
    fn apply_payload(&self, payload: &str) -> Result<(), String> {
        // Payload format: JSON with file operations
        // { "operations": [{ "type": "write", "path": "...", "content": "..." }, ...] }
        let ops: PayloadOps =
            serde_json::from_str(payload).map_err(|e| format!("failed to parse payload: {e}"))?;

        for op in ops.operations {
            match op.op_type.as_str() {
                "write" => {
                    let path = self.workspace_root.join(&op.path);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("failed to create directory: {e}"))?;
                    }
                    std::fs::write(&path, &op.content)
                        .map_err(|e| format!("failed to write file: {e}"))?;
                }
                "delete" => {
                    let path = self.workspace_root.join(&op.path);
                    if path.exists() {
                        std::fs::remove_file(&path)
                            .map_err(|e| format!("failed to delete file: {e}"))?;
                    }
                }
                _ => {
                    return Err(format!("unknown operation type: {}", op.op_type));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadOps {
    operations: Vec<PayloadOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PayloadOp {
    op_type: String,
    path: String,
    content: String,
}

/// Summary of a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub snapshot_id: String,
    pub timestamp: u64,
    pub description: String,
    pub file_count: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═════════════════════════════════════════════════════════════════════════════

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Create a forward payload for writing a file
pub fn write_payload(path: &str, content: &str) -> String {
    let ops = PayloadOps {
        operations: vec![PayloadOp {
            op_type: "write".into(),
            path: path.into(),
            content: content.into(),
        }],
    };
    serde_json::to_string(&ops).unwrap_or_default()
}

/// Create a backward payload for deleting a file
pub fn delete_payload(path: &str) -> String {
    let ops = PayloadOps {
        operations: vec![PayloadOp {
            op_type: "delete".into(),
            path: path.into(),
            content: String::new(),
        }],
    };
    serde_json::to_string(&ops).unwrap_or_default()
}

/// Create a backward payload for writing a file (undo of delete)
pub fn restore_payload(path: &str, content: &str) -> String {
    write_payload(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_control() {
        let temp_dir = std::env::temp_dir().join("roco_test_version_control");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let vc = VersionControl::new(temp_dir.clone());

        // Take initial snapshot
        let snap1 = vc.snapshot("initial").unwrap();

        // Write a file
        std::fs::write(temp_dir.join("test.txt"), "hello").unwrap();

        // Record action
        vc.record_action(
            "write",
            "write test.txt",
            &write_payload("test.txt", "hello"),
            &delete_payload("test.txt"),
        )
        .unwrap();

        // Take another snapshot
        let _snap2 = vc.snapshot("after write").unwrap();

        // Write another file
        std::fs::write(temp_dir.join("test2.txt"), "world").unwrap();

        // Record action
        vc.record_action(
            "write",
            "write test2.txt",
            &write_payload("test2.txt", "world"),
            &delete_payload("test2.txt"),
        )
        .unwrap();

        // Check snapshots
        let snapshots = vc.list_snapshots();
        assert_eq!(snapshots.len(), 2);

        // Undo last action
        let undone = vc.undo().unwrap();
        assert!(undone.is_some());
        assert!(!temp_dir.join("test2.txt").exists());

        // Redo
        let redone = vc.redo().unwrap();
        assert!(redone.is_some());
        assert!(temp_dir.join("test2.txt").exists());

        // Rollback to first snapshot
        vc.rollback(&snap1).unwrap();
        assert!(!temp_dir.join("test.txt").exists());
        assert!(!temp_dir.join("test2.txt").exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
