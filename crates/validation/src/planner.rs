//! Outline diff & modification planning.
//!
//! Tracks outline snapshots per session and computes diffs to detect:
//! - Chapters added, removed, or renamed
//! - Chapter summary changes
//! - Plot arc or metadata changes
//!
//! Also generates modification plans from diffs using the model.
//!
//! # Usage
//!
//! ```ignore
//! let diff = OutlineDiff::compute(&old_outline, &new_outline);
//! println!("{}", diff.summary);
//! ```

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

/// A single change detected between two outline snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OutlineChange {
    ChapterAdded {
        number: usize,
        title: String,
    },
    ChapterRemoved {
        number: usize,
        title: String,
    },
    ChapterRenamed {
        number: usize,
        old_title: String,
        new_title: String,
    },
    ChapterSummaryChanged {
        number: usize,
        summary_old: String,
        summary_new: String,
    },
    PlotArcChanged {
        description: String,
    },
    MetadataChanged {
        field: String,
        old: String,
        new: String,
    },
}

/// The complete diff between two outlines.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutlineDiff {
    pub changes: Vec<OutlineChange>,
    pub summary: String,
}

impl OutlineDiff {
    /// Compute a diff between two outline texts.
    ///
    /// Both outlines should be markdown with `## Chapter N: Title` headings
    /// and summary paragraphs. Metadata (Title:, Genre:, Tone:) is tracked
    /// from the first few lines.
    pub fn compute(old_text: &str, new_text: &str) -> Self {
        let old_chapters = parse_outline_chapters(old_text);
        let new_chapters = parse_outline_chapters(new_text);
        let old_meta = parse_outline_metadata(old_text);
        let new_meta = parse_outline_metadata(new_text);

        let mut changes = Vec::new();

        // Detect metadata changes
        for (field, old_val) in &old_meta {
            if let Some(new_val) = new_meta.get(field) {
                if old_val != new_val {
                    changes.push(OutlineChange::MetadataChanged {
                        field: field.clone(),
                        old: old_val.clone(),
                        new: new_val.clone(),
                    });
                }
            }
        }

        // Detect added/removed chapters by number
        let old_by_num: std::collections::HashMap<usize, &ChapterInfo> =
            old_chapters.iter().map(|c| (c.number, c)).collect();
        let new_by_num: std::collections::HashMap<usize, &ChapterInfo> =
            new_chapters.iter().map(|c| (c.number, c)).collect();

        // Find removed chapters
        for (num, old_ch) in &old_by_num {
            if !new_by_num.contains_key(num) {
                changes.push(OutlineChange::ChapterRemoved {
                    number: *num,
                    title: old_ch.title.clone(),
                });
            }
        }

        // Find added / renamed / summary-changed chapters
        for (num, new_ch) in &new_by_num {
            match old_by_num.get(num) {
                None => {
                    changes.push(OutlineChange::ChapterAdded {
                        number: *num,
                        title: new_ch.title.clone(),
                    });
                }
                Some(old_ch) => {
                    if old_ch.title != new_ch.title {
                        changes.push(OutlineChange::ChapterRenamed {
                            number: *num,
                            old_title: old_ch.title.clone(),
                            new_title: new_ch.title.clone(),
                        });
                    }
                    if old_ch.summary != new_ch.summary {
                        changes.push(OutlineChange::ChapterSummaryChanged {
                            number: *num,
                            summary_old: old_ch.summary.clone(),
                            summary_new: new_ch.summary.clone(),
                        });
                    }
                }
            }
        }

        // Generate summary
        let summary = Self::generate_summary_text(&changes);

        Self { changes, summary }
    }

    fn generate_summary_text(changes: &[OutlineChange]) -> String {
        if changes.is_empty() {
            return "✅ No changes to the outline.".to_string();
        }

        let mut parts = Vec::new();
        let mut added = 0;
        let mut removed = 0;
        let mut renamed = 0;
        let mut summary_changed = 0;

        for change in changes {
            match change {
                OutlineChange::ChapterAdded { .. } => added += 1,
                OutlineChange::ChapterRemoved { .. } => removed += 1,
                OutlineChange::ChapterRenamed { .. } => renamed += 1,
                OutlineChange::ChapterSummaryChanged { .. } => summary_changed += 1,
                OutlineChange::PlotArcChanged { .. } => {
                    parts.push("Plot arc has been updated.".to_string());
                }
                OutlineChange::MetadataChanged { field, .. } => {
                    parts.push(format!("Metadata field '{field}' has changed."));
                }
            }
        }

        let mut detail_parts = Vec::new();
        if added > 0 {
            detail_parts.push(format!("{added} chapter(s) added"));
        }
        if removed > 0 {
            detail_parts.push(format!("{removed} chapter(s) removed"));
        }
        if renamed > 0 {
            detail_parts.push(format!("{renamed} chapter(s) renamed"));
        }
        if summary_changed > 0 {
            detail_parts.push(format!("{summary_changed} chapter summary/summaries changed"));
        }

        let detail = detail_parts.join(", ");
        let change_count = changes.len();
        if !detail.is_empty() {
            parts.push(format!("{change_count} change(s): {detail}."));
        }

        parts.join(" ")
    }
}

