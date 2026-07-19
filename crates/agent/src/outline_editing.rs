//! Outline Editing — collaborative outline creation and modification.
//!
//! The human and AI co-create the outline:
//! - AI generates initial outline
//! - Human can add, remove, reorder, modify chapters
//! - AI suggests improvements
//! - Human approves final outline

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{schema_to_gbnf, Schema};
use serde::{Deserialize, Serialize};

use super::story_engine::ChapterInfo;

// ═════════════════════════════════════════════════════════════════════════════
// Outline Edit Commands
// ═════════════════════════════════════════════════════════════════════════════

/// Commands for editing the outline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutlineCommand {
    /// Add a new chapter at position
    Add {
        position: usize,
        title: String,
        summary: String,
    },
    /// Remove a chapter
    Remove { position: usize },
    /// Move a chapter from one position to another
    Move { from: usize, to: usize },
    /// Modify a chapter's title or summary
    Modify {
        position: usize,
        title: Option<String>,
        summary: Option<String>,
    },
    /// Regenerate a chapter's summary
    Regenerate { position: usize },
    /// Add a chapter before another
    AddBefore {
        reference: usize,
        title: String,
        summary: String,
    },
    /// Add a chapter after another
    AddAfter {
        reference: usize,
        title: String,
        summary: String,
    },
}

/// Result of an outline edit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineEditResult {
    /// Whether the edit was successful
    pub success: bool,
    /// The updated outline
    pub outline: Vec<ChapterInfo>,
    /// Message about what was done
    pub message: String,
    /// Suggestions for further edits
    pub suggestions: Vec<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Outline Editor
// ═════════════════════════════════════════════════════════════════════════════

/// Collaborative outline editor
pub struct OutlineEditor {
    /// Current outline
    outline: Vec<ChapterInfo>,
    /// Edit history
    history: Vec<OutlineEditResult>,
}

impl OutlineEditor {
    /// Create a new outline editor
    pub fn new(outline: Vec<ChapterInfo>) -> Self {
        Self {
            outline,
            history: Vec::new(),
        }
    }

    /// Get the current outline
    pub fn outline(&self) -> &[ChapterInfo] {
        &self.outline
    }

    /// Get edit history
    pub fn history(&self) -> &[OutlineEditResult] {
        &self.history
    }

    /// Execute an outline command
    pub fn execute(&mut self, command: OutlineCommand) -> OutlineEditResult {
        let result = match command {
            OutlineCommand::Add {
                position,
                title,
                summary,
            } => self.add_chapter(position, title, summary),
            OutlineCommand::Remove { position } => self.remove_chapter(position),
            OutlineCommand::Move { from, to } => self.move_chapter(from, to),
            OutlineCommand::Modify {
                position,
                title,
                summary,
            } => self.modify_chapter(position, title, summary),
            OutlineCommand::Regenerate { position } => self.regenerate_chapter(position),
            OutlineCommand::AddBefore {
                reference,
                title,
                summary,
            } => self.add_before(reference, title, summary),
            OutlineCommand::AddAfter {
                reference,
                title,
                summary,
            } => self.add_after(reference, title, summary),
        };

        self.history.push(result.clone());
        result
    }

    /// Add a chapter at position
    fn add_chapter(
        &mut self,
        position: usize,
        title: String,
        summary: String,
    ) -> OutlineEditResult {
        if position > self.outline.len() {
            return OutlineEditResult {
                success: false,
                outline: self.outline.clone(),
                message: format!(
                    "Position {} is out of bounds (max: {})",
                    position,
                    self.outline.len()
                ),
                suggestions: vec![format!("Use position 0-{}", self.outline.len())],
            };
        }

        let chapter = ChapterInfo {
            number: (position + 1) as u64,
            title,
            summary,
        };

        self.outline.insert(position, chapter);
        self.renumber();

        OutlineEditResult {
            success: true,
            outline: self.outline.clone(),
            message: format!("Added chapter at position {}", position + 1),
            suggestions: Vec::new(),
        }
    }

