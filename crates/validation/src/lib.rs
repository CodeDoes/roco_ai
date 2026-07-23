//! # RoCo Validation — multi-layer story validation framework
//!
//! Validates chapters, outlines, and world-building through classic
//! programmatic checks and inference-backed critique.
//!
//! ════════════════════════════════════════════════════════════════════════════
//! FILE STATUS: EDITABLE — freely editable crate
//! SIZE: ~200 lines (core types + orchestrator)
//! KEY SECTIONS:
//!   1. ValidationReport, ValidationSeverity, ValidationCheck (lines 20-90)
//!   2. ValidationEngine — orchestrator (lines 90-200)
//!   3. Sub-modules: classic, inference, outline, wiki
//! ════════════════════════════════════════════════════════════════════════════
//!
//! # Architecture
//!
//! ```text
//! ValidationEngine
//!   ├── classic::ChapterValidator   (programmatic: word count, spacing, repetition, typos)
//!   ├── inference::Critic           (model-as-judge: coherence, engagement, quality)
//!   ├── outline::OutlineValidator   (outline structure, completeness, plot arc)
//!   └── wiki::WikiValidator         (world-building: interlinks, tags, consistency)
//! ```
//!
//! Every validator returns a `Vec<ValidationCheck>` which the engine collects
//! into a `ValidationReport`. Natural language summaries are generated from
//! the report.

pub mod agent;
pub mod brainstorm;
pub mod classic;
pub mod condensed;
pub mod inference;
pub mod intent;
pub mod outline;
pub mod planner;
pub mod session;
pub mod summarizer;
pub mod tool_set;
pub mod wiki;

use std::collections::HashMap;

use roco_engine::ModelBackend;

// ═══════════════════════════════════════════════════════════════════════════
// Core types
// ═══════════════════════════════════════════════════════════════════════════

/// Severity of a validation finding.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Informational — not a problem, just an observation
    Info,
    /// Minor issue — should be fixed but not blocking
    Warning,
    /// Significant issue — should be fixed before publication
    Error,
    /// Critical — blocks publication, must be fixed
    Critical,
}

impl std::fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Source of a validation check — was it programmatic or inference-backed?
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSource {
    /// Programmatic / rule-based check
    Classic,
    /// Model-as-judge / inference-backed check
    Inference,
}

/// A single validation check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationCheck {
    /// Human-readable name, e.g. "word_count_per_chapter"
    pub name: String,
    /// Whether this check passed
    pub passed: bool,
    /// Severity if failed
    pub severity: ValidationSeverity,
    /// Human-readable detail
    pub detail: String,
    /// Suggestion for fixing (if failed)
    pub suggestion: Option<String>,
    /// Source of the check
    pub source: ValidationSource,
    /// Which scope this applies to: "chapter:1", "outline", "wiki", etc.
    pub scope: String,
}

/// Complete validation report for a story element.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ValidationReport {
    /// All checks across all validators
    pub checks: Vec<ValidationCheck>,
    /// Overall pass/fail — true only if no errors or criticals
    pub passed: bool,
    /// Summary of the report in natural language
    pub summary: String,
    /// Target word counts (derived from outline or natural language)
    pub targets: WordCountTargets,
}

impl ValidationReport {
    /// Create a report from a set of checks.
    pub fn from_checks(checks: Vec<ValidationCheck>) -> Self {
        let has_error_or_critical = checks.iter().any(|c| {
            !c.passed
                && matches!(
                    c.severity,
                    ValidationSeverity::Error | ValidationSeverity::Critical
                )
        });
        Self {
            summary: String::new(),
            passed: !has_error_or_critical,
            checks,
            targets: WordCountTargets::default(),
        }
    }

    /// Generate a natural language summary of this report.
    pub fn generate_summary(&mut self) -> String {
        let total = self.checks.len();
        let passed = self.checks.iter().filter(|c| c.passed).count();
        let _failed = total - passed;

        let error_count = self
            .checks
            .iter()
            .filter(|c| !c.passed && c.severity >= ValidationSeverity::Error)
            .count();

        let warning_count = self
            .checks
            .iter()
            .filter(|c| !c.passed && c.severity == ValidationSeverity::Warning)
            .count();

        let summary = if self.passed {
            format!("✅ All {total} checks passed. The content is ready for the next stage.")
        } else if error_count == 0 {
            format!(
                "⚠️ {warning_count} warnings found. {passed}/{total} checks passed. Review suggestions before proceeding."
            )
        } else {
            format!(
                "❌ {error_count} errors, {warning_count} warnings. {passed}/{total} checks passed. \
                 Fix issues before proceeding."
            )
        };

        self.summary = summary.clone();
        summary
    }

