//! Story Persistence — save/load story state for long-running stories.
//!
//! Enables resuming story generation across sessions:
//! - Save story state (outline, chapters, plot state, revisions)
//! - Load story state from workspace
//! - Resume generation from where left off
//!
//! # Storage Format
//!
//! Stories are saved as JSON in the workspace:
//! - `story-state.json` — complete story state
//! - `01-OUTLINE.md` — outline (already exists)
//! - `03-CHAPTER_*.md` — chapters (already exist)
//! - `07-PLOT-STATE.json` — plot state (already exists)
//! - `08-QUALITY-*.md` — quality reports (already exist)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::story_engine::{ChapterInfo, PlotState, RevisionRecord};

// ═════════════════════════════════════════════════════════════════════════════
// Story State — serializable state for persistence
// ═════════════════════════════════════════════════════════════════════════════

/// Complete story state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryState {
    /// Story premise
    pub premise: String,
    /// Current outline
    pub outline: Vec<ChapterInfo>,
    /// Generated chapters (content)
    pub chapters: Vec<String>,
    /// Current chapter number
    pub current_chapter: usize,
    /// Plot state
    pub plot_state: PlotState,
    /// Revision history
    pub revisions: Vec<RevisionRecord>,
    /// Chapter quality scores (overall only)
    pub chapter_scores: Vec<f32>,
    /// Timestamp of last save
    pub last_saved: u64,
    /// Story metadata
    pub metadata: StoryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryMetadata {
    /// Story title (from outline)
    pub title: String,
    /// Genre
    pub genre: String,
    /// Total word count
    pub word_count: usize,
    /// Total chapter count
    pub chapter_count: usize,
    /// Average quality score
    pub average_quality: f32,
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Persistence Manager
// ═════════════════════════════════════════════════════════════════════════════

/// Manages saving and loading story state.
pub struct StoryPersistence {
    workspace_path: PathBuf,
}

impl StoryPersistence {
    /// Create a new persistence manager for a workspace.
    pub fn new(workspace_path: PathBuf) -> Self {
        Self { workspace_path }
    }

    /// Save story state to workspace.
    pub fn save(&self, state: &StoryState) -> Result<(), String> {
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| format!("failed to serialize state: {e}"))?;

        let path = self.workspace_path.join("story-state.json");
        std::fs::write(&path, json).map_err(|e| format!("failed to write state: {e}"))?;

        Ok(())
    }

    /// Load story state from workspace.
    pub fn load(&self) -> Result<StoryState, String> {
        let path = self.workspace_path.join("story-state.json");
        if !path.exists() {
            return Err("story-state.json not found".to_string());
        }

        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("failed to read state: {e}"))?;

        let state: StoryState =
            serde_json::from_str(&json).map_err(|e| format!("failed to parse state: {e}"))?;

        Ok(state)
    }

    /// Check if a story state exists in the workspace.
    pub fn exists(&self) -> bool {
        self.workspace_path.join("story-state.json").exists()
    }

    /// List all saved stories in a base directory.
    pub fn list_stories(base_dir: &Path) -> Result<Vec<StorySummary>, String> {
        let mut stories = Vec::new();

        if !base_dir.exists() {
            return Ok(stories);
        }

        for entry in
            std::fs::read_dir(base_dir).map_err(|e| format!("failed to read directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("failed to read entry: {e}"))?;
            let path = entry.path();

            if path.is_dir() {
                let state_path = path.join("story-state.json");
                if state_path.exists() {
                    if let Ok(state) = StoryPersistence::new(path.clone()).load() {
                        stories.push(StorySummary {
                            workspace_path: path,
                            title: state.metadata.title,
                            chapter_count: state.metadata.chapter_count,
                            word_count: state.metadata.word_count,
                            average_quality: state.metadata.average_quality,
                            last_saved: state.last_saved,
                        });
                    }
                }
            }
        }

        // Sort by last saved (most recent first)
        stories.sort_by(|a, b| b.last_saved.cmp(&a.last_saved));

        Ok(stories)
    }
}

/// Summary of a saved story (for listing).
#[derive(Debug, Clone)]
pub struct StorySummary {
    pub workspace_path: PathBuf,
    pub title: String,
    pub chapter_count: usize,
    pub word_count: usize,
    pub average_quality: f32,
    pub last_saved: u64,
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═════════════════════════════════════════════════════════════════════════════

/// Count words in text
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Extract title from outline
pub fn extract_title_from_outline(outline: &[ChapterInfo]) -> String {
    outline
        .first()
        .map(|ch| ch.title.clone())
        .unwrap_or_else(|| "Untitled".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_save_and_load() {
        let temp_dir = std::env::temp_dir().join("roco_test_persistence");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let persistence = StoryPersistence::new(temp_dir.clone());

        let state = StoryState {
            premise: "Test premise".into(),
            outline: vec![ChapterInfo {
                number: 1,
                title: "Chapter 1".into(),
                summary: "The beginning".into(),
            }],
            chapters: vec!["# Chapter 1\n\nContent.".into()],
            current_chapter: 1,
            plot_state: PlotState::default(),
            revisions: vec![],
            chapter_scores: vec![7.0],
            last_saved: 1234567890,
            metadata: StoryMetadata {
                title: "Test Story".into(),
                genre: "Fantasy".into(),
                word_count: 100,
                chapter_count: 1,
                average_quality: 7.0,
            },
        };

        // Save
        persistence.save(&state).unwrap();

        // Load
        let loaded = persistence.load().unwrap();
        assert_eq!(loaded.premise, "Test premise");
        assert_eq!(loaded.chapters.len(), 1);
        assert_eq!(loaded.metadata.title, "Test Story");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_exists() {
        let temp_dir = std::env::temp_dir().join("roco_test_exists");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let persistence = StoryPersistence::new(temp_dir.clone());
        assert!(!persistence.exists());

        let state = StoryState {
            premise: "Test".into(),
            outline: vec![],
            chapters: vec![],
            current_chapter: 0,
            plot_state: PlotState::default(),
            revisions: vec![],
            chapter_scores: vec![],
            last_saved: 0,
            metadata: StoryMetadata {
                title: "Test".into(),
                genre: "Test".into(),
                word_count: 0,
                chapter_count: 0,
                average_quality: 0.0,
            },
        };

        persistence.save(&state).unwrap();
        assert!(persistence.exists());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_count_words() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words("  hello   world  "), 2);
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("one"), 1);
    }
}