    /// Remove a chapter
    fn remove_chapter(&mut self, position: usize) -> OutlineEditResult {
        if position >= self.outline.len() {
            return OutlineEditResult {
                success: false,
                outline: self.outline.clone(),
                message: format!(
                    "Position {} is out of bounds (max: {})",
                    position,
                    self.outline.len() - 1
                ),
                suggestions: Vec::new(),
            };
        }

        let removed = self.outline.remove(position);
        self.renumber();

        OutlineEditResult {
            success: true,
            outline: self.outline.clone(),
            message: format!("Removed chapter {}: {}", removed.number, removed.title),
            suggestions: Vec::new(),
        }
    }

    /// Move a chapter
    fn move_chapter(&mut self, from: usize, to: usize) -> OutlineEditResult {
        if from >= self.outline.len() || to >= self.outline.len() {
            return OutlineEditResult {
                success: false,
                outline: self.outline.clone(),
                message: format!(
                    "Invalid positions: {} -> {} (max: {})",
                    from,
                    to,
                    self.outline.len() - 1
                ),
                suggestions: Vec::new(),
            };
        }

        let chapter = self.outline.remove(from);
        self.outline.insert(to, chapter);
        self.renumber();

        OutlineEditResult {
            success: true,
            outline: self.outline.clone(),
            message: format!("Moved chapter from position {} to {}", from + 1, to + 1),
            suggestions: Vec::new(),
        }
    }

    /// Modify a chapter
    fn modify_chapter(
        &mut self,
        position: usize,
        title: Option<String>,
        summary: Option<String>,
    ) -> OutlineEditResult {
        if position >= self.outline.len() {
            return OutlineEditResult {
                success: false,
                outline: self.outline.clone(),
                message: format!(
                    "Position {} is out of bounds (max: {})",
                    position,
                    self.outline.len() - 1
                ),
                suggestions: Vec::new(),
            };
        }

        if let Some(t) = title {
            self.outline[position].title = t;
        }
        if let Some(s) = summary {
            self.outline[position].summary = s;
        }

        OutlineEditResult {
            success: true,
            outline: self.outline.clone(),
            message: format!("Modified chapter {}", position + 1),
            suggestions: Vec::new(),
        }
    }

    /// Regenerate a chapter's summary (placeholder - needs model)
    fn regenerate_chapter(&mut self, position: usize) -> OutlineEditResult {
        if position >= self.outline.len() {
            return OutlineEditResult {
                success: false,
                outline: self.outline.clone(),
                message: format!("Position {} is out of bounds", position),
                suggestions: Vec::new(),
            };
        }

        // For now, just mark as needing regeneration
        OutlineEditResult {
            success: true,
            outline: self.outline.clone(),
            message: format!("Chapter {} marked for regeneration", position + 1),
            suggestions: vec!["Use AI to regenerate the summary".to_string()],
        }
    }

    /// Add a chapter before another
    fn add_before(
        &mut self,
        reference: usize,
        title: String,
        summary: String,
    ) -> OutlineEditResult {
        self.add_chapter(reference, title, summary)
    }

    /// Add a chapter after another
    fn add_after(&mut self, reference: usize, title: String, summary: String) -> OutlineEditResult {
        self.add_chapter(reference + 1, title, summary)
    }

    /// Renumber all chapters
    fn renumber(&mut self) {
        for (i, chapter) in self.outline.iter_mut().enumerate() {
            chapter.number = (i + 1) as u64;
        }
    }

