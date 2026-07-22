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

    pub fn parse(s: &str) -> WorkspaceKind {
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
        std::fs::create_dir_all(&root).map_err(|e| {
            WorkspaceError(format!(
                "failed to create workspace root {}: {}",
                root.display(),
                e
            ))
        })?;
        Self::from_existing(root, kind)
    }

    /// Open a workspace over an existing directory.
    pub fn from_existing(
        root: impl Into<PathBuf>,
        kind: WorkspaceKind,
    ) -> Result<Self, WorkspaceError> {
        let root = root.into();
        if !root.exists() {
            return Err(WorkspaceError(format!(
                "workspace root does not exist: {}",
                root.display()
            )));
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

    /// Create a workspace using the conventional root for its `kind`, rooted
    /// at the current working directory.
    ///
    /// - `Agent` / `User` pick persistent, cwd-relative roots
    ///   (`.roco/workspace/agent` and `.`, respectively) so an agent's working
    ///   memory survives across runs.
    /// - `Eval` / `Temp` / `Generic` fall back to an isolated temp dir.
    pub fn preset(kind: WorkspaceKind) -> Result<Self, WorkspaceError> {
        let base = std::env::current_dir()
            .map_err(|e| WorkspaceError(format!("could not determine current directory: {e}")))?;
        Self::preset_in(kind, &base)
    }

    /// Like [`Workspace::preset`], but the persistent roots are resolved
    /// against `base` instead of the process cwd (handy for tests).
    pub fn preset_in(kind: WorkspaceKind, base: &Path) -> Result<Self, WorkspaceError> {
        match kind {
            WorkspaceKind::Eval | WorkspaceKind::Temp | WorkspaceKind::Generic => Self::temp(kind),
            WorkspaceKind::Agent => {
                Self::new(base.join(".roco").join("workspace").join("agent"), kind)
            }
            WorkspaceKind::User => Self::new(base.to_path_buf(), kind),
        }
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

        let root_canon = self.root.canonicalize().map_err(|e| {
            WorkspaceError(format!(
                "workspace root '{}' is not accessible: {}",
                self.root.display(),
                e
            ))
        })?;

        // Canonical containment — catches symlink escapes for existing files.
        if let Ok(canon) = norm.canonicalize() {
            if !canon.starts_with(&root_canon) {
                return Err(WorkspaceError(format!(
                    "path '{}' escapes workspace boundary",
                    path
                )));
            }
        }

        // Lexical containment — catches `..` traversal for not-yet-existing paths.
        let norm_comps: Vec<_> = norm.components().collect();
        let root_comps: Vec<_> = root_canon.components().collect();
        if norm_comps.len() < root_comps.len() {
            return Err(WorkspaceError(format!(
                "path '{}' escapes workspace boundary",
                path
            )));
        }
        for (rc, nc) in root_comps.iter().zip(norm_comps.iter()) {
            if rc.as_os_str() != nc.as_os_str() {
                return Err(WorkspaceError(format!(
                    "path '{}' escapes workspace boundary",
                    path
                )));
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
            std::sync::Arc::new(crate::tools::WorkspaceGrepTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceListTool { ws: ws.clone() }),
            std::sync::Arc::new(crate::tools::WorkspaceBashTool { ws }),
        ]
    }

    /// Garbage collect temporary workspace files/folders older than `max_age`.
    pub fn vacuum_temp_files(
        dir: &Path,
        max_age: std::time::Duration,
    ) -> Result<usize, std::io::Error> {
        let mut count = 0;
        let now = SystemTime::now();
        if !dir.exists() {
            return Ok(0);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > max_age {
                            if path.is_dir() {
                                if std::fs::remove_dir_all(&path).is_ok() {
                                    count += 1;
                                }
                            } else {
                                if std::fs::remove_file(&path).is_ok() {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Recursively backup the workspace root directory into a backup folder.
    pub fn backup(&self, destination_dir: &Path) -> Result<PathBuf, WorkspaceError> {
        if !destination_dir.exists() {
            std::fs::create_dir_all(destination_dir)
                .map_err(|e| WorkspaceError(format!("Failed to create backup dir: {e}")))?;
        }
        let backup_folder =
            destination_dir.join(format!("{}_backup_{}", self.name(), self.created_at));
        std::fs::create_dir_all(&backup_folder)
            .map_err(|e| WorkspaceError(format!("Failed to create sub backup folder: {e}")))?;

        for entry in walkdir::WalkDir::new(&self.root) {
            let entry = entry.map_err(|e| WorkspaceError(format!("WalkDir error: {e}")))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|e| WorkspaceError(format!("Failed to strip prefix: {e}")))?;
            let dest = backup_folder.join(relative);

            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&dest).map_err(|e| {
                    WorkspaceError(format!("Failed to create dir {}: {}", dest.display(), e))
                })?;
            } else {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| WorkspaceError(format!("Failed to create parent dir: {e}")))?;
                }
                std::fs::copy(path, &dest).map_err(|e| {
                    WorkspaceError(format!(
                        "Failed to copy file from {} to {}: {}",
                        path.display(),
                        dest.display(),
                        e
                    ))
                })?;
            }
        }
        Ok(backup_folder)
    }

    /// Restore the workspace root from a previously taken backup.
    pub fn restore(&self, backup_dir: &Path) -> Result<(), WorkspaceError> {
        if !backup_dir.exists() {
            return Err(WorkspaceError(format!(
                "Backup folder does not exist: {}",
                backup_dir.display()
            )));
        }
        // Clear current root files safely
        for entry in std::fs::read_dir(&self.root)
            .map_err(|e| WorkspaceError(format!("Failed to read root for clearing: {e}")))?
        {
            let entry = entry.map_err(|e| WorkspaceError(format!("Failed to read entry: {e}")))?;
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path).map_err(|e| {
                    WorkspaceError(format!("Failed to delete dir {}: {}", path.display(), e))
                })?;
            } else {
                std::fs::remove_file(&path).map_err(|e| {
                    WorkspaceError(format!("Failed to delete file {}: {}", path.display(), e))
                })?;
            }
        }

        // Copy files back
        for entry in walkdir::WalkDir::new(backup_dir) {
            let entry =
                entry.map_err(|e| WorkspaceError(format!("WalkDir error during restore: {e}")))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(backup_dir)
                .map_err(|e| WorkspaceError(format!("Failed to strip prefix: {e}")))?;
            let dest = self.root.join(relative);

            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&dest)
                    .map_err(|e| WorkspaceError(format!("Failed to create dir: {e}")))?;
            } else {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| WorkspaceError(format!("Failed to create parent: {e}")))?;
                }
                std::fs::copy(path, &dest).map_err(|e| {
                    WorkspaceError(format!("Failed to restore file {}: {}", dest.display(), e))
                })?;
            }
        }
        Ok(())
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

