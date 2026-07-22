//! Workspace capabilities: `workspace`, `workspace_transform`,
//! `workspace_timeline_reset`, `workspace_timeline_compare`.
//!
//! Wraps `roco_workspace::Workspace` + `roco_agent::reversibility::VersionControl`
//! so surfaces don't import either directly.

use std::path::{Path, PathBuf};

use roco_agent::reversibility::VersionControl;
use roco_workspace::{Workspace as CoreWorkspace, WorkspaceKind};

use crate::{AppError, AppResult};

/// A sandbox workspace plus its timeline (version control).
pub struct AppWorkspace {
    pub name: String,
    pub kind: WorkspaceKind,
    core: CoreWorkspace,
    vc: VersionControl,
}

/// A timeline checkpoint (opaque to surfaces; compared via `diff`).
pub struct Timeline {
    pub id: String,
    pub label: String,
}

impl AppWorkspace {
    /// Open (creating if missing) a workspace of `kind` under `root`.
    pub fn open(root: &Path, name: &str, kind: WorkspaceKind) -> AppResult<Self> {
        let core = CoreWorkspace::new(root.join(name), kind).map_err(AppError::Workspace)?;
        let vc = VersionControl::new(root.join(name));
        Ok(Self {
            name: name.to_string(),
            kind,
            core,
            vc,
        })
    }

    /// Resolve a path inside the workspace sandbox.
    pub fn resolve(&self, path: &str) -> AppResult<PathBuf> {
        self.core.resolve(path).map_err(AppError::Workspace)
    }

    /// Apply a transform (the `workspace_transform` op): run `transform`
    /// against the workspace and record it as a reversible action.
    pub fn transform(
        &self,
        description: &str,
        transform: impl FnOnce(&CoreWorkspace) -> AppResult<()>,
    ) -> AppResult<()> {
        transform(&self.core)?;
        self.vc
            .record_action("transform", description, "", "")
            .map_err(AppError::Other)?;
        Ok(())
    }

    /// Take a timeline checkpoint (the `workspace_timeline_reset` op).
    pub fn checkpoint(&self, label: &str) -> AppResult<Timeline> {
        let id = self.vc.snapshot(label).map_err(AppError::Other)?;
        Ok(Timeline {
            id,
            label: label.to_string(),
        })
    }

    /// Diff two checkpoints (the `workspace_timeline_compare` op).
    pub fn diff(&self, a: &Timeline, b: &Timeline) -> AppResult<String> {
        // Read both snapshot manifests and produce a unified diff of paths.
        let ra = self.read_snapshot_files(&a.id)?;
        let rb = self.read_snapshot_files(&b.id)?;
        let added: Vec<String> = rb
            .keys()
            .filter(|p| !ra.contains_key(*p))
            .cloned()
            .collect();
        let removed: Vec<String> = ra
            .keys()
            .filter(|p| !rb.contains_key(*p))
            .cloned()
            .collect();
        let mut out = String::new();
        for p in &added {
            out.push_str(&format!("+ {p}\n"));
        }
        for p in &removed {
            out.push_str(&format!("- {p}\n"));
        }
        if out.is_empty() {
            out.push_str("(no changes)\n");
        }
        Ok(out)
    }

    /// Read a snapshot's file map from disk.
    fn read_snapshot_files(
        &self,
        id: &str,
    ) -> AppResult<std::collections::HashMap<String, String>> {
        let path = self
            .core
            .root()
            .join(".snapshots")
            .join(format!("{id}.json"));
        let bytes = std::fs::read(&path).map_err(|e| AppError::Other(e.to_string()))?;
        let content = String::from_utf8_lossy(&bytes).to_string();
        let snap: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| AppError::Other(e.to_string()))?;
        let files = snap
            .get("files")
            .and_then(|f| f.as_object())
            .map(|o| {
                o.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(files)
    }

    /// Undo the last action on this workspace timeline.
    pub fn undo(&self) -> AppResult<Option<String>> {
        self.vc
            .undo()
            .map(|o| o.map(|a| a.description))
            .map_err(AppError::Other)
    }
}