/// Parsed chapter info from an outline.
#[derive(Debug, Clone)]
struct ChapterInfo {
    number: usize,
    title: String,
    summary: String,
}

/// Parse chapter headings and summaries from outline markdown.
///
/// Expects format:
/// ```markdown
/// ## Chapter 1: Title
/// Summary paragraph...
///
/// ## Chapter 2: Another Title
/// More summary...
/// ```
fn parse_outline_chapters(text: &str) -> Vec<ChapterInfo> {
    let mut chapters = Vec::new();
    let mut current_number: Option<usize> = None;
    let mut current_title = String::new();
    let mut current_summary_lines = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Try to parse "## Chapter N: Title" or "## Chapter N - Title"
        if let Some(cap) = parse_chapter_heading(trimmed) {
            // Save previous chapter
            if let Some(num) = current_number {
                chapters.push(ChapterInfo {
                    number: num,
                    title: current_title.clone(),
                    summary: current_summary_lines.join(" ").trim().to_string(),
                });
            }
            current_number = Some(cap.0);
            current_title = cap.1;
            current_summary_lines.clear();
        } else if current_number.is_some() && !trimmed.is_empty() && !trimmed.starts_with('#'){
            current_summary_lines.push(trimmed);
        }
    }

    // Save last chapter
    if let Some(num) = current_number {
        chapters.push(ChapterInfo {
            number: num,
            title: current_title,
            summary: current_summary_lines.join(" ").trim().to_string(),
        });
    }

    chapters
}

/// Parse a chapter heading like "## Chapter 1: The Beginning"
/// Returns (number, title).
fn parse_chapter_heading(line: &str) -> Option<(usize, String)> {
    let line = line.trim_start_matches('#').trim();
    if !line.to_lowercase().starts_with("chapter ") {
        return None;
    }

    let rest = line.trim_start_matches(|c: char| c.is_alphabetic() || c == ' ');
    // Now rest should start with a number
    let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let number = num_str.parse::<usize>().ok()?;

    // After the number, expect ':' or '-' then title
    let after_num = &rest[num_str.len()..];
    let title = after_num
        .trim_start_matches(|c: char| c == ':' || c == '-' || c == ' ')
        .to_string();

    Some((number, title))
}

/// Parse metadata from outline top (Title:, Genre:, Tone:).
fn parse_outline_metadata(text: &str) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();

    for line in text.lines().take(20) {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("Title:") {
            meta.insert("title".to_string(), title.trim().to_string());
        } else if let Some(genre) = trimmed.strip_prefix("Genre:") {
            meta.insert("genre".to_string(), genre.trim().to_string());
        } else if let Some(tone) = trimmed.strip_prefix("Tone:") {
            meta.insert("tone".to_string(), tone.trim().to_string());
        } else if trimmed.starts_with('#') {
            // Stop at first heading — metadata should be before that
            break;
        }
    }

    meta
}

// ═════════════════════════════════════════════════════════════════════════════
// Modification Plan
// ═════════════════════════════════════════════════════════════════════════════

/// A plan describing what changes are needed to bring chapters in line with
/// the current outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModificationPlan {
    pub affected_chapters: Vec<usize>,
    pub changes_required: Vec<String>,
    pub preserves_continuity: bool,
    pub recommended_approach: String,
    pub estimated_effort: String,
}

impl ModificationPlan {
    /// Schema for grammar-constrained generation of modification plans.
    pub fn schema() -> Schema {
        Schema::object()
            .prop("affected_chapters", Schema::array(Schema::integer()))
            .prop("changes_required", Schema::array(Schema::string()))
            .prop("preserves_continuity", Schema::boolean())
            .prop("recommended_approach", Schema::string())
            .prop(
                "estimated_effort",
                Schema::enum_values(vec![
                    serde_json::json!("minor"),
                    serde_json::json!("moderate"),
                    serde_json::json!("major rewrite"),
                ]),
            )
            .build()
    }