/// Commands that are never permitted inside a workspace shell, even though it
/// is cwd-scoped. The shell is not a syscall sandbox, so this is a
/// belt-and-suspenders guard that refuses the most destructive or
/// escape-prone patterns (e.g. wiping the root filesystem, formatting a
/// device, or forking a fork-bomb).
const BLOCKED_COMMAND_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    ":(){:|:&",
    "dd if=/dev",
    "> /dev/sda",
    "shutdown",
    "reboot",
    "chmod -r 000",
    "chmod -r 777",
];

/// Returns `Some(pattern)` if `cmd` matches a [`BLOCKED_COMMAND_PATTERNS`]
/// entry, otherwise `None`. Matching is case-insensitive on a trimmed command.
///
/// This is deliberately a small, conservative denylist — it is meant to stop
/// catastrophically destructive commands, not to be a security boundary (the
/// workspace boundary itself is `Workspace::resolve`).
pub fn blocked_command_reason(cmd: &str) -> Option<&'static str> {
    let lower = cmd.trim().to_lowercase();
    BLOCKED_COMMAND_PATTERNS
        .iter()
        .copied()
        .find(|pat| lower.contains(pat))
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

    #[test]
    fn preset_agent_root_is_persistent_under_roco() {
        let base = std::env::temp_dir().join(format!("roco-base-{}", std::process::id()));
        let ws = Workspace::preset_in(WorkspaceKind::Agent, &base).unwrap();
        assert!(ws.root().ends_with(".roco/workspace/agent"));
        assert_eq!(ws.kind(), WorkspaceKind::Agent);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn preset_user_root_is_base() {
        let base = std::env::temp_dir().join(format!("roco-base-{}-u", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let ws = Workspace::preset_in(WorkspaceKind::User, &base).unwrap();
        assert_eq!(ws.root(), &base);
        assert_eq!(ws.kind(), WorkspaceKind::User);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn preset_temp_and_eval_are_isolated() {
        let base = std::env::temp_dir();
        let tmp = Workspace::preset_in(WorkspaceKind::Temp, &base).unwrap();
        let eval = Workspace::preset_in(WorkspaceKind::Eval, &base).unwrap();
        assert!(tmp.root().starts_with(std::env::temp_dir()));
        assert!(eval.root().starts_with(std::env::temp_dir()));
        assert_ne!(tmp.root(), eval.root());
    }

    // ── Sandbox-escape regression guard ─────────────────────────────
    //
    // These tests plant a secret *outside* the workspace and assert that
    // neither lexical traversal (`../../`) nor symlink escapes can reach it,
    // whether resolved directly or via the `read` tool. This is the
    // regression guard called for by the workspace-layer self-directed goal.
    fn plant_secret_outside(ws: &Workspace) -> std::path::PathBuf {
        // Put a secret one level above the workspace root, named uniquely so
        // parallel tests don't collide.
        let outside = ws.root().parent().expect("workspace root has a parent");
        let secret = outside.join(format!(
            "roco-secret-{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&secret, "TOP-SECRET").unwrap();
        secret
    }

    #[test]
    fn escape_via_parent_traversal_is_blocked() {
        let ws = temp();
        let secret = plant_secret_outside(&ws);
        defer_delete(&secret);

        // Resolve the secret through `..` traversal — must be rejected.
        assert!(ws.resolve("../../").is_err() || ws.resolve("..").is_err());
        assert!(
            ws.resolve(&format!(
                "../{}",
                secret.file_name().unwrap().to_string_lossy()
            ))
            .is_err(),
            "resolving a sibling-of-root file via '..' must be rejected"
        );
    }

    #[test]
    fn escape_via_absolute_path_is_blocked() {
        let ws = temp();
        let secret = plant_secret_outside(&ws);
        defer_delete(&secret);

        // Absolute path to the secret must be rejected (it is outside root).
        let abs = secret.to_string_lossy().to_string();
        assert!(
            ws.resolve(&abs).is_err(),
            "absolute path outside root must be rejected"
        );
    }

    #[test]
    fn read_tool_blocks_traversal_escape() {
        let ws = temp();
        let secret = plant_secret_outside(&ws);
        defer_delete(&secret);

        let tools = Workspace::scoped_tools(std::sync::Arc::new(ws));
        let read = tools.iter().find(|t| t.name() == "read").unwrap();
        let rel = format!("../{}", secret.file_name().unwrap().to_string_lossy());
        let res = read.call(serde_json::json!({ "path": rel }));
        assert!(res.is_err(), "read tool must reject traversal escape");
    }

    #[cfg(unix)]
    #[test]
    fn escape_via_symlink_is_blocked() {
        use std::os::unix::fs::symlink;
        let ws = temp();
        let secret = plant_secret_outside(&ws);
        defer_delete(&secret);

        // Create a symlink *inside* the workspace that points at the parent
        // directory containing the secret, then try to read the secret through
        // it. Canonical containment in `resolve` must catch this.
        let link = ws.root().join("escape_link");
        symlink(secret.parent().unwrap(), &link).unwrap();

        let tools = Workspace::scoped_tools(std::sync::Arc::new(ws));
        let read = tools.iter().find(|t| t.name() == "read").unwrap();
        let target = format!(
            "escape_link/{}",
            secret.file_name().unwrap().to_string_lossy()
        );
        let res = read.call(serde_json::json!({ "path": target }));
        assert!(res.is_err(), "read tool must reject symlink escape");
    }

    #[test]
    fn legit_in_bounds_access_still_works() {
        let ws = temp();
        std::fs::write(ws.root().join("ok.txt"), "safe").unwrap();
        let resolved = ws.resolve("ok.txt").unwrap();
        assert!(resolved.starts_with(ws.root()));
        assert_eq!(std::fs::read_to_string(&resolved).unwrap(), "safe");
    }

    #[test]
    fn test_backup_and_restore() {
        let ws = temp();
        let file1 = ws.root().join("test_file.txt");
        std::fs::write(&file1, "workspace-content").unwrap();

        let backup_dest = std::env::temp_dir().join(format!("roco-backups-{}", std::process::id()));
        let backup_folder = ws.backup(&backup_dest).unwrap();
        assert!(backup_folder.exists());
        assert!(backup_folder.join("test_file.txt").exists());

        // Change workspace file content
        std::fs::write(&file1, "changed-content").unwrap();

        // Restore
        ws.restore(&backup_folder).unwrap();
        let restored_content = std::fs::read_to_string(&file1).unwrap();
        assert_eq!(restored_content, "workspace-content");

        let _ = std::fs::remove_dir_all(&backup_dest);
    }

    #[test]
    fn test_vacuum_temp_files() {
        let temp_dir = std::env::temp_dir().join(format!("roco-vacuum-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let old_file = temp_dir.join("old_file.txt");
        std::fs::write(&old_file, "old-content").unwrap();

        // Modify time of old_file to 10 minutes ago
        let filetime = filetime::FileTime::from_system_time(
            SystemTime::now() - std::time::Duration::from_secs(600),
        );
        filetime::set_file_times(&old_file, filetime, filetime).unwrap();

        // Vacuum files older than 5 minutes (300 secs)
        let deleted =
            Workspace::vacuum_temp_files(&temp_dir, std::time::Duration::from_secs(300)).unwrap();
        assert_eq!(deleted, 1);
        assert!(!old_file.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Best-effort cleanup helper so planted secrets don't leak between runs.
    fn defer_delete(path: &std::path::Path) {
        let p = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = std::fs::remove_file(&p);
        });
    }

    // ── Workspace lifecycle tests ────────────────────────────────────

    #[test]
    fn start_in_existing_workspace() {
        // Create a workspace, then "start in" it by opening from_existing
        let original = Workspace::temp(WorkspaceKind::Agent).unwrap();
        let name = "my-story-workspace";
        let original = original.with_name(name);

        // Write a story file into it
        std::fs::write(original.root().join("01-OUTLINE.md"), "# My Story Outline").unwrap();

        // Now "start" by opening it with a different Workspace instance
        let restarted = Workspace::from_existing(original.root(), WorkspaceKind::Agent)
            .unwrap()
            .with_name(name);

        assert_eq!(restarted.name(), name);
        assert_eq!(restarted.kind(), WorkspaceKind::Agent);
        assert!(restarted.root().exists());

        // Verify the story file is accessible
        let outline = std::fs::read_to_string(restarted.root().join("01-OUTLINE.md")).unwrap();
        assert!(outline.contains("My Story Outline"));

        let _ = std::fs::remove_dir_all(original.root());
    }

    #[test]
    fn make_temp_workspace_with_name() {
        // Create a temp workspace with a human-readable name (slug)
        let slug = "my-test-story";
        let ws = Workspace::temp(WorkspaceKind::Eval)
            .unwrap()
            .with_name(slug);

        // Name is set (slug is human-readable, not necessarily the dir name)
        assert_eq!(ws.name(), slug);
        assert_eq!(ws.kind(), WorkspaceKind::Eval);
        assert!(ws.root().starts_with(std::env::temp_dir()));
        assert!(ws.root().exists());

        // Write a story file to confirm it's usable
        std::fs::write(
            ws.root().join("chapter_1.md"),
            "# Chapter 1\n\nIt was a dark and stormy night...",
        )
        .unwrap();
        assert!(ws.resolve("chapter_1.md").unwrap().exists());

        let _ = std::fs::remove_dir_all(ws.root());
    }

    #[test]
    fn make_workspace_with_unique_name_and_slug() {
        // Create a persistent-style workspace with a unique name and optional slug
        let base = std::env::temp_dir().join(format!("roco-unique-{}", std::process::id()));
        let unique_name = format!(
            "story-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let slug = "ghost-story";

        let ws = Workspace::new(base.join(&unique_name), WorkspaceKind::Agent)
            .unwrap()
            .with_name(slug);

        assert_eq!(ws.name(), slug);
        assert!(ws.root().exists());
        assert!(ws.root().to_string_lossy().contains(&unique_name));

        // Verify the directory is actually at the unique path
        assert!(std::fs::read_dir(ws.root()).is_ok());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn many_stories_in_one_workspace() {
        // A workspace can hold multiple story files (different stories)
        let ws = Workspace::temp(WorkspaceKind::Agent).unwrap();

        // Write multiple story files simulating different stories in same workspace
        let stories = vec![
            (
                "stories/the_lighthouse.md",
                "# The Lighthouse\n\nA tale of the sea...",
            ),
            ("stories/the_dragon.md", "# The Dragon\n\nA tale of fire..."),
            (
                "stories/the_robot.md",
                "# The Robot\n\nA tale of circuits...",
            ),
        ];

        for (path, content) in &stories {
            let full_path = ws.root().join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full_path, content).unwrap();
        }

        // Verify all stories are accessible
        for (path, content) in &stories {
            let resolved = ws.resolve(path).unwrap();
            assert!(resolved.exists(), "story file {} should exist", path);
            let read_content = std::fs::read_to_string(&resolved).unwrap();
            assert_eq!(read_content, *content);
        }

        // Also verify that the workspace can hold outline + chapter files alongside stories
        let pipeline_files = vec![
            "01-OUTLINE.md",
            "02-WIKI.md",
            "03-CHAPTER_1.md",
            "03-CHAPTER_2.md",
            "04-VALIDATION.md",
            "05-SYNOPSIS.md",
            "06-STORY.md",
        ];

        for fname in &pipeline_files {
            let content = format!("# {}", fname.replace(".md", ""));
            let resolved = ws.resolve(fname).unwrap();
            std::fs::write(&resolved, &content).unwrap();
            assert!(resolved.exists());
        }

        // Confirm all files still resolve and are within the single workspace root
        let mut file_count = 0;
        for entry in walkdir::WalkDir::new(ws.root()) {
            if entry.is_ok() {
                file_count += 1;
            }
        }
        // stories/ dir (3 files) + 7 pipeline files + stories/ dir entry + .snapshots/ maybe + ws root
        assert!(
            file_count >= 10,
            "expected at least 10 files in workspace, got {}",
            file_count
        );

        let _ = std::fs::remove_dir_all(ws.root());
    }
}
