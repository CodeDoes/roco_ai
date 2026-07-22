//! Inference-backed (model-as-judge) validation.
//!
//! Uses the model to evaluate:
//! - Chapter quality (coherence, engagement, prose)
//! - Performance against initial instructions
//! - Critique and revision suggestions
//! - Natural language feedback

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use serde::Deserialize;

use super::{ValidationCheck, ValidationSeverity, ValidationSource};

/// Inference-backed critic that uses the model as a judge.
#[derive(Debug, Clone)]
pub struct Critic {
    /// Temperature for critique generation (lower = more consistent)
    pub temperature: f32,
    /// Max tokens for critique output
    pub max_tokens: usize,
    /// Whether to use strict mode (higher thresholds)
    pub strict_mode: bool,
    /// Minimum overall score to pass (0-10)
    pub pass_threshold: f32,
    /// Whether to require grammar-constrained output
    pub use_grammar: bool,
}

impl Default for Critic {
    fn default() -> Self {
        Self {
            temperature: 0.3,
            max_tokens: 600,
            strict_mode: false,
            pass_threshold: 6.0,
            use_grammar: true,
        }
    }
}

impl Critic {
    /// Create a strict critic with higher thresholds.
    pub fn strict() -> Self {
        Self {
            temperature: 0.2,
            max_tokens: 800,
            strict_mode: true,
            pass_threshold: 7.5,
            use_grammar: true,
        }
    }

    /// Critique a chapter using the model as judge.
    pub fn critique_chapter(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        outline_context: &str,
    ) -> Result<Vec<ValidationCheck>, String> {
        let mut checks = Vec::new();

        // 1. Overall quality critique
        let critique = self.query_critique(backend, chapter_text, chapter_num, outline_context)?;

        // Check overall quality
        let quality_ok = critique.overall_score >= self.pass_threshold;
        checks.push(ValidationCheck {
            name: "inference_overall_quality".into(),
            passed: quality_ok,
            severity: if quality_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!(
                "Overall quality score: {:.1}/10 (threshold: {:.1})",
                critique.overall_score, self.pass_threshold,
            ),
            suggestion: if !quality_ok {
                Some(critique.suggestion.clone())
            } else {
                None
            },
            source: ValidationSource::Inference,
            scope: format!("chapter:{chapter_num}"),
        });

        // Check coherence
        let coherence_ok = critique.coherence_score >= self.pass_threshold * 0.8;
        checks.push(ValidationCheck {
            name: "inference_coherence".into(),
            passed: coherence_ok,
            severity: if coherence_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!(
                "Coherence score: {:.1}/10 (threshold: {:.1})",
                critique.coherence_score,
                self.pass_threshold * 0.8,
            ),
            suggestion: if !coherence_ok {
                Some("The chapter may have plot holes or logical inconsistencies. Review the narrative flow.".into())
            } else {
                None
            },
            source: ValidationSource::Inference,
            scope: format!("chapter:{chapter_num}"),
        });

        // Check engagement
        let engagement_ok = critique.engagement_score >= self.pass_threshold * 0.7;
        checks.push(ValidationCheck {
            name: "inference_engagement".into(),
            passed: engagement_ok,
            severity: if engagement_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "Engagement score: {:.1}/10 (threshold: {:.1})",
                critique.engagement_score,
                self.pass_threshold * 0.7,
            ),
            suggestion: if !engagement_ok {
                Some(critique.engagement_suggestion.clone())
            } else {
                None
            },
            source: ValidationSource::Inference,
            scope: format!("chapter:{chapter_num}"),
        });

        // 2. Performance against initial instructions (if outline context provided)
        if !outline_context.is_empty() {
            let perf = self.evaluate_against_instructions(
                backend,
                chapter_text,
                chapter_num,
                outline_context,
            )?;

            checks.push(ValidationCheck {
                name: "inference_instruction_following".into(),
                passed: perf.follows_instructions,
                severity: if perf.follows_instructions {
                    ValidationSeverity::Info
                } else {
                    ValidationSeverity::Error
                },
                detail: perf.detail.clone(),
                suggestion: perf.suggestion.clone(),
                source: ValidationSource::Inference,
                scope: format!("chapter:{chapter_num}"),
            });
        }

