//! Workspace management for the gateway.
//!
//! The gateway is the single source of truth for workspace state.
//! All file operations go through the gateway, not directly to disk.

use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;

/// A workspace managed by the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
    pub created_at: u64,
    pub last_accessed_at: u64,
}

/// Workspace manager with sandbox enforcement.
pub struct WorkspaceManager {
    workspaces: RwLock<HashMap<String, Workspace>>,
    /// Base directory for all workspaces.
    base_dir: PathBuf,
}

impl WorkspaceManager {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(&base_dir).ok();
        Self {
            workspaces: RwLock::new(HashMap::new()),
            base_dir,
        }
    }

    /// Create a new workspace.
    pub fn create(&self, id: impl Into<String>) -> Result<Workspace, String> {
        let id = id.into();
        let root = self.base_dir.join(&id);
        std::fs::create_dir_all(&root).map_err(|e| format!("create workspace dir: {e}"))?;

        let ws = Workspace {
            id: id.clone(),
            root,
            created_at: now_secs(),
            last_accessed_at: now_secs(),
        };
        self.workspaces.write().insert(id.clone(), ws.clone());
        info!("Created workspace {}", id);
        Ok(ws)
    }

    /// Get a workspace.
    pub fn get(&self, id: &str) -> Option<Workspace> {
        let mut workspaces = self.workspaces.write();
        if let Some(ws) = workspaces.get_mut(id) {
            ws.last_accessed_at = now_secs();
            Some(ws.clone())
        } else {
            None
        }
    }

    /// Resolve a path within a workspace, enforcing sandbox.
    pub fn resolve(&self, workspace_id: &str, relative: &str) -> Result<PathBuf, String> {
        let ws = self.get(workspace_id).ok_or("workspace not found")?;
        let path = ws.root.join(relative);

        // Sandbox check: must be within workspace root
        let canonical = path.canonicalize().unwrap_or(path.clone());
        let root_canonical = ws.root.canonicalize().unwrap_or(ws.root.clone());
        if !canonical.starts_with(&root_canonical) {
            return Err("path escapes workspace sandbox".into());
        }
        Ok(path)
    }

    /// Read a file from a workspace.
    pub fn read_file(&self, workspace_id: &str, path: &str) -> Result<String, String> {
        let resolved = self.resolve(workspace_id, path)?;
        std::fs::read_to_string(&resolved).map_err(|e| format!("read file: {e}"))
    }

    /// Write a file to a workspace.
    pub fn write_file(&self, workspace_id: &str, path: &str, content: &str) -> Result<(), String> {
        let resolved = self.resolve(workspace_id, path)?;
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
        }
        std::fs::write(&resolved, content).map_err(|e| format!("write file: {e}"))
    }

    /// List files in a workspace (non-recursive).
    pub fn list_files(&self, workspace_id: &str) -> Result<Vec<String>, String> {
        let ws = self.get(workspace_id).ok_or("workspace not found")?;
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&ws.root).map_err(|e| format!("read dir: {e}"))? {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                files.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        Ok(files)
    }

    /// Delete a workspace and all its files.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let ws = self.get(id).ok_or("workspace not found")?;
        std::fs::remove_dir_all(&ws.root).map_err(|e| format!("delete workspace: {e}"))?;
        self.workspaces.write().remove(id);
        info!("Deleted workspace {}", id);
        Ok(())
    }

    /// List all workspaces.
    pub fn list_all(&self) -> Vec<Workspace> {
        self.workspaces.read().values().cloned().collect()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}


