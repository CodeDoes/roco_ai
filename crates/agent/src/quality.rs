//! Story Quality Metrics — multi-dimensional quality scoring.
//!
//! Uses the model itself to evaluate story quality across multiple dimensions:
//! - Pacing (sentence length variation, paragraph breaks)
//! - Show-don't-tell (ratio of dialogue/action to exposition)
//! - Character voice consistency
//! - Tense consistency
//! - Plot coherence (contradictions with previous chapters)
//! - Engagement (reader interest, hooks, tension)
//!
//! # Approach
//!
//! Feed the model critique/approval examples and ask it to critique the story.
//! This leverages the model's understanding to evaluate quality.

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{schema_to_gbnf, Schema};
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Quality Dimensions
// ═════════════════════════════════════════════════════════════════════════════

/// Multi-dimensional quality score for a chapter or story.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityScore {
    /// Overall score 0-10
    pub overall: f32,
    /// Pacing score 0-10 (sentence variety, rhythm)
    pub pacing: f32,
    /// Show-don't-tell score 0-10 (dialogue/action vs exposition)
    pub show_dont_tell: f32,
    /// Character voice consistency 0-10
    pub character_voice: f32,
    /// Tense consistency 0-10
    pub tense_consistency: f32,
    /// Plot coherence 0-10 (no contradictions)
    pub plot_coherence: f32,
    /// Engagement 0-10 (hooks, tension, reader interest)
    pub engagement: f32,
    /// Prose quality 0-10 (word choice, imagery, flow)
    pub prose_quality: f32,
    /// List of specific issues found
    pub issues: Vec<QualityIssue>,
    /// List of strengths found
    pub strengths: Vec<String>,
    /// Suggested improvements
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    /// Category: pacing, show_dont_tell, voice, tense, coherence, engagement, prose
    pub category: String,
    /// Severity: low, medium, high
    pub severity: String,
    /// Description of the issue
    pub description: String,
    /// Location in text (if identifiable)
    pub location: Option<String>,
}

impl QualityScore {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("overall", Schema::number())
            .prop("pacing", Schema::number())
            .prop("show_dont_tell", Schema::number())
            .prop("character_voice", Schema::number())
            .prop("tense_consistency", Schema::number())
            .prop("plot_coherence", Schema::number())
            .prop("engagement", Schema::number())
            .prop("prose_quality", Schema::number())
            .prop(
                "issues",
                Schema::array(
                    Schema::object()
                        .prop("category", Schema::string())
                        .prop(
                            "severity",
                            Schema::enum_values(vec![
                                serde_json::json!("low"),
                                serde_json::json!("medium"),
                                serde_json::json!("high"),
                            ]),
                        )
                        .prop("description", Schema::string())
                        .prop("location", Schema::string())
                        .build(),
                ),
            )
            .prop("strengths", Schema::array(Schema::string()))
            .prop("suggestions", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("QualityScore schema is valid")
    }

    /// Check if the chapter passes quality thresholds
    pub fn passes(&self, threshold: f32) -> bool {
        self.overall >= threshold
            && self.pacing >= threshold * 0.8
            && self.engagement >= threshold * 0.8
            && self.plot_coherence >= threshold * 0.9
    }