        Ok(checks)
    }

    /// Query the model for a structured critique.
    fn query_critique(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        _outline_context: &str,
    ) -> Result<CritiqueResult, String> {
        let grammar = if self.use_grammar {
            Some(CritiqueResult::grammar())
        } else {
            None
        };

        let strictness = if self.strict_mode {
            "Be very strict with your evaluation."
        } else {
            "Be fair but thorough."
        };

        let system = format!(
            "You are an expert literary critic and editor. {strictness}\n\
             Evaluate chapters on these dimensions:\n\
             1. Overall quality — prose, flow, readability (0-10)\n\
             2. Coherence — logical progression, no contradictions (0-10)\n\
             3. Engagement — hooks, tension, reader interest (0-10)\n\n\
             Output valid JSON only. Do NOT include thinking or reasoning in your output."
        );

        let prompt = format!(
            "Critique this chapter for quality.\n\n\
             Chapter {chapter_num}:\n{chapter_text}\n\n\
             Evaluate across all dimensions and provide:\n\
             - overall_score (0-10)\n\
             - coherence_score (0-10)\n\
             - engagement_score (0-10)\n\
             - brief_suggestion (string, what to improve)\n\
             - engagement_suggestion (string, how to improve engagement)\n\n\
             Output valid JSON matching the schema."
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.clone(),
            prompt: prompt.clone(),
            grammar,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n\"overall_score\"".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        // Parse with fallback
        parse_critique_result(&text)
            .map_err(|e| format!("failed to parse critique: {e}\nraw: {text}"))
    }

    /// Evaluate whether the chapter follows the original instructions/outline.
    fn evaluate_against_instructions(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        outline_context: &str,
    ) -> Result<InstructionPerformance, String> {
        let grammar = if self.use_grammar {
            Some(InstructionPerformance::grammar())
        } else {
            None
        };

        let system = "You are a quality assurance evaluator. Determine if a chapter \
                       follows its outline instructions. Output valid JSON only.";

        let prompt = format!(
            "Determine if this chapter follows the original instructions.\n\n\
             Outline/Instructions:\n{outline_context}\n\n\
             Chapter {chapter_num}:\n{chapter_text}\n\n\
             Does this chapter follow the instructions? Output:\n\
             - follows_instructions (boolean)\n\
             - detail (string, brief explanation)\n\
             - suggestion (string or null, how to fix)\n\n\
             Output valid JSON only."
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt: prompt.clone(),
            grammar,
            temperature: self.temperature.min(0.2),
            max_tokens: 200,
            prefill: Some("{\n\"follows_instructions\"".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        parse_instruction_performance(&text)
            .map_err(|e| format!("failed to parse instruction performance: {e}\nraw: {text}"))
    }

    /// Generate natural language feedback about a chapter.
    pub fn generate_feedback(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        instruction: &str,
    ) -> Result<String, String> {
        let system = "You are a helpful writing assistant. Provide constructive feedback \
                       on the chapter. Be specific and actionable.";

        let prompt = format!(
            "Review this chapter and provide natural language feedback.\n\n\
             Original instructions: {instruction}\n\n\
             Chapter {chapter_num}:\n{chapter_text}\n\n\
             Provide feedback covering:\n\
             1. What works well\n\
             2. What could be improved\n\
             3. Specific suggestions\n\n\
             Be concise — 2-3 paragraphs."
        );

        let response = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt: prompt.clone(),
            grammar: None,
            temperature: 0.5,
            max_tokens: self.max_tokens,
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?;

        Ok(response.text)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal types for model response parsing
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CritiqueResult {
    overall_score: f32,
    coherence_score: f32,
    engagement_score: f32,
    #[serde(default)]
    #[allow(dead_code)]
    brief_suggestion: String,
    #[serde(default)]
    #[allow(dead_code)]
    engagement_suggestion: String,
    #[serde(default)]
    suggestion: String,
}

impl CritiqueResult {
    fn schema() -> Schema {
        Schema::object()
            .prop("overall_score", Schema::number())
            .prop("coherence_score", Schema::number())
            .prop("engagement_score", Schema::number())
            .prop("brief_suggestion", Schema::string())
            .prop("engagement_suggestion", Schema::string())
            .prop("suggestion", Schema::string())
            .build()
    }

    fn grammar() -> String {
        roco_grammar::schema_to_gbnf("root", Self::schema().to_json())
            .expect("CritiqueResult schema is valid")
    }
}

#[derive(Debug, Deserialize)]
struct InstructionPerformance {
    follows_instructions: bool,
    detail: String,
    #[serde(default)]
    suggestion: Option<String>,
}

impl InstructionPerformance {
    fn schema() -> Schema {
        Schema::object()
            .prop("follows_instructions", Schema::boolean())
            .prop("detail", Schema::string())
            .prop("suggestion", Schema::string())
            .build()
    }

    fn grammar() -> String {
        roco_grammar::schema_to_gbnf("root", Self::schema().to_json())
            .expect("InstructionPerformance schema is valid")
    }
}

// ── Parsing helpers ──────────────────────────────────────────────────────

fn parse_critique_result(text: &str) -> Result<CritiqueResult, String> {
    let cleaned = roco_grammar::strategies::clean_json_output(text);
    serde_json::from_str::<CritiqueResult>(&cleaned).map_err(|e| format!("JSON parse error: {e}"))
}

fn parse_instruction_performance(text: &str) -> Result<InstructionPerformance, String> {
    let cleaned = roco_grammar::strategies::clean_json_output(text);
    serde_json::from_str::<InstructionPerformance>(&cleaned)
        .map_err(|e| format!("JSON parse error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critique_default_config() {
        let critic = Critic::default();
        assert_eq!(critic.temperature, 0.3);
        assert_eq!(critic.pass_threshold, 6.0);
        assert!(!critic.strict_mode);
    }

    #[test]
    fn test_critique_strict_config() {
        let critic = Critic::strict();
        assert!(critic.strict_mode);
        assert_eq!(critic.pass_threshold, 7.5);
        assert_eq!(critic.temperature, 0.2);
    }

    #[test]
    fn test_parse_critique_result() {
        let json = r#"{
            "overall_score": 7.5,
            "coherence_score": 8.0,
            "engagement_score": 6.5,
            "brief_suggestion": "Add more dialogue.",
            "engagement_suggestion": "Create more tension.",
            "suggestion": "Consider adding a cliffhanger."
        }"#;
        let result = parse_critique_result(json).unwrap();
        assert!((result.overall_score - 7.5).abs() < 0.01);
        assert!((result.coherence_score - 8.0).abs() < 0.01);
        assert!((result.engagement_score - 6.5).abs() < 0.01);
        assert_eq!(result.brief_suggestion, "Add more dialogue.");
    }

    #[test]
    fn test_parse_instruction_performance() {
        let json = r#"{
            "follows_instructions": true,
            "detail": "The chapter follows the outline closely.",
            "suggestion": null
        }"#;
        let result = parse_instruction_performance(json).unwrap();
        assert!(result.follows_instructions);
        assert_eq!(result.detail, "The chapter follows the outline closely.");
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_parse_instruction_performance_failed() {
        let json = r#"{
            "follows_instructions": false,
            "detail": "Chapter diverges from outline.",
            "suggestion": "Rewrite to match the outline's plot points."
        }"#;
        let result = parse_instruction_performance(json).unwrap();
        assert!(!result.follows_instructions);
        assert!(result.suggestion.is_some());
    }

    #[test]
    fn test_parse_critique_cleaned() {
        // Test with code fences (common model output pattern)
        let json = "```json\n{\n\"overall_score\": 6.0,\n\"coherence_score\": 7.0,\n\"engagement_score\": 5.5,\n\"brief_suggestion\": \"\",\n\"engagement_suggestion\": \"\",\n\"suggestion\": \"\"\n}\n```";
        let result = parse_critique_result(json).unwrap();
        assert!((result.overall_score - 6.0).abs() < 0.01);
        assert!((result.coherence_score - 7.0).abs() < 0.01);
    }
}