    /// Get all failing checks grouped by scope.
    pub fn failures_by_scope(&self) -> HashMap<String, Vec<&ValidationCheck>> {
        let mut map: HashMap<String, Vec<&ValidationCheck>> = HashMap::new();
        for check in &self.checks {
            if !check.passed {
                map.entry(check.scope.clone()).or_default().push(check);
            }
        }
        map
    }

    /// Get checks for a specific scope (e.g., "chapter:1", "outline", "wiki").
    pub fn for_scope(&self, scope: &str) -> Vec<&ValidationCheck> {
        self.checks.iter().filter(|c| c.scope == scope).collect()
    }
}

/// Target word counts derived from outline or natural language instructions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WordCountTargets {
    /// Target word count per chapter
    pub per_chapter: usize,
    /// Target word count per paragraph
    pub per_paragraph: usize,
    /// Target word count per chapter section (if chapter has sections)
    pub per_section: usize,
    /// Minimum total word count for the entire story
    pub minimum_total: usize,
}

impl Default for WordCountTargets {
    fn default() -> Self {
        Self {
            per_chapter: 500,
            per_paragraph: 100,
            per_section: 200,
            minimum_total: 1500,
        }
    }
}

impl WordCountTargets {
    /// Parse word count targets from natural language.
    ///
    /// Accepts strings like:
    /// - "2000 words per chapter"
    /// - "at least 3000 words per chapter, 150 per paragraph"
    /// - "minimum total 10000 words"
    pub fn from_natural_language(text: &str) -> Self {
        let mut targets = WordCountTargets::default();
        let lower = text.to_lowercase();

        // Parse "X words per chapter"
        if let Some(cap) = parse_number_before(&lower, "words per chapter") {
            targets.per_chapter = cap;
        }
        // Parse "X words per paragraph"
        if let Some(cap) = parse_number_before(&lower, "words per paragraph") {
            targets.per_paragraph = cap;
        }
        // Parse "X words per section"
        if let Some(cap) = parse_number_before(&lower, "words per section") {
            targets.per_section = cap;
        }
        // Parse "minimum total X words" or "total X words"
        if let Some(cap) = parse_number_after(&lower, "minimum total") {
            targets.minimum_total = cap;
        } else if let Some(cap) = parse_number_after(&lower, "total of") {
            targets.minimum_total = cap;
        } else if let Some(cap) = parse_number_before(&lower, "words total") {
            targets.minimum_total = cap;
        }

        // "at least X words" without qualifier -> both per_chapter and minimum_total
        if let Some(cap) = parse_number_after(&lower, "at least") {
            if targets.per_chapter == 500 && targets.minimum_total == 1500 {
                targets.per_chapter = cap;
                targets.minimum_total = cap * 3; // assume ~3 chapters
            } else if targets.per_chapter == 500 {
                targets.per_chapter = cap;
            }
        }

        targets
    }
}

/// Parse the first number before a keyword.
fn parse_number_before(text: &str, keyword: &str) -> Option<usize> {
    if let Some(pos) = text.find(keyword) {
        let before = &text[..pos];
        // Find numbers in the reversed slice
        for token in before.split_whitespace().rev() {
            if let Ok(n) = token
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<usize>()
            {
                return Some(n.max(1));
            }
        }
    }
    None
}

