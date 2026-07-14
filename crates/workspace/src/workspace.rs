use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use roco_tools::Tool;

use crate::error::WorkspaceError;

/// The kind of workspace, distinguishing sandbox purposes.
///
/// The `User:` note on `goals/workspace/workspace.md` calls out four flavors
/// of workspace the product cares about: eval, temp, user, and agent. We
/// model them here so tooling and logging can behave accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceKind {
    /// Isolated workspace for running eval cases / fixtures.
    Eval,
    /// Throwaway workspace (usually backed by a temp dir) for scratch work.
    Temp,
    /// A user-supplied project directory the agent may read/write.
    User,
    /// A workspace the autonomous agent drives as its working memory.
    Agent,
    /// Unspecified / generic boundary.
    Generic,
}

impl WorkspaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WorkspaceKind::Eval => "eval",
            WorkspaceKind::Temp => "temp",
            WorkspaceKind::User => "user",
            WorkspaceKind::Agent => "agent",
            WorkspaceKind::Generic => "generic",
        }
    }

    pub fn from_str(s: &str) -> WorkspaceKind {
        match s.to_ascii_lowercase().as_str() {
            "eval" => WorkspaceKind::Eval,
            "temp" => WorkspaceKind::Temp,
            "user" => WorkspaceKind::User,
            "agent" => WorkspaceKind::Agent,
            _ => WorkspaceKind::Generic,
        }
    }
}

/// A scoped workspace that controls file access and tool execution.
///
/// All file paths passed to workspace tools are resolved against the
/// workspace root and checked so they cannot escape the boundary — lexical
/// `..` traversal is neutralized by path normalization, and symlink escapes
/// are caught by canonicalizing targets that already exist. The optional
/// `cwd` is a relative working directory used by shell-like tools.
pub struct Workspace {
    root: PathBuf,
    cwd: RwLock<PathBuf>,
    kind: WorkspaceKind,
    name: RwLock<String>,
    created_at: u64,
}

