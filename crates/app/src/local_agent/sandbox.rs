//! Workspace sandbox enforcing file access boundaries with strict containment checks.
use std::path::{Path, PathBuf, Component};
use std::fs;

pub struct Sandbox {
    root: PathBuf,
    allowed_exts: Vec<String>,
}

impl Sandbox {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allowed_exts: vec!["txt".into(), "md".into(), "json".into(), "py".into(), "rs".into()],
        }
    }

    /// Checks if a path is strictly relative and contains no parent traversal (`..`),
    /// current directory reference (`.`), absolute roots, or prefixes, preventing
    /// any lexical path traversal attempts to escape the root.
    pub fn is_safe_relative_path(path_str: &str) -> bool {
        let path = Path::new(path_str);
        if path.is_absolute() {
            return false;
        }
        for component in path.components() {
            match component {
                Component::ParentDir => return false, // Disallow ".."
                Component::CurDir => return false,    // Disallow "."
                Component::Prefix(_) | Component::RootDir => return false,
                Component::Normal(_) => {}
            }
        }
        true
    }

    pub fn read(&self, path: &str) -> Result<String, String> {
        if !Self::is_safe_relative_path(path) {
            return Err("path escape blocked".into());
        }
        if !self.allowed(path) {
            return Err("file extension not allowed".into());
        }
        let full = self.root.join(path);
        if !full.starts_with(&self.root) {
            return Err("path escape blocked".into());
        }
        let meta = fs::metadata(&full).map_err(|_| "file not found".to_string())?;
        if meta.len() > 10_000_000 {
            return Err("file too large".into());
        }
        fs::read_to_string(&full).map_err(|_| "read error".into())
    }

    pub fn write(&self, path: &str, content: &str) -> Result<(), String> {
        if !Self::is_safe_relative_path(path) {
            return Err("path escape blocked".into());
        }
        if !self.allowed(path) {
            return Err("file extension not allowed".into());
        }
        let full = self.root.join(path);
        if !full.starts_with(&self.root) {
            return Err("path escape blocked".into());
        }
        // Ensure parent directory exists
        if let Some(parent) = full.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&full, content).map_err(|_| "write error".into())
    }

    pub fn allowed(&self, path: &str) -> bool {
        self.allowed_exts.iter().any(|e| path.ends_with(e))
    }

    pub fn list_files(&self) -> Vec<String> {
        let mut results = vec![];
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(s) = name.to_str() {
                    results.push(s.to_string());
                }
            }
        }
        results.sort();
        results
    }

    pub fn exists(&self, path: &str) -> bool {
        self.root.join(path).exists()
    }

    pub fn delete(&self, path: &str) -> Result<(), String> {
        if !Self::is_safe_relative_path(path) {
            return Err("escape".into());
        }
        let full = self.root.join(path);
        if !full.starts_with(&self.root) {
            return Err("escape".into());
        }
        fs::remove_file(&full).map_err(|_| "delete failed".into())
    }

    pub fn size_limit_check(&self, path: &str, limit: u64) -> bool {
        if let Ok(meta) = fs::metadata(self.root.join(path)) {
            meta.len() <= limit
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_relative_path() {
        assert!(Sandbox::is_safe_relative_path("foo/bar.txt"));
        assert!(Sandbox::is_safe_relative_path("code.rs"));
        assert!(!Sandbox::is_safe_relative_path("../escaped.txt"));
        assert!(!Sandbox::is_safe_relative_path("./local.txt"));
        assert!(!Sandbox::is_safe_relative_path("/etc/passwd"));
    }

    #[test]
    fn test_sandbox_extension_enforcement() {
        let dir = std::env::temp_dir().join(format!("roco-sandbox-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let sandbox = Sandbox::new(&dir);

        // Writing allowed extension
        assert!(sandbox.write("test.txt", "hello").is_ok());
        assert_eq!(sandbox.read("test.txt").unwrap(), "hello");

        // Writing disallowed extension
        assert!(sandbox.write("test.sh", "echo 1").is_err());
        assert!(sandbox.read("test.sh").is_err());

        // Relative path traversal prevention
        assert!(sandbox.write("../outside.txt", "data").is_err());
        assert!(sandbox.read("../outside.txt").is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