    /// Get high-severity issues
    pub fn critical_issues(&self) -> Vec<&QualityIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == "high")
            .collect()
    }

    /// Merge multiple quality scores (e.g., from multiple evaluators)
    pub fn merge(scores: &[QualityScore]) -> QualityScore {
        if scores.is_empty() {
            return QualityScore::default();
        }

        let n = scores.len() as f32;
        let mut merged = QualityScore {
            overall: scores.iter().map(|s| s.overall).sum::<f32>() / n,
            pacing: scores.iter().map(|s| s.pacing).sum::<f32>() / n,
            show_dont_tell: scores.iter().map(|s| s.show_dont_tell).sum::<f32>() / n,
            character_voice: scores.iter().map(|s| s.character_voice).sum::<f32>() / n,
            tense_consistency: scores.iter().map(|s| s.tense_consistency).sum::<f32>() / n,
            plot_coherence: scores.iter().map(|s| s.plot_coherence).sum::<f32>() / n,
            engagement: scores.iter().map(|s| s.engagement).sum::<f32>() / n,
            prose_quality: scores.iter().map(|s| s.prose_quality).sum::<f32>() / n,
            issues: scores.iter().flat_map(|s| s.issues.clone()).collect(),
            strengths: scores.iter().flat_map(|s| s.strengths.clone()).collect(),
            suggestions: scores.iter().flat_map(|s| s.suggestions.clone()).collect(),
        };

        // Deduplicate
        merged
            .issues
            .sort_by(|a, b| a.description.cmp(&b.description));
        merged
            .issues
            .dedup_by(|a, b| a.description == b.description);
        merged.strengths.sort();
        merged.strengths.dedup();
        merged.suggestions.sort();
        merged.suggestions.dedup();

        merged
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Critique — model-based evaluation
// ═════════════════════════════════════════════════════════════════════════════

/// Critique response from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryCritique {
    /// Summary of the critique
    pub summary: String,
    /// Quality scores
    pub scores: QualityScore,
    /// Whether this chapter should be revised
    pub should_revise: bool,
    /// Priority revisions if should_revise is true
    pub priority_revisions: Vec<String>,
}