    /// Generate a modification plan from an outline diff using the model.
    pub fn generate(
        backend: &dyn ModelBackend,
        diff: &OutlineDiff,
        chapter_count: usize,
    ) -> Result<Self, String> {
        let prompt = format!(
            "Given the following outline changes for a story with {chapter_count} chapters, \
             create a modification plan.\n\nChanges:\n{}\n\n\
             Output JSON with:\n\
             - affected_chapters: array of chapter numbers that need changes\n\
             - changes_required: array of specific changes needed\n\
             - preserves_continuity: boolean, whether the plan maintains story continuity\n\
             - recommended_approach: string describing the recommended approach\n\
             - estimated_effort: one of \"minor\", \"moderate\", \"major rewrite\"",
            diff.summary,
        );

        let schema = Self::schema();
        let grammar = schema.to_gbnf("ModificationPlan").ok();

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: "You are a story planning assistant. Output valid JSON only. \
                     No thinking, no reasoning, only JSON."
                .to_string(),
            prompt,
            grammar,
            temperature: 0.4,
            max_tokens: 400,
            prefill: Some("{\n".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        serde_json::from_str::<Self>(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chapter_heading() {
        let (num, title) = parse_chapter_heading("## Chapter 1: The Beginning").unwrap();
        assert_eq!(num, 1);
        assert_eq!(title, "The Beginning");

        let (num, title) = parse_chapter_heading("## Chapter 2 - Continuation").unwrap();
        assert_eq!(num, 2);
        assert_eq!(title, "Continuation");
    }

    #[test]
    fn test_parse_outline_chapters() {
        let text = "Title: Test\n\n## Chapter 1: Intro\nFirst chapter summary.\n\n## Chapter 2: Middle\nSecond chapter.\n";
        let chapters = parse_outline_chapters(text);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].number, 1);
        assert_eq!(chapters[0].title, "Intro");
        assert_eq!(chapters[1].number, 2);
        assert_eq!(chapters[1].title, "Middle");
    }

    #[test]
    fn test_outline_diff_no_changes() {
        let text = "## Chapter 1: Intro\nFirst chapter.\n## Chapter 2: Middle\nSecond chapter.\n";
        let diff = OutlineDiff::compute(text, text);
        assert_eq!(diff.changes.len(), 0);
        assert!(diff.summary.contains("No changes"));
    }

    #[test]
    fn test_outline_detect_chapter_rename() {
        let old = "## Chapter 1: Intro\nFirst chapter.\n## Chapter 2: Middle\nSecond chapter.\n";
        let new = "## Chapter 1: Introduction\nFirst chapter.\n## Chapter 2: Middle\nSecond chapter.\n";
        let diff = OutlineDiff::compute(old, new);
        assert!(diff.changes.iter().any(|c| matches!(c, OutlineChange::ChapterRenamed { .. })));
    }

    #[test]
    fn test_outline_detect_chapter_added() {
        let old = "## Chapter 1: Intro\nFirst chapter.\n";
        let new = "## Chapter 1: Intro\nFirst chapter.\n## Chapter 2: New\nNew chapter.\n";
        let diff = OutlineDiff::compute(old, new);
        assert!(diff.changes.iter().any(|c| matches!(c, OutlineChange::ChapterAdded { .. })));
    }

    #[test]
    fn test_outline_detect_chapter_removed() {
        let old = "## Chapter 1: Intro\nFirst chapter.\n## Chapter 2: Middle\nSecond chapter.\n";
        let new = "## Chapter 1: Intro\nFirst chapter.\n";
        let diff = OutlineDiff::compute(old, new);
        assert!(diff.changes.iter().any(|c| matches!(c, OutlineChange::ChapterRemoved { .. })));
    }

    #[test]
    fn test_outline_detect_metadata_change() {
        let old = "Title: Old Title\nGenre: Fantasy\n## Chapter 1: Intro\nFirst chapter.\n";
        let new = "Title: New Title\nGenre: Fantasy\n## Chapter 1: Intro\nFirst chapter.\n";
        let diff = OutlineDiff::compute(old, new);
        assert!(diff.changes.iter().any(|c| matches!(c, OutlineChange::MetadataChanged { field, .. } if field == "title")));
    }

    #[test]
    fn test_outline_detect_summary_change() {
        let old = "## Chapter 1: Intro\nFirst chapter summary.\n## Chapter 2: Middle\nSecond chapter summary.\n";
        let new = "## Chapter 1: Intro\nUpdated first chapter summary.\n## Chapter 2: Middle\nSecond chapter summary.\n";
        let diff = OutlineDiff::compute(old, new);
        assert!(diff.changes.iter().any(|c| matches!(c, OutlineChange::ChapterSummaryChanged { .. })));
    }

    #[test]
    fn test_modification_plan_schema() {
        let schema = ModificationPlan::schema();
        let gbnf = schema.to_gbnf("ModificationPlan");
        assert!(gbnf.is_ok());
    }
}