    /// Parse a natural language edit command
    pub fn parse_command(&self, input: &str) -> Option<OutlineCommand> {
        let lower = input.trim().to_lowercase();

        // Add chapter
        if lower.starts_with("add ") || lower.starts_with("insert ") {
            let rest = if lower.starts_with("add ") {
                &input[4..]
            } else {
                &input[7..]
            };

            // Try to extract position and content
            if let Some((pos_str, content)) = rest.split_once(' ') {
                if let Ok(pos) = pos_str.parse::<usize>() {
                    if let Some((title, summary)) = content.split_once(':') {
                        return Some(OutlineCommand::Add {
                            position: pos - 1,
                            title: title.trim().to_string(),
                            summary: summary.trim().to_string(),
                        });
                    }
                }
            }
        }

        // Remove chapter
        if lower.starts_with("remove ") || lower.starts_with("delete ") {
            let rest = if lower.starts_with("remove ") {
                &input[7..]
            } else {
                &input[7..]
            };

            if let Ok(pos) = rest.trim().parse::<usize>() {
                return Some(OutlineCommand::Remove { position: pos - 1 });
            }
        }

        // Move chapter
        if lower.starts_with("move ") {
            let rest = &input[5..];
            if let Some((from_str, to_str)) = rest.split_once(" to ") {
                if let (Ok(from), Ok(to)) = (
                    from_str.trim().parse::<usize>(),
                    to_str.trim().parse::<usize>(),
                ) {
                    return Some(OutlineCommand::Move {
                        from: from - 1,
                        to: to - 1,
                    });
                }
            }
        }

        None
    }

    /// Get a summary of the current outline
    pub fn summary(&self) -> String {
        let mut summary = String::new();
        summary.push_str(&format!("Outline ({} chapters):\n\n", self.outline.len()));

        for chapter in &self.outline {
            summary.push_str(&format!(
                "{}. {}\n   {}\n\n",
                chapter.number, chapter.title, chapter.summary
            ));
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_outline() -> Vec<ChapterInfo> {
        vec![
            ChapterInfo {
                number: 1,
                title: "The Beginning".to_string(),
                summary: "Introduction of the protagonist".to_string(),
            },
            ChapterInfo {
                number: 2,
                title: "The Journey".to_string(),
                summary: "The protagonist sets out on a quest".to_string(),
            },
            ChapterInfo {
                number: 3,
                title: "The End".to_string(),
                summary: "Resolution of the conflict".to_string(),
            },
        ]
    }

    #[test]
    fn test_add_chapter() {
        let mut editor = OutlineEditor::new(sample_outline());
        let result = editor.execute(OutlineCommand::Add {
            position: 1,
            title: "New Chapter".to_string(),
            summary: "A new chapter".to_string(),
        });

        assert!(result.success);
        assert_eq!(result.outline.len(), 4);
        assert_eq!(result.outline[1].title, "New Chapter");
        assert_eq!(result.outline[1].number, 2);
    }

    #[test]
    fn test_remove_chapter() {
        let mut editor = OutlineEditor::new(sample_outline());
        let result = editor.execute(OutlineCommand::Remove { position: 1 });

        assert!(result.success);
        assert_eq!(result.outline.len(), 2);
        assert_eq!(result.outline[0].title, "The Beginning");
        assert_eq!(result.outline[1].title, "The End");
    }

    #[test]
    fn test_move_chapter() {
        let mut editor = OutlineEditor::new(sample_outline());
        let result = editor.execute(OutlineCommand::Move { from: 0, to: 2 });

        assert!(result.success);
        assert_eq!(result.outline[0].title, "The Journey");
        assert_eq!(result.outline[2].title, "The Beginning");
    }

    #[test]
    fn test_parse_command_add() {
        let editor = OutlineEditor::new(sample_outline());
        let cmd = editor.parse_command("add 2 New Chapter: A new chapter");
        assert!(cmd.is_some());
    }

    #[test]
    fn test_parse_command_remove() {
        let editor = OutlineEditor::new(sample_outline());
        let cmd = editor.parse_command("remove 2");
        assert!(cmd.is_some());
    }

    #[test]
    fn test_parse_command_move() {
        let editor = OutlineEditor::new(sample_outline());
        let cmd = editor.parse_command("move 1 to 3");
        assert!(cmd.is_some());
    }
}
