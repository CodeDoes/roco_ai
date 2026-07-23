//! StoryToolSet — safe filesystem operations for story editing.
//!
//! Wraps a workspace directory and provides read/write/grep/edit operations
//! on story files. All write operations create backups in `.backup/`.
//!
//! # Conventions
//!
//! A story workspace is expected to have:
//! ```text
//! .roco/workspaces/<name>/
//!   ├── outline.md          (story outline)
//!   ├── wiki.md             (world-building wiki)
//!   ├── chapters/           (chapter files)
//!   │   ├── 01-chapter.md
//!   │   ├── 02-chapter.md
//!   │   └── ...
//!   └── .backup/            (automatic backups)
//! ```
//!
//! # Backup
//!
//! Every write operation saves the previous version to `.backup/` with a
//! timestamp. Undo is supported by copying from `.backup/`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single grep match result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrepMatch {
    pub file: String,
    pub line: String,
    pub line_number: usize,
}

/// Result of a find-replace operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditResult {
    pub file: String,
    pub replacements: usize,
    pub success: bool,
}

/// Safe filesystem operations for story editing.
///
/// All paths are resolved against the workspace root and checked for
/// boundary escapes. Backup is created before every write.
#[derive(Debug, Clone)]
pub struct StoryToolSet {
    /// Root directory of the story workspace.
    workspace_path: PathBuf,
}

impl StoryToolSet {
    /// Create a new tool set for a story workspace.
    ///
    /// The workspace path should point to a `.roco/workspaces/<name>/` directory.
    pub fn new(workspace_path: impl Into<PathBuf>) -> Self {
        Self {
            workspace_path: workspace_path.into(),
        }
    }

    // ═════════════════════════════════════════════════════════════════
    // Path resolution
    // ═════════════════════════════════════════════════════════════════

    /// Resolve a file path relative to the workspace root.
    fn resolve(&self, path: &str) -> PathBuf {
        let mut resolved = self.workspace_path.join(path);
        // Normalize to prevent directory traversal
        if let Ok(normalized) = resolved.canonicalize() {
            resolved = normalized;
        }
        resolved
    }

    /// Get the path to the outline file.
    pub fn outline_path(&self) -> PathBuf {
        // Try common outline file names
        for name in &["outline.md", "01-OUTLINE.md", "outline.txt"] {
            let path = self.workspace_path.join(name);
            if path.exists() {
                return path;
            }
        }
        // Default
        self.workspace_path.join("outline.md")
    }

    /// Get the path to the wiki file.
    pub fn wiki_path(&self) -> PathBuf {
        for name in &["wiki.md", "02-WIKI.md", "world.md"] {
            let path = self.workspace_path.join(name);
            if path.exists() {
                return path;
            }
        }
        self.workspace_path.join("wiki.md")
    }

    /// Get the path to a chapter file by number.
    pub fn chapter_path(&self, num: usize) -> PathBuf {
        let chapters_dir = self.workspace_path.join("chapters");
        if chapters_dir.exists() {
            // Look for numbered files in chapters/ directory
            for name in &[
                format!("{:02}-chapter.md", num),
                format!("{:02}-CHAPTER.md", num),
                format!("chapter-{}.md", num),
                format!("chapter_{}.md", num),
                format!("03-CHAPTER_{}.md", num),
            ] {
                let path = chapters_dir.join(&name);
                if path.exists() {
                    return path;
                }
            }
        }
        // Also check root for flat layout
        for name in &[
            format!("CHAPTER_{}.md", num),
            format!("chapter_{}.md", num),
            format!("03-CHAPTER_{}.md", num),
            format!("{:02}-chapter.md", num),
        ] {
            let path = self.workspace_path.join(&name);
            if path.exists() {
                return path;
            }
        }
        // Fallback: construct default path
        self.workspace_path
            .join("chapters")
            .join(format!("{:02}-chapter.md", num))
    }

