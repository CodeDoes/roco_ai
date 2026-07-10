use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::{Result, Context};

/// A managed filesystem area for an agent session.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    /// Create a new workspace at the given path.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { root }
    }

    /// Create a temporary workspace under `.roco/workspaces/` with a given prefix
    /// and pre-populate it with files.
    pub fn temp(prefix: &str, files: &HashMap<String, String>) -> Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        
        let root = std::env::current_dir()?
            .join(".roco")
            .join("workspaces")
            .join(format!("{}-{}", prefix, timestamp));
        
        fs::create_dir_all(&root).context("failed to create workspace root")?;
        
        for (path, content) in files {
            let full_path = root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(full_path, content)?;
        }
        
        Ok(Self { root })
    }

    /// Create a folder within the workspace and return its path.
    pub fn add_folder(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        let _ = fs::create_dir_all(&path);
        path
    }

    /// Canonicalize the root for safety checks.
    pub fn canonical_root(&self) -> Result<PathBuf> {
        fs::canonicalize(&self.root).context("failed to canonicalize workspace root")
    }
}