impl Workspace {
    /// Create a workspace rooted at `root`, creating the directory if missing.
    pub fn new(root: impl Into<PathBuf>, kind: WorkspaceKind) -> Result<Self, WorkspaceError> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .map_err(|e| WorkspaceError(format!("failed to create workspace root {}: {}", root.display(), e)))?;
        Self::from_existing(root, kind)
    }

    /// Open a workspace over an existing directory.
    pub fn from_existing(root: impl Into<PathBuf>, kind: WorkspaceKind) -> Result<Self, WorkspaceError> {
        let root = root.into();
        if !root.exists() {
            return Err(WorkspaceError(format!("workspace root does not exist: {}", root.display())));
        }
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Ok(Self {
            root,
            cwd: RwLock::new(PathBuf::new()),
            kind,
            name: RwLock::new("workspace".to_string()),
            created_at,
        })
    }

    /// Create an isolated temporary workspace under `std::env::temp_dir()`.
    ///
    /// The caller is responsible for removing the directory when finished.
    pub fn temp(kind: WorkspaceKind) -> Result<Self, WorkspaceError> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("roco-ws-{}-{:x}", kind.as_str(), stamp));
        Self::new(dir, kind)
    }

    /// Set a human-readable name for this workspace.
    pub fn with_name(self, name: impl Into<String>) -> Self {
        if let Ok(mut n) = self.name.write() {
            *n = name.into();
        }
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Current working directory, relative to [`Workspace::root`].
    pub fn cwd(&self) -> PathBuf {
        self.cwd.read().expect("cwd lock poisoned").clone()
    }

    pub fn kind(&self) -> WorkspaceKind {
        self.kind
    }

    pub fn name(&self) -> String {
        self.name.read().expect("name lock poisoned").clone()
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    /// Set the relative working directory. Validates that the resulting path
    /// stays inside the workspace boundary.
    pub fn set_cwd(&self, cwd: &str) -> Result<(), WorkspaceError> {
        self.resolve(cwd)?;
        let mut guard = self.cwd.write().expect("cwd lock poisoned");
        *guard = PathBuf::from(cwd);
        Ok(())
    }

    /// Resolve `path` against the workspace, returning a path guaranteed to
    /// lie inside the workspace root. Errors if the path would escape.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, WorkspaceError> {
        let target = Path::new(path);
        let base = if path.is_empty() {
            self.root.join(self.cwd())
        } else if target.is_absolute() {
            target.to_path_buf()
        } else {
            self.root.join(self.cwd()).join(target)
        };

        let norm = Self::lexical_normalize(&base);

        let root_canon = self
            .root
            .canonicalize()
            .map_err(|e| WorkspaceError(format!("workspace root '{}' is not accessible: {}", self.root.display(), e)))?;

        // Canonical containment — catches symlink escapes for existing files.
        if let Ok(canon) = norm.canonicalize() {
            if !canon.starts_with(&root_canon) {
                return Err(WorkspaceError(format!("path '{}' escapes workspace boundary", path)));
            }
        }

        // Lexical containment — catches `..` traversal for not-yet-existing paths.
        let norm_comps: Vec<_> = norm.components().collect();
        let root_comps: Vec<_> = root_canon.components().collect();
        if norm_comps.len() < root_comps.len() {
            return Err(WorkspaceError(format!("path '{}' escapes workspace boundary", path)));
        }
        for (rc, nc) in root_comps.iter().zip(norm_comps.iter()) {
            if rc.as_os_str() != nc.as_os_str() {
                return Err(WorkspaceError(format!("path '{}' escapes workspace boundary", path)));
            }
        }

        Ok(norm)
    }

    /// Build the workspace-scoped tool set: read / write / edit / search /
    /// list / bash, all confined to this workspace boundary.
    pub fn scoped_tools(ws: std::sync::Arc<Workspace>) -> Vec<std::sync::Arc<dyn Tool>> {
        vec![
            std::sync::Arc::new(crate::tools::WorkspaceReadTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceWriteTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceEditTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceSearchTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceListTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceBashTool { ws }),
        ]
    }

    /// Metadata describing this workspace, useful for logging / debugging.
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name(),
            "kind": self.kind().as_str(),
            "root": self.root.display().to_string(),
            "cwd": self.cwd().display().to_string(),
            "created_at": self.created_at(),
        })
    }

    fn lexical_normalize(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::Prefix(p) => out.push(p.as_os_str()),
                Component::RootDir => out.push("/"),
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(s) => out.push(s),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> Workspace {
        Workspace::temp(WorkspaceKind::Temp).unwrap()
    }

    #[test]
    fn new_creates_dir_and_metadata() {
        let ws = temp();
        assert!(ws.root().exists());
        assert_eq!(ws.kind(), WorkspaceKind::Temp);
        assert!(!ws.name().is_empty());
        let m = ws.metadata();
        assert_eq!(m["kind"], "temp");
        assert!(m["root"].as_str().unwrap().contains("roco-ws"));
    }

    #[test]
    fn resolve_keeps_paths_inside_root() {
        let ws = temp();
        let p = ws.resolve("sub/dir/file.txt").unwrap();
        assert!(p.starts_with(ws.root()));
    }

    #[test]
    fn resolve_blocks_parent_traversal() {
        let ws = temp();
        assert!(ws.resolve("../../etc/passwd").is_err());
        assert!(ws.resolve("/etc/passwd").is_err());
    }

    #[test]
    fn resolve_allows_traversal_that_stays_inside() {
        let ws = temp();
        std::fs::create_dir_all(ws.root().join("a/b")).unwrap();
        let p = ws.resolve("a/b/../../c").unwrap();
        assert!(p.starts_with(ws.root()));
        assert!(p.ends_with("c"));
    }

    #[test]
    fn set_cwd_validates_boundary() {
        let ws = temp();
        std::fs::create_dir_all(ws.root().join("work")).unwrap();
        assert!(ws.set_cwd("work").is_ok());
        assert_eq!(ws.cwd(), PathBuf::from("work"));
        // Enough `..` to climb above the (unknown-depth) workspace root.
        assert!(ws.set_cwd("../../../../../../../../escape").is_err());
    }

    #[test]
    fn from_existing_rejects_missing_root() {
        let missing = std::env::temp_dir().join("roco-ws-does-not-exist-xyz");
        assert!(Workspace::from_existing(&missing, WorkspaceKind::User).is_err());
    }
}