/// Parse the first number after a keyword.
fn parse_number_after(text: &str, keyword: &str) -> Option<usize> {
    if let Some(pos) = text.find(keyword) {
        let after = &text[pos + keyword.len()..];
        for token in after.split_whitespace() {
            if let Ok(n) = token
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<usize>()
            {
                return Some(n.max(1));
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Validation Engine — orchestrator
// ═══════════════════════════════════════════════════════════════════════════

/// Orchestrates all validators and produces a unified report.
pub struct ValidationEngine {
    /// Classic (programmatic) validator
    pub classic: classic::ChapterValidator,
    /// Inference-backed critic
    pub inference: inference::Critic,
    /// Outline validator
    pub outline: outline::OutlineValidator,
    /// Wiki/world-building validator
    pub wiki: wiki::WikiValidator,
}

impl Default for ValidationEngine {
    fn default() -> Self {
        Self {
            classic: classic::ChapterValidator::default(),
            inference: inference::Critic::default(),
            outline: outline::OutlineValidator::default(),
            wiki: wiki::WikiValidator::default(),
        }
    }
}

impl ValidationEngine {
    /// Create a new validation engine with custom config.
    pub fn new(
        classic: classic::ChapterValidator,
        inference: inference::Critic,
        outline: outline::OutlineValidator,
        wiki: wiki::WikiValidator,
    ) -> Self {
        Self {
            classic,
            inference,
            outline,
            wiki,
        }
    }

    /// Run all validation checks for a single chapter.
    pub fn validate_chapter(
        &self,
        backend: Option<&dyn ModelBackend>,
        chapter_text: &str,
        chapter_num: usize,
        outline_context: &str,
        targets: &WordCountTargets,
    ) -> ValidationReport {
        let scope = format!("chapter:{chapter_num}");
        let mut all_checks = Vec::new();

        // Classic checks (always run)
        let classic_checks = self.classic.validate(chapter_text, &scope, targets);
        all_checks.extend(classic_checks);

        // Inference checks (only if backend is available)
        if let Some(be) = backend {
            match self
                .inference
                .critique_chapter(be, chapter_text, chapter_num, outline_context)
            {
                Ok(inference_checks) => all_checks.extend(inference_checks),
                Err(e) => {
                    all_checks.push(ValidationCheck {
                        name: "inference_critique".into(),
                        passed: false,
                        severity: ValidationSeverity::Warning,
                        detail: format!("Inference critique failed: {e}"),
                        suggestion: Some(
                            "Retry with a different backend or skip inference checks.".into(),
                        ),
                        source: ValidationSource::Inference,
                        scope: scope.clone(),
                    });
                }
            }
        }

        let mut report = ValidationReport::from_checks(all_checks);
        report.targets = targets.clone();
        report.generate_summary();
        report
    }

    /// Run all validation checks for an outline.
    pub fn validate_outline(
        &self,
        backend: Option<&dyn ModelBackend>,
        outline_text: &str,
    ) -> ValidationReport {
        let scope = "outline".to_string();
        let mut all_checks = Vec::new();

        let outline_checks = self.outline.validate(outline_text, &scope);
        all_checks.extend(outline_checks);

        if let Some(be) = backend {
            match self.outline.validate_with_inference(be, outline_text) {
                Ok(checks) => all_checks.extend(checks),
                Err(e) => {
                    all_checks.push(ValidationCheck {
                        name: "outline_inference".into(),
                        passed: false,
                        severity: ValidationSeverity::Warning,
                        detail: format!("Outline inference check failed: {e}"),
                        suggestion: None,
                        source: ValidationSource::Inference,
                        scope: scope.clone(),
                    });
                }
            }
        }

        let mut report = ValidationReport::from_checks(all_checks);
        report.generate_summary();
        report
    }

    /// Run all validation checks for wiki/world-building.
    pub fn validate_wiki(
        &self,
        backend: Option<&dyn ModelBackend>,
        wiki_text: &str,
        chapters: &[String],
    ) -> ValidationReport {
        let scope = "wiki".to_string();
        let mut all_checks = Vec::new();

        let wiki_checks = self.wiki.validate(wiki_text, &scope);
        all_checks.extend(wiki_checks);

        // Cross-chapter consistency checks
        let consistency_checks = self
            .wiki
            .check_cross_chapter_consistency(wiki_text, chapters, &scope);
        all_checks.extend(consistency_checks);

        if let Some(be) = backend {
            match self.wiki.validate_with_inference(be, wiki_text, chapters) {
                Ok(checks) => all_checks.extend(checks),
                Err(e) => {
                    all_checks.push(ValidationCheck {
                        name: "wiki_inference".into(),
                        passed: false,
                        severity: ValidationSeverity::Warning,
                        detail: format!("Wiki inference check failed: {e}"),
                        suggestion: None,
                        source: ValidationSource::Inference,
                        scope: scope.clone(),
                    });
                }
            }
        }

        let mut report = ValidationReport::from_checks(all_checks);
        report.generate_summary();
        report
    }

    /// Natural language access point: parse instruction and run validation.
    ///
    /// Accepts natural language like:
    /// - "Validate chapter 1 for word count and spacing"
    /// - "Check the outline for completeness"
    /// - "Run all validations on chapter 2"
    /// - "Is the wiki consistent with chapters 1-3?"
    pub fn validate_from_natural_language(
        &self,
        instruction: &str,
        backend: Option<&dyn ModelBackend>,
        chapters: &[String],
        outline_text: &str,
        wiki_text: &str,
    ) -> ValidationReport {
        let lower = instruction.to_lowercase();

        // Parse targets from the instruction
        let targets = WordCountTargets::from_natural_language(instruction);

        // Parse chapter number from instruction
        let chapter_nums: Vec<usize> = {
            let mut nums = Vec::new();
            for word in lower.split_whitespace() {
                if let Ok(n) = word
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<usize>()
                {
                    if n >= 1 && n <= chapters.len() {
                        nums.push(n);
                    }
                }
            }
            nums
        };

        // Determine what to validate
        let validate_outline = lower.contains("outline");
        let validate_wiki =
            lower.contains("wiki") || lower.contains("world") || lower.contains("worldbuilding");
        let validate_chapters = lower.contains("chapter") || lower.contains("chapters");

        let mut all_checks = Vec::new();

        if validate_outline && !outline_text.is_empty() {
            let outline_report = self.validate_outline(backend, outline_text);
            all_checks.extend(outline_report.checks);
        }

        if validate_wiki && !wiki_text.is_empty() {
            let wiki_report = self.validate_wiki(backend, wiki_text, chapters);
            all_checks.extend(wiki_report.checks);
        }

        if validate_chapters {
            let nums: Vec<usize> = if chapter_nums.is_empty() {
                (0..chapters.len()).map(|i| i + 1).collect()
            } else {
                chapter_nums
            };
            for num in nums {
                if num <= chapters.len() {
                    let chapter_text = &chapters[num - 1];
                    let chapter_report =
                        self.validate_chapter(backend, chapter_text, num, outline_text, &targets);
                    all_checks.extend(chapter_report.checks);
                }
            }
        }

        // If nothing specific was mentioned, run all validations
        if all_checks.is_empty() {
            // Run outline validation
            if !outline_text.is_empty() {
                let outline_report = self.validate_outline(backend, outline_text);
                all_checks.extend(outline_report.checks);
            }

            // Run wiki validation
            if !wiki_text.is_empty() {
                let wiki_report = self.validate_wiki(backend, wiki_text, chapters);
                all_checks.extend(wiki_report.checks);
            }

            // Run chapter validation for all chapters
            for (i, chapter_text) in chapters.iter().enumerate() {
                let chapter_report =
                    self.validate_chapter(backend, chapter_text, i + 1, outline_text, &targets);
                all_checks.extend(chapter_report.checks);
            }
        }

        let mut report = ValidationReport::from_checks(all_checks);
        report.targets = targets;
        report.generate_summary();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count_targets_from_natural_language() {
        let targets = WordCountTargets::from_natural_language(
            "Write a story with 2000 words per chapter, at least 100 words per paragraph",
        );
        assert_eq!(targets.per_chapter, 2000);
        assert_eq!(targets.per_paragraph, 100);
    }

    #[test]
    fn test_word_count_targets_minimum_total() {
        let targets = WordCountTargets::from_natural_language(
            "At least 10000 words total, 3000 words per chapter",
        );
        assert_eq!(targets.minimum_total, 10000);
        assert_eq!(targets.per_chapter, 3000);
    }

    #[test]
    fn test_validation_report_passed() {
        let checks = vec![
            ValidationCheck {
                name: "word_count".into(),
                passed: true,
                severity: ValidationSeverity::Info,
                detail: "ok".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:1".into(),
            },
            ValidationCheck {
                name: "spacing".into(),
                passed: true,
                severity: ValidationSeverity::Info,
                detail: "ok".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:1".into(),
            },
        ];
        let report = ValidationReport::from_checks(checks);
        assert!(report.passed);
    }

    #[test]
    fn test_validation_report_failed() {
        let checks = vec![ValidationCheck {
            name: "word_count".into(),
            passed: false,
            severity: ValidationSeverity::Error,
            detail: "too short".into(),
            suggestion: Some("Add more content".into()),
            source: ValidationSource::Classic,
            scope: "chapter:1".into(),
        }];
        let report = ValidationReport::from_checks(checks);
        assert!(!report.passed);
    }

    #[test]
    fn test_validation_report_summary() {
        let checks = vec![
            ValidationCheck {
                name: "check_1".into(),
                passed: true,
                severity: ValidationSeverity::Info,
                detail: "ok".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:1".into(),
            },
            ValidationCheck {
                name: "check_2".into(),
                passed: false,
                severity: ValidationSeverity::Warning,
                detail: "minor".into(),
                suggestion: Some("fix it".into()),
                source: ValidationSource::Classic,
                scope: "chapter:1".into(),
            },
        ];
        let mut report = ValidationReport::from_checks(checks);
        let summary = report.generate_summary();
        // Warnings don't cause report to fail; info-level issues pass
        assert!(summary.contains("passed"));
        // The report should still show all 2 checks passed (warnings are informational)
        assert!(report.passed);
    }

    #[test]
    fn test_failures_by_scope() {
        let checks = vec![
            ValidationCheck {
                name: "wc".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                detail: "short".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:1".into(),
            },
            ValidationCheck {
                name: "spacing".into(),
                passed: true,
                severity: ValidationSeverity::Info,
                detail: "ok".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:2".into(),
            },
            ValidationCheck {
                name: "wc".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                detail: "short".into(),
                suggestion: None,
                source: ValidationSource::Classic,
                scope: "chapter:2".into(),
            },
        ];
        let report = ValidationReport::from_checks(checks);
        let by_scope = report.failures_by_scope();
        assert_eq!(by_scope.len(), 2);
        assert_eq!(by_scope.get("chapter:1").unwrap().len(), 1);
        assert_eq!(by_scope.get("chapter:2").unwrap().len(), 1);
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests_file;
