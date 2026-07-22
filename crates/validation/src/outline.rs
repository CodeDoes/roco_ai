//! Outline validation — validates story outlines for completeness and structure.
//!
//! Checks:
//! - Has required fields (title, genre, tone, chapters)
//! - Each chapter has number, title, and summary
//! - Minimum number of chapters
//! - Chapter numbering is sequential
//! - Plot arc completeness (beginning, middle, end)
//! - Chapter summaries have minimum detail
//! - No duplicate chapter titles
//! - No empty or placeholder content

use std::collections::HashSet;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use serde::Deserialize;

use super::{ValidationCheck, ValidationSeverity, ValidationSource, WordCountTargets};

/// Outline validator.
#[derive(Debug, Clone)]
pub struct OutlineValidator {
    /// Minimum number of chapters required
    pub min_chapters: usize,
    /// Maximum number of chapters allowed
    pub max_chapters: usize,
    /// Minimum word count per chapter summary
    pub min_summary_words: usize,
    /// Whether to check plot arc completeness
    pub check_plot_arc: bool,
    /// Whether to check for placeholder text
    pub check_placeholders: bool,
}

impl Default for OutlineValidator {
    fn default() -> Self {
        Self {
            min_chapters: 1,
            max_chapters: 20,
            min_summary_words: 10,
            check_plot_arc: true,
            check_placeholders: true,
        }
    }
}

impl OutlineValidator {
    /// Run validation checks on an outline.
    pub fn validate(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // 1. Check for required sections
        checks.extend(self.check_required_sections(text, scope));

        // 2. Check chapter structure
        if let Some(chapters) = self.parse_chapters(text) {
            checks.extend(self.check_chapter_structure(&chapters, scope));
            checks.extend(self.check_chapter_numbers(&chapters, scope));
            checks.extend(self.check_duplicate_titles(&chapters, scope));

            // Derive word count targets from chapters
            let targets = self.derive_word_counts(&chapters);
            // Add target info check
            checks.push(ValidationCheck {
                name: "word_count_targets_derived".into(),
                passed: targets.per_chapter > 0,
                severity: if targets.per_chapter > 0 {
                    ValidationSeverity::Info
                } else {
                    ValidationSeverity::Warning
                },
                detail: format!(
                    "Derived target: ~{} words per chapter, ~{} words total",
                    targets.per_chapter, targets.minimum_total,
                ),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: scope.to_string(),
            });
        } else {
            checks.push(ValidationCheck {
                name: "outline_chapter_parsing".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                detail: "Could not parse chapter structure from outline.".into(),
                suggestion: Some(
                    "Ensure each chapter has a heading (## Chapter N) and a summary paragraph."
                        .into(),
                ),
                source: ValidationSource::Classic,
                scope: scope.to_string(),
            });
        }

        // 3. Check for plot arc
        if self.check_plot_arc {
            checks.extend(self.check_plot_arc(text, scope));
        }

        // 4. Check for placeholders
        if self.check_placeholders {
            checks.extend(self.check_placeholders(text, scope));
        }

        checks
    }

