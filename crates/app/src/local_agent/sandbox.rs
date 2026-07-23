//! Workspace sandbox enforcing file access boundaries.
use std::path::PathBuf;
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

    pub fn read(&self, path: &str) -> Result<String, String> {
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