impl StoryCritique {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("summary", Schema::string())
            .prop("scores", QualityScore::schema())
            .prop("should_revise", Schema::boolean())
            .prop("priority_revisions", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("StoryCritique schema is valid")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Quality Analyzer
// ═════════════════════════════════════════════════════════════════════════════

/// Analyzes story quality using the model as judge.
pub struct QualityAnalyzer {
    /// Minimum score to pass (0-10)
    pub pass_threshold: f32,
    /// Whether to use strict mode (higher thresholds)
    pub strict_mode: bool,
}

impl Default for QualityAnalyzer {
    fn default() -> Self {
        Self {
            pass_threshold: 6.0,
            strict_mode: false,
        }
    }
}

impl QualityAnalyzer {
    /// Create a new quality analyzer
    pub fn new(pass_threshold: f32) -> Self {
        Self {
            pass_threshold,
            strict_mode: false,
        }
    }

    /// Enable strict mode
    pub fn with_strict_mode(mut self) -> Self {
        self.strict_mode = true;
        self.pass_threshold = 7.0;
        self
    }

    /// Evaluate a chapter using the model as judge.
    ///
    /// Feed the model examples of good and bad critique, then ask it to
    /// critique the chapter.
    pub fn evaluate_chapter(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        plot_context: &str,
    ) -> Result<StoryCritique, String> {
        let critique: StoryCritique = structured_complete(
            backend,
            &self.critique_system_prompt(),
            &format!(
                "Critique this chapter for quality.\n\n\
                 Chapter {chapter_num}:\n{chapter_text}\n\n\
                 Plot context:\n{plot_context}\n\n\
                 Evaluate across all dimensions and provide specific, actionable feedback.\n\
                 Output JSON matching the schema."
            ),
            &StoryCritique::grammar(),
            0.3,
            600,
        )?;

        Ok(critique)
    }

    /// Evaluate a complete story arc.
    pub fn evaluate_story(
        &self,
        backend: &dyn ModelBackend,
        chapters: &[String],
        plot_context: &str,
    ) -> Result<StoryCritique, String> {
        let story_text: String = chapters
            .iter()
            .enumerate()
            .map(|(i, ch)| format!("## Chapter {}\n{}", i + 1, ch))
            .collect::<Vec<_>>()
            .join("\n\n");

        let critique: StoryCritique = structured_complete(
            backend,
            &self.critique_system_prompt(),
            &format!(
                "Critique this complete story for quality.\n\n\
                 Story:\n{story_text}\n\n\
                 Plot context:\n{plot_context}\n\n\
                 Evaluate the story as a whole — arc completeness, consistency, \
                 character development, pacing across chapters.\n\
                 Output JSON matching the schema."
            ),
            &StoryCritique::grammar(),
            0.3,
            800,
        )?;

        Ok(critique)
    }

    /// Generate revision instructions based on critique
    pub fn generate_revision_instructions(&self, critique: &StoryCritique) -> String {
        let mut instructions = String::new();

        if critique.should_revise {
            instructions.push_str("Priority revisions:\n");
            for (i, rev) in critique.priority_revisions.iter().enumerate() {
                instructions.push_str(&format!("{}. {}\n", i + 1, rev));
            }
        }

        if !critique.scores.strengths.is_empty() {
            instructions.push_str("\nStrengths to preserve:\n");
            for s in &critique.scores.strengths {
                instructions.push_str(&format!("- {}\n", s));
            }
        }

        instructions
    }

    /// System prompt for critique generation
    fn critique_system_prompt(&self) -> String {
        let strictness = if self.strict_mode {
            "Be very strict. Only high-quality prose passes."
        } else {
            "Be fair but thorough. Identify both strengths and weaknesses."
        };

        format!(
            "You are an expert literary critic and editor. \
             {strictness}\n\n\
             Evaluate stories across these dimensions:\n\
             1. Pacing — sentence variety, rhythm, scene transitions\n\
             2. Show-don't-tell — dialogue/action vs exposition ratio\n\
             3. Character voice — distinct, consistent character speech\n\
             4. Tense consistency — no unintended tense shifts\n\
             5. Plot coherence — no contradictions, logical progression\n\
             6. Engagement — hooks, tension, reader interest\n\
             7. Prose quality — word choice, imagery, flow\n\n\
             Provide specific, actionable feedback. \
             Output valid JSON only."
        )
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper function
// ═════════════════════════════════════════════════════════════════════════════

fn structured_complete<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let text = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: Some(grammar.to_string()),
        temperature,
        max_tokens,
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?
    .text;

    serde_json::from_str::<T>(&text).map_err(|e| format!("parse error: {e}\nraw: {text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_score_passes() {
        let score = QualityScore {
            overall: 7.0,
            pacing: 6.0,
            engagement: 6.0,
            plot_coherence: 7.0,
            ..Default::default()
        };
        assert!(score.passes(6.0));
    }

    #[test]
    fn test_quality_score_fails_low_overall() {
        let score = QualityScore {
            overall: 5.0,
            pacing: 7.0,
            engagement: 7.0,
            plot_coherence: 7.0,
            ..Default::default()
        };
        assert!(!score.passes(6.0));
    }

    #[test]
    fn test_quality_score_fails_low_coherence() {
        let score = QualityScore {
            overall: 7.0,
            pacing: 7.0,
            engagement: 7.0,
            plot_coherence: 4.0,
            ..Default::default()
        };
        assert!(!score.passes(6.0));
    }

    #[test]
    fn test_critical_issues() {
        let score = QualityScore {
            issues: vec![
                QualityIssue {
                    category: "pacing".into(),
                    severity: "low".into(),
                    description: "Minor pacing issue".into(),
                    location: None,
                },
                QualityIssue {
                    category: "coherence".into(),
                    severity: "high".into(),
                    description: "Major plot hole".into(),
                    location: Some("Chapter 2".into()),
                },
            ],
            ..Default::default()
        };
        assert_eq!(score.critical_issues().len(), 1);
        assert_eq!(score.critical_issues()[0].description, "Major plot hole");
    }

    #[test]
    fn test_merge_scores() {
        let scores = vec![
            QualityScore {
                overall: 6.0,
                pacing: 7.0,
                ..Default::default()
            },
            QualityScore {
                overall: 8.0,
                pacing: 5.0,
                ..Default::default()
            },
        ];
        let merged = QualityScore::merge(&scores);
        assert_eq!(merged.overall, 7.0);
        assert_eq!(merged.pacing, 6.0);
    }
}