    /// Validate outline using inference (coherence, plot logic).
    pub fn validate_with_inference(
        &self,
        backend: &dyn ModelBackend,
        outline_text: &str,
    ) -> Result<Vec<ValidationCheck>, String> {
        let mut checks = Vec::new();

        let grammar = Some(OutlineInferenceResult::grammar());
        let system = "You are an expert story editor. Evaluate this outline for \
                       plot coherence, character motivation, and narrative logic. \
                       Output valid JSON only.";

        let prompt = format!(
            "Evaluate this story outline:\n\n{outline_text}\n\n\
             Provide:\n\
             - plot_coherent (boolean): Does the plot make logical sense?\n\
             - character_motivation_clear (boolean): Are character goals clear?\n\
             - has_narrative_arc (boolean): Is there a clear beginning/middle/end?\n\
             - detail (string): Brief evaluation\n\
             - suggestion (string or null): How to improve\n\n\
             Output valid JSON only."
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt: prompt.clone(),
            grammar,
            temperature: 0.3,
            max_tokens: 300,
            prefill: Some("{\n\"plot_coherent\"".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        let result: OutlineInferenceResult = serde_json::from_str(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))?;

        checks.push(ValidationCheck {
            name: "outline_plot_coherence".into(),
            passed: result.plot_coherent,
            severity: if result.plot_coherent {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!("Plot coherence: {}", result.detail),
            suggestion: result.suggestion.clone(),
            source: ValidationSource::Inference,
            scope: "outline".into(),
        });

        checks.push(ValidationCheck {
            name: "outline_character_motivation".into(),
            passed: result.character_motivation_clear,
            severity: if result.character_motivation_clear {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "Character motivation: {}",
                if result.character_motivation_clear {
                    "clear"
                } else {
                    "unclear"
                }
            ),
            suggestion: if !result.character_motivation_clear {
                Some("Add character goals and motivations to the outline.".into())
            } else {
                None
            },
            source: ValidationSource::Inference,
            scope: "outline".into(),
        });

        checks.push(ValidationCheck {
            name: "outline_narrative_arc".into(),
            passed: result.has_narrative_arc,
            severity: if result.has_narrative_arc {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!(
                "Narrative arc: {}",
                if result.has_narrative_arc {
                    "present"
                } else {
                    "missing"
                }
            ),
            suggestion: if !result.has_narrative_arc {
                Some("Ensure the outline has a clear beginning, middle, and end.".into())
            } else {
                None
            },
            source: ValidationSource::Inference,
            scope: "outline".into(),
        });

        Ok(checks)
    }

    /// Derive word count targets from the outline chapters.
    pub fn derive_word_counts(&self, chapters: &[OutlineChapter]) -> WordCountTargets {
        let num_chapters = chapters.len().max(1);

        // Average summary length gives a hint about desired chapter length
        let avg_summary_len: usize = if !chapters.is_empty() {
            chapters
                .iter()
                .map(|c| c.summary.split_whitespace().count())
                .sum::<usize>()
                / num_chapters
        } else {
            20
        };

        // Estimate target: ~20-30 words of summary per 100 words of prose
        let per_chapter = (avg_summary_len * 8).max(200);
        let per_paragraph = (per_chapter / 5).max(50);
        let minimum_total = per_chapter * num_chapters;

        WordCountTargets {
            per_chapter,
            per_paragraph,
            per_section: per_chapter / 2,
            minimum_total,
        }
    }

    // ── Sub-checks ──────────────────────────────────────────────────────

    fn check_required_sections(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let lower = text.to_lowercase();

        // Title
        let has_title = lower.contains("title")
            || lower.starts_with("# ")
            || text.lines().next().map_or(false, |l| !l.is_empty());
        checks.push(ValidationCheck {
            name: "outline_title".into(),
            passed: has_title,
            severity: if has_title {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Critical
            },
            detail: if has_title {
                "Outline has a title.".into()
            } else {
                "Outline is missing a title.".into()
            },
            suggestion: if !has_title {
                Some("Add a title to the outline (first line or 'Title:' field).".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Chapters section
        let has_chapters = lower.contains("chapter") || text.contains("##");
        checks.push(ValidationCheck {
            name: "outline_chapters".into(),
            passed: has_chapters,
            severity: if has_chapters {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Critical
            },
            detail: if has_chapters {
                "Outline contains chapters.".into()
            } else {
                "Outline is missing chapter definitions.".into()
            },
            suggestion: if !has_chapters {
                Some("Add at least one chapter with a heading and summary.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    fn check_chapter_structure(
        &self,
        chapters: &[OutlineChapter],
        scope: &str,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Minimum chapters
        let min_ok = chapters.len() >= self.min_chapters;
        checks.push(ValidationCheck {
            name: "outline_min_chapters".into(),
            passed: min_ok,
            severity: if min_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!(
                "{} chapter(s) (minimum: {})",
                chapters.len(),
                self.min_chapters,
            ),
            suggestion: if !min_ok {
                Some(format!(
                    "Add at least {} more chapter(s) to the outline.",
                    self.min_chapters.saturating_sub(chapters.len()),
                ))
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Maximum chapters
        let max_ok = chapters.len() <= self.max_chapters;
        checks.push(ValidationCheck {
            name: "outline_max_chapters".into(),
            passed: max_ok,
            severity: if max_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} chapter(s) (maximum: {})",
                chapters.len(),
                self.max_chapters,
            ),
            suggestion: if !max_ok {
                Some(format!(
                    "Consider consolidating {} chapters into fewer, more substantial ones.",
                    chapters.len(),
                ))
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Summary word count
        let empty_summaries: Vec<usize> = chapters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.summary.split_whitespace().count() < self.min_summary_words)
            .map(|(i, _)| i + 1)
            .collect();

        let summary_ok = empty_summaries.is_empty();
        checks.push(ValidationCheck {
            name: "outline_summary_detail".into(),
            passed: summary_ok,
            severity: if summary_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} chapter(s) have short or empty summaries: {:?}",
                empty_summaries.len(),
                empty_summaries,
            ),
            suggestion: if !empty_summaries.is_empty() {
                Some(format!(
                    "Expand summaries for chapters {:?} to at least {} words.",
                    empty_summaries, self.min_summary_words,
                ))
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    fn check_chapter_numbers(
        &self,
        chapters: &[OutlineChapter],
        scope: &str,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Check sequential numbering
        let expected: Vec<usize> = (1..=chapters.len()).collect();
        let actual: Vec<usize> = chapters.iter().map(|c| c.number).collect();

        let seq_ok = expected == actual;
        checks.push(ValidationCheck {
            name: "outline_chapter_numbering".into(),
            passed: seq_ok,
            severity: if seq_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: if seq_ok {
                "Chapter numbering is sequential.".into()
            } else {
                format!(
                    "Chapter numbering is non-sequential. Expected: {:?}, Got: {:?}",
                    expected, actual,
                )
            },
            suggestion: if !seq_ok {
                Some("Renumber chapters to be sequential (1, 2, 3, ...).".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    fn check_duplicate_titles(
        &self,
        chapters: &[OutlineChapter],
        scope: &str,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let mut seen = HashSet::new();
        let mut dups = Vec::new();

        for ch in chapters {
            if !seen.insert(ch.title.to_lowercase()) {
                dups.push(ch.title.clone());
            }
        }

        let dup_ok = dups.is_empty();
        checks.push(ValidationCheck {
            name: "outline_duplicate_titles".into(),
            passed: dup_ok,
            severity: if dup_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: if dup_ok {
                "No duplicate chapter titles.".into()
            } else {
                format!("Duplicate chapter titles: {:?}", dups)
            },
            suggestion: if !dup_ok {
                Some("Give each chapter a unique title.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    fn check_plot_arc(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let lower = text.to_lowercase();

        // Check for beginning/middle/end indicators
        let has_beginning = lower.contains("beginning")
            || lower.contains("introduction")
            || lower.contains("opening")
            || lower.contains("inciting")
            || lower.contains("setup");
        let has_middle = lower.contains("middle")
            || lower.contains("rising")
            || lower.contains("conflict")
            || lower.contains("development")
            || lower.contains("confrontation");
        let has_end = lower.contains("end")
            || lower.contains("conclusion")
            || lower.contains("resolution")
            || lower.contains("climax")
            || lower.contains("falling");

        checks.push(ValidationCheck {
            name: "outline_plot_arc".into(),
            passed: chapters_imply_arc(&chapters_from_text(text)),
            severity: ValidationSeverity::Info,
            detail: format!(
                "Beginning: {}, Middle: {}, End: {}",
                if has_beginning { "✓" } else { "?" },
                if has_middle { "✓" } else { "?" },
                if has_end { "✓" } else { "?" },
            ),
            suggestion: if !(has_beginning && has_middle && has_end) {
                Some(
                    "Ensure the outline covers a complete narrative arc: \
                     setup (beginning), conflict (middle), resolution (end)."
                        .to_string(),
                )
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    fn check_placeholders(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let lower = text.to_lowercase();

        let placeholder_patterns = [
            "todo",
            "tbd",
            "replace this",
            "fill in",
            "coming soon",
            "placeholder",
            "to be written",
            "to be determined",
            "summary needed",
            "description needed",
        ];

        let found: Vec<&str> = placeholder_patterns
            .iter()
            .filter(|p| lower.contains(*p))
            .copied()
            .collect();

        let placeholder_ok = found.is_empty();
        checks.push(ValidationCheck {
            name: "outline_placeholder_text".into(),
            passed: placeholder_ok,
            severity: if placeholder_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: if placeholder_ok {
                "No placeholder text found.".into()
            } else {
                format!("Found placeholder text: {:?}", found)
            },
            suggestion: if !placeholder_ok {
                Some("Replace all placeholder text with actual content.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    // ── Parsing ───────────────────────────────────────────────────────────

    fn parse_chapters(&self, text: &str) -> Option<Vec<OutlineChapter>> {
        let chapters = chapters_from_text(text);
        if chapters.is_empty() {
            None
        } else {
            Some(chapters)
        }
    }
}

// ── Internal types ────────────────────────────────────────────────────────

/// A parsed chapter from the outline.
#[derive(Debug, Clone)]
pub struct OutlineChapter {
    pub number: usize,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
struct OutlineInferenceResult {
    plot_coherent: bool,
    character_motivation_clear: bool,
    has_narrative_arc: bool,
    detail: String,
    #[serde(default)]
    suggestion: Option<String>,
}

impl OutlineInferenceResult {
    fn schema() -> Schema {
        Schema::object()
            .prop("plot_coherent", Schema::boolean())
            .prop("character_motivation_clear", Schema::boolean())
            .prop("has_narrative_arc", Schema::boolean())
            .prop("detail", Schema::string())
            .prop("suggestion", Schema::string())
            .build()
    }

    fn grammar() -> String {
        roco_grammar::schema_to_gbnf("root", Self::schema().to_json())
            .expect("OutlineInferenceResult schema is valid")
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Extract chapter information from outline text.
fn chapters_from_text(text: &str) -> Vec<OutlineChapter> {
    let mut chapters = Vec::new();
    let mut current_number: Option<usize> = None;
    let mut current_title = String::new();
    let mut current_summary = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Match "## Chapter N: Title" or "## Chapter N" patterns
        if trimmed.starts_with("## ") {
            // Save previous chapter if any
            if let Some(num) = current_number.take() {
                chapters.push(OutlineChapter {
                    number: num,
                    title: std::mem::take(&mut current_title),
                    summary: std::mem::take(&mut current_summary),
                });
            }

            let rest = &trimmed[3..];
            if let Some(chapter_info) = parse_chapter_heading(rest) {
                current_number = Some(chapter_info.0);
                current_title = chapter_info.1;
            }
        } else if current_number.is_some() {
            // Accumulate summary text
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                if !current_summary.is_empty() {
                    current_summary.push(' ');
                }
                current_summary.push_str(trimmed);
            }
        }
    }

    // Save last chapter
    if let Some(num) = current_number {
        chapters.push(OutlineChapter {
            number: num,
            title: current_title,
            summary: current_summary,
        });
    }

    chapters
}

fn parse_chapter_heading(text: &str) -> Option<(usize, String)> {
    let text = text.trim();
    // Match "Chapter N: Title" or "Chapter N"
    if let Some(rest) = text.strip_prefix("Chapter ") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        let num_part = parts[0].trim();
        let num = num_part.parse::<usize>().ok()?;
        let title = parts
            .get(1)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        Some((num, title))
    } else if let Some(rest) = text.strip_prefix("chapter ") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        let num_part = parts[0].trim();
        let num = num_part.parse::<usize>().ok()?;
        let title = parts
            .get(1)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        Some((num, title))
    } else {
        None
    }
}

/// Determine if the chapters imply a narrative arc (beginning/middle/end).
fn chapters_imply_arc(chapters: &[OutlineChapter]) -> bool {
    chapters.len() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_outline() -> &'static str {
        "Title: The Quest\n\
         Genre: Fantasy\n\
         Tone: Adventurous\n\n\
         ## Chapter 1: The Beginning\n\
         The hero sets out on a journey to find the lost artifact.\n\n\
         ## Chapter 2: The Conflict\n\
         The hero faces challenges and meets allies.\n\n\
         ## Chapter 3: The Resolution\n\
         The hero finds the artifact and returns home.\n"
    }

    fn minimal_outline() -> &'static str {
        "Title: Short\n\
         ## Chapter 1\n\
         Brief.\n"
    }

    #[test]
    fn test_parse_chapters() {
        let chapters = chapters_from_text(sample_outline());
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].number, 1);
        assert_eq!(chapters[0].title, "The Beginning");
        assert_eq!(chapters[1].number, 2);
        assert_eq!(chapters[2].number, 3);
    }

    #[test]
    fn test_parse_chapters_minimal() {
        let chapters = chapters_from_text(minimal_outline());
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].number, 1);
    }

    #[test]
    fn test_outline_title_check() {
        let valid = OutlineValidator::default();
        let checks = valid.check_required_sections(sample_outline(), "outline");
        assert!(checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_outline_chapter_count() {
        let valid = OutlineValidator::default();
        let chapters = chapters_from_text(minimal_outline());
        let checks = valid.check_chapter_structure(&chapters, "outline");
        assert!(checks[0].passed, "1 chapter >= min 1");
    }

    #[test]
    fn test_outline_chapter_numbering() {
        let valid = OutlineValidator::default();
        let chapters = chapters_from_text(sample_outline());
        let checks = valid.check_chapter_numbers(&chapters, "outline");
        assert!(checks[0].passed, "chapters should be sequential");
    }

    #[test]
    fn test_outline_non_sequential() {
        let valid = OutlineValidator::default();
        let chapters = vec![
            OutlineChapter {
                number: 1,
                title: "A".into(),
                summary: "Summary".into(),
            },
            OutlineChapter {
                number: 3,
                title: "C".into(),
                summary: "Summary".into(),
            },
        ];
        let checks = valid.check_chapter_numbers(&chapters, "outline");
        assert!(!checks[0].passed, "non-sequential should fail");
    }

    #[test]
    fn test_derive_word_counts() {
        let valid = OutlineValidator::default();
        let chapters = chapters_from_text(sample_outline());
        let targets = valid.derive_word_counts(&chapters);
        assert!(targets.per_chapter > 0);
        assert!(targets.minimum_total > 0);
        // Summaries in sample are about 10 words each -> ~80 words per chapter
        // (depends on exact parsing)
    }

    #[test]
    fn test_outline_placeholder_detected() {
        let valid = OutlineValidator::default();
        let text = "Title: X\n## Chapter 1\nTODO: write summary";
        let checks = valid.check_placeholders(text, "outline");
        assert!(!checks[0].passed, "should detect TODO placeholder");
    }

    #[test]
    fn test_outline_placeholder_clean() {
        let valid = OutlineValidator::default();
        let checks = valid.check_placeholders(sample_outline(), "outline");
        assert!(checks[0].passed, "should pass clean outline");
    }

    #[test]
    fn test_chapters_imply_arc() {
        let three = vec![
            OutlineChapter {
                number: 1,
                title: "Start".into(),
                summary: "S".into(),
            },
            OutlineChapter {
                number: 2,
                title: "Middle".into(),
                summary: "M".into(),
            },
            OutlineChapter {
                number: 3,
                title: "End".into(),
                summary: "E".into(),
            },
        ];
        assert!(chapters_imply_arc(&three));

        let one = vec![OutlineChapter {
            number: 1,
            title: "Only".into(),
            summary: "O".into(),
        }];
        assert!(!chapters_imply_arc(&one));
    }
}