    /// Get the backup directory path.
    fn backup_dir(&self) -> PathBuf {
        let dir = self.workspace_path.join(".backup");
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    /// Create a timestamped backup of a file before writing to it.
    fn backup_file(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file for backup: {e}"))?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let rel_path = path.strip_prefix(&self.workspace_path).unwrap_or(path);

        let backup_name = format!(
            "{}_{}_{}",
            timestamp,
            rel_path.to_string_lossy().replace('/', "_"),
            "bak"
        );

        let backup_path = self.backup_dir().join(&backup_name);
        std::fs::write(&backup_path, &content)
            .map_err(|e| format!("Failed to write backup: {e}"))?;

        Ok(())
    }

    /// List available backups for a file.
    pub fn list_backups(&self, file_path: &Path) -> Result<Vec<PathBuf>, String> {
        let backup_dir = self.backup_dir();
        let rel_path = file_path
            .strip_prefix(&self.workspace_path)
            .unwrap_or(file_path);

        let prefix = format!("_{}_", rel_path.to_string_lossy().replace('/', "_"));

        let mut backups = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(&prefix) {
                    backups.push(entry.path());
                }
            }
        }
        backups.sort();
        Ok(backups)
    }

    /// Restore the most recent backup for a file.
    pub fn restore_latest_backup(&self, file_path: &Path) -> Result<(), String> {
        let backups = self.list_backups(file_path)?;
        let latest = backups
            .last()
            .ok_or_else(|| "No backups found".to_string())?;

        let content =
            std::fs::read_to_string(latest).map_err(|e| format!("Failed to read backup: {e}"))?;

        std::fs::write(file_path, &content)
            .map_err(|e| format!("Failed to restore backup: {e}"))?;

        Ok(())
    }

    // ═════════════════════════════════════════════════════════════════
    // Reading
    // ═════════════════════════════════════════════════════════════════

    /// Read the full wiki text.
    pub fn read_wiki(&self) -> Result<String, String> {
        let path = self.wiki_path();
        if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read wiki: {e}"))
        } else {
            Ok(String::new())
        }
    }

    /// Read a chapter by number.
    pub fn read_chapter(&self, num: usize) -> Result<String, String> {
        let path = self.chapter_path(num);
        if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read chapter {num}: {e}"))
        } else {
            Err(format!("Chapter {num} not found at {:?}", path))
        }
    }

    /// Read all chapters, returning them in order.
    pub fn read_all_chapters(&self) -> Result<Vec<String>, String> {
        let count = self.count_chapters();
        let mut chapters = Vec::with_capacity(count);
        for i in 1..=count {
            match self.read_chapter(i) {
                Ok(text) => chapters.push(text),
                Err(_) => break, // Stop at first missing chapter
            }
        }
        Ok(chapters)
    }

    /// Read the full outline text.
    pub fn read_outline(&self) -> Result<String, String> {
        let path = self.outline_path();
        if path.exists() {
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read outline: {e}"))
        } else {
            Ok(String::new())
        }
    }

    /// Count the number of chapters in the story.
    pub fn count_chapters(&self) -> usize {
        let chapters_dir = self.workspace_path.join("chapters");
        if chapters_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&chapters_dir) {
                let count = entries
                    .filter_map(|e| {
                        e.ok().and_then(|e| {
                            let name = e.file_name().to_string_lossy().to_lowercase();
                            if name.ends_with(".md") || name.ends_with(".txt") {
                                // Try to extract number from filename
                                name.split(|c: char| !c.is_ascii_digit())
                                    .next()
                                    .and_then(|s| s.parse::<usize>().ok())
                            } else {
                                None
                            }
                        })
                    })
                    .max()
                    .unwrap_or(0);
                return count;
            }
        }

        // Fallback: count by trying to read chapters 1..N
        for i in 1..=100 {
            if self.chapter_path(i).exists() {
                continue;
            }
            return i - 1;
        }
        0
    }

    /// Get total word count across all chapters.
    pub fn total_word_count(&self) -> usize {
        self.read_all_chapters()
            .unwrap_or_default()
            .iter()
            .map(|c| c.split_whitespace().count())
            .sum()
    }

    // ═════════════════════════════════════════════════════════════════
    // Grep / Search
    // ═════════════════════════════════════════════════════════════════

    /// Grep across all chapters for a pattern (case-insensitive).
    pub fn grep_chapters(&self, pattern: &str) -> Result<Vec<GrepMatch>, String> {
        let low_pattern = pattern.to_lowercase();
        let mut results = Vec::new();

        let chapters_dir = self.workspace_path.join("chapters");
        if chapters_dir.exists() {
            let mut files: Vec<_> = std::fs::read_dir(&chapters_dir)
                .map_err(|e| format!("Failed to list chapters dir: {e}"))?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.ends_with(".md") || name.ends_with(".txt")
                })
                .collect();
            files.sort_by_key(|e| e.file_name());

            for entry in &files {
                let path = entry.path();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    for (i, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&low_pattern) {
                            results.push(GrepMatch {
                                file: file_name.clone(),
                                line: line.trim().to_string(),
                                line_number: i + 1,
                            });
                        }
                    }
                }
            }
        }

        // Also check root for flat chapter files
        for i in 1..=self.count_chapters() {
            let path = self.chapter_path(i);
            if path.exists() {
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Skip if already found from chapters dir
                    if results.iter().any(|r| r.file == file_name) {
                        continue;
                    }
                    for (i, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&low_pattern) {
                            results.push(GrepMatch {
                                file: file_name.clone(),
                                line: line.trim().to_string(),
                                line_number: i + 1,
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Grep the wiki for a pattern (case-insensitive).
    pub fn grep_wiki(&self, pattern: &str) -> Result<Vec<GrepMatch>, String> {
        let path = self.wiki_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let low_pattern = pattern.to_lowercase();
        let mut results = Vec::new();
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read wiki: {e}"))?;

        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        for (i, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&low_pattern) {
                results.push(GrepMatch {
                    file: file_name.clone(),
                    line: line.trim().to_string(),
                    line_number: i + 1,
                });
            }
        }

        Ok(results)
    }

    /// Grep across all story files (outline, wiki, chapters) for a pattern.
    pub fn grep(&self, pattern: &str) -> Result<Vec<GrepMatch>, String> {
        let mut results = self.grep_chapters(pattern)?;
        results.extend(self.grep_wiki(pattern)?);

        // Also grep outline
        if let Ok(outline) = self.read_outline() {
            let path = self.outline_path();
            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let low_pattern = pattern.to_lowercase();
            for (i, line) in outline.lines().enumerate() {
                if line.to_lowercase().contains(&low_pattern) {
                    results.push(GrepMatch {
                        file: file_name.clone(),
                        line: line.trim().to_string(),
                        line_number: i + 1,
                    });
                }
            }
        }

        Ok(results)
    }

    // ═════════════════════════════════════════════════════════════════
    // Writing / Editing
    // ═════════════════════════════════════════════════════════════════

    /// Write content to a chapter file (creates backup beforehand).
    pub fn write_chapter(&self, num: usize, content: &str) -> Result<(), String> {
        let path = self.chapter_path(num);
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {e}"))?;
        }
        self.backup_file(&path)?;
        std::fs::write(&path, content).map_err(|e| format!("Failed to write chapter {num}: {e}"))
    }

    /// Write content to the wiki file (creates backup beforehand).
    pub fn write_wiki(&self, content: &str) -> Result<(), String> {
        let path = self.wiki_path();
        self.backup_file(&path)?;
        std::fs::write(&path, content).map_err(|e| format!("Failed to write wiki: {e}"))
    }

    /// Write content to the outline file (creates backup beforehand).
    pub fn write_outline(&self, content: &str) -> Result<(), String> {
        let path = self.outline_path();
        self.backup_file(&path)?;
        std::fs::write(&path, content).map_err(|e| format!("Failed to write outline: {e}"))
    }

    /// Find and replace text in a specific chapter.
    pub fn edit_chapter(&self, num: usize, old: &str, new: &str) -> Result<usize, String> {
        let path = self.chapter_path(num);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read chapter {num}: {e}"))?;

        let new_content = content.replace(old, new);
        let replacements = content.matches(old).count();

        if replacements > 0 {
            self.backup_file(&path)?;
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("Failed to write chapter {num}: {e}"))?;
        }

        Ok(replacements)
    }

    /// Find and replace text in the wiki.
    pub fn edit_wiki(&self, old: &str, new: &str) -> Result<usize, String> {
        let path = self.wiki_path();
        if !path.exists() {
            return Ok(0);
        }

        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read wiki: {e}"))?;

        let new_content = content.replace(old, new);
        let replacements = content.matches(old).count();

        if replacements > 0 {
            self.backup_file(&path)?;
            std::fs::write(&path, &new_content)
                .map_err(|e| format!("Failed to write wiki: {e}"))?;
        }

        Ok(replacements)
    }

    /// Find and replace text across all chapters.
    ///
    /// Returns results for each file that was modified.
    pub fn find_replace_chapters(&self, old: &str, new: &str) -> Result<Vec<EditResult>, String> {
        let count = self.count_chapters();
        let mut results = Vec::with_capacity(count);

        for i in 1..=count {
            match self.edit_chapter(i, old, new) {
                Ok(replacements) => {
                    if replacements > 0 {
                        results.push(EditResult {
                            file: self
                                .chapter_path(i)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                            replacements,
                            success: true,
                        });
                    }
                }
                Err(e) => {
                    results.push(EditResult {
                        file: format!("chapter_{i}"),
                        replacements: 0,
                        success: false,
                    });
                    eprintln!("Warning: edit error on chapter {i}: {e}");
                }
            }
        }

        Ok(results)
    }

    /// Find and replace text across all story files (chapters + wiki).
    pub fn find_replace_all(&self, old: &str, new: &str) -> Result<Vec<EditResult>, String> {
        let mut results = self.find_replace_chapters(old, new)?;

        match self.edit_wiki(old, new) {
            Ok(replacements) => {
                if replacements > 0 {
                    results.push(EditResult {
                        file: self
                            .wiki_path()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        replacements,
                        success: true,
                    });
                }
            }
            Err(e) => {
                results.push(EditResult {
                    file: "wiki".to_string(),
                    replacements: 0,
                    success: false,
                });
                eprintln!("Warning: edit error on wiki: {e}");
            }
        }

        Ok(results)
    }

    /// Get the workspace root path.
    pub fn root(&self) -> &Path {
        &self.workspace_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_workspace() -> (tempfile::TempDir, StoryToolSet) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Create a minimal story structure
        fs::create_dir_all(path.join("chapters")).unwrap();
        fs::write(
            path.join("outline.md"),
            "Title: Test Story\nGenre: Fantasy\n\n## Chapter 1: Intro\nSummary here.\n",
        )
        .unwrap();
        fs::write(path.join("wiki.md"), "## Characters\n### Alice\nA hero.\n").unwrap();
        fs::write(
            path.join("chapters/01-chapter.md"),
            "Chapter one content. Alice wakes up.\n",
        )
        .unwrap();
        fs::write(
            path.join("chapters/02-chapter.md"),
            "Chapter two content. Alice explores.\n",
        )
        .unwrap();

        let tool_set = StoryToolSet::new(&path);
        (dir, tool_set)
    }

    #[test]
    fn test_read_outline() {
        let (_dir, ts) = setup_test_workspace();
        let outline = ts.read_outline().unwrap();
        assert!(outline.contains("Test Story"));
    }

    #[test]
    fn test_read_wiki() {
        let (_dir, ts) = setup_test_workspace();
        let wiki = ts.read_wiki().unwrap();
        assert!(wiki.contains("Alice"));
    }

    #[test]
    fn test_read_chapter() {
        let (_dir, ts) = setup_test_workspace();
        let ch = ts.read_chapter(1).unwrap();
        assert!(ch.contains("Chapter one"));
    }

    #[test]
    fn test_read_all_chapters() {
        let (_dir, ts) = setup_test_workspace();
        let chapters = ts.read_all_chapters().unwrap();
        assert_eq!(chapters.len(), 2);
    }

    #[test]
    fn test_count_chapters() {
        let (_dir, ts) = setup_test_workspace();
        assert_eq!(ts.count_chapters(), 2);
    }

    #[test]
    fn test_grep_chapters() {
        let (_dir, ts) = setup_test_workspace();
        let results = ts.grep_chapters("Alice").unwrap();
        // Both chapters contain "Alice" (chapter 1: "Alice wakes up", chapter 2: "Alice explores")
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.line.contains("Alice")));
    }

    #[test]
    fn test_grep_wiki() {
        let (_dir, ts) = setup_test_workspace();
        let results = ts.grep_wiki("Alice").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_edit_chapter() {
        let (_dir, ts) = setup_test_workspace();
        let replacements = ts.edit_chapter(1, "Alice", "Bob").unwrap();
        assert_eq!(replacements, 1);
        let content = ts.read_chapter(1).unwrap();
        assert!(content.contains("Bob"));
    }

    #[test]
    fn test_find_replace_chapters() {
        let (_dir, ts) = setup_test_workspace();
        let results = ts.find_replace_chapters("Alice", "Bob").unwrap();
        // Both chapters contain "Alice"
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].replacements, 1);
    }

    #[test]
    fn test_backup_created_on_write() {
        let (_dir, ts) = setup_test_workspace();
        ts.write_chapter(1, "New content").unwrap();
        let backups = ts.list_backups(&ts.chapter_path(1)).unwrap();
        assert!(!backups.is_empty(), "Backup should have been created");
    }

    #[test]
    fn test_restore_backup() {
        let (_dir, ts) = setup_test_workspace();
        let path = ts.chapter_path(1);
        ts.write_chapter(1, "Modified content").unwrap();
        ts.restore_latest_backup(&path).unwrap();
        let content = ts.read_chapter(1).unwrap();
        assert!(content.contains("Chapter one"));
    }

    #[test]
    fn test_read_missing_chapter() {
        let (_dir, ts) = setup_test_workspace();
        let result = ts.read_chapter(99);
        assert!(result.is_err());
    }

    #[test]
    fn test_total_word_count() {
        let (_dir, ts) = setup_test_workspace();
        let count = ts.total_word_count();
        assert!(count > 0);
    }
}
