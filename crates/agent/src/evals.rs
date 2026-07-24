//! Story Evals — model-based evaluation of story quality.
//!
//! Uses the model itself to judge completeness by feeding it critique/approval
//! examples and asking it to critique the story for all factors.
//!
//! # Approach
//!
//! 1. Define evaluation criteria (arc completeness, plot continuity, prose quality)
//! 2. Provide examples of good and bad stories
//! 3. Ask the model to evaluate the story against criteria
//! 4. Use grammar-constrained output for structured evaluation
//!
//! This is more flexible than rule-based evals and leverages the model's
//! understanding of narrative quality.

use roco_engine::ModelBackend;
use roco_grammar::{schema_to_gbnf, Schema};
use crate::util::structured_complete;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Evaluation Criteria
// ═════════════════════════════════════════════════════════════════════════════

/// Evaluation result for a story
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryEval {
    /// Whether the story passes evaluation
    pub passes: bool,
    /// Overall score 0-10
    pub overall_score: f32,
    /// Arc completeness score 0-10
    pub arc_completeness: f32,
    /// Plot continuity score 0-10
    pub plot_continuity: f32,
    /// Prose quality score 0-10
    pub prose_quality: f32,
    /// Character consistency score 0-10
    pub character_consistency: f32,
    /// Pacing score 0-10
    pub pacing: f32,
    /// Specific findings
    pub findings: Vec<EvalFinding>,
    /// Summary of evaluation
    pub summary: String,
    /// Whether revision is recommended
    pub recommend_revision: bool,
    /// Priority areas for revision
    pub revision_priorities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalFinding {
    /// Category: arc, continuity, prose, character, pacing
    pub category: String,
    /// Type: strength, weakness, issue
    pub finding_type: String,
    /// Description
    pub description: String,
    /// Severity if issue: low, medium, high
    pub severity: Option<String>,
}

impl StoryEval {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("passes", Schema::boolean())
            .prop("overall_score", Schema::number())
            .prop("arc_completeness", Schema::number())
            .prop("plot_continuity", Schema::number())
            .prop("prose_quality", Schema::number())
            .prop("character_consistency", Schema::number())
            .prop("pacing", Schema::number())
            .prop(
                "findings",
                Schema::array(
                    Schema::object()
                        .prop(
                            "category",
                            Schema::enum_values(vec![
                                serde_json::json!("arc"),
                                serde_json::json!("continuity"),
                                serde_json::json!("prose"),
                                serde_json::json!("character"),
                                serde_json::json!("pacing"),
                            ]),
                        )
                        .prop(
                            "finding_type",
                            Schema::enum_values(vec![
                                serde_json::json!("strength"),
                                serde_json::json!("weakness"),
                                serde_json::json!("issue"),
                            ]),
                        )
                        .prop("description", Schema::string())
                        .prop(
                            "severity",
                            Schema::enum_values(vec![
                                serde_json::json!("low"),
                                serde_json::json!("medium"),
                                serde_json::json!("high"),
                            ]),
                        )
                        .build(),
                ),
            )
            .prop("summary", Schema::string())
            .prop("recommend_revision", Schema::boolean())
            .prop("revision_priorities", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("StoryEval schema is valid")
    }

    /// Get high-severity issues
    pub fn critical_issues(&self) -> Vec<&EvalFinding> {
        self.findings
            .iter()
            .filter(|f| f.finding_type == "issue" && f.severity.as_deref() == Some("high"))
            .collect()
    }

    /// Get strengths
    pub fn strengths(&self) -> Vec<&EvalFinding> {
        self.findings
            .iter()
            .filter(|f| f.finding_type == "strength")
            .collect()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Evaluator
// ═════════════════════════════════════════════════════════════════════════════

/// Evaluates stories using the model as judge.
pub struct StoryEvaluator {
    /// Minimum passing score (0-10)
    pub pass_threshold: f32,
}

impl Default for StoryEvaluator {
    fn default() -> Self {
        Self {
            pass_threshold: 6.0,
        }
    }
}

impl StoryEvaluator {
    /// Create a new evaluator with custom threshold
    pub fn new(pass_threshold: f32) -> Self {
        Self { pass_threshold }
    }

    /// Evaluate a story for arc completeness, plot continuity, and prose quality.
    ///
    /// Uses the model as judge by providing evaluation criteria and examples.
    pub fn evaluate(
        &self,
        backend: &dyn ModelBackend,
        chapters: &[String],
        outline: &str,
        plot_context: &str,
    ) -> Result<StoryEval, String> {
        let story_text: String = chapters
            .iter()
            .enumerate()
            .map(|(i, ch)| format!("## Chapter {}\n{}", i + 1, ch))
            .collect::<Vec<_>>()
            .join("\n\n");

        let eval: StoryEval = structured_complete(
            backend,
            &self.evaluator_system_prompt(),
            &format!(
                "Evaluate this story against the criteria.\n\n\
                 Outline:\n{outline}\n\n\
                 Story:\n{story_text}\n\n\
                 Plot context:\n{plot_context}\n\n\
                 Provide a thorough evaluation with specific findings.\n\
                 Output JSON matching the schema."
            ),
            &StoryEval::grammar(),
            0.3,
            800,
        )?;

        Ok(eval)
    }

    /// Evaluate a single chapter
    pub fn evaluate_chapter(
        &self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
        chapter_num: usize,
        previous_chapters: &[String],
        plot_context: &str,
    ) -> Result<StoryEval, String> {
        let previous_text: String = previous_chapters
            .iter()
            .enumerate()
            .map(|(i, ch)| format!("## Chapter {} (previous)\n{}", i + 1, ch))
            .collect::<Vec<_>>()
            .join("\n\n");

        let eval: StoryEval = structured_complete(
            backend,
            &self.evaluator_system_prompt(),
            &format!(
                "Evaluate Chapter {chapter_num}.\n\n\
                 Chapter:\n{chapter_text}\n\n\
                 Previous chapters:\n{previous_text}\n\n\
                 Plot context:\n{plot_context}\n\n\
                 Check for continuity with previous chapters and quality.\n\
                 Output JSON matching the schema."
            ),
            &StoryEval::grammar(),
            0.3,
            600,
        )?;

        Ok(eval)
    }

    /// System prompt for evaluator
    fn evaluator_system_prompt(&self) -> String {
        "You are an expert story evaluator. Judge stories against these criteria:\n\n\
         1. Arc Completeness — Does the story have setup, rising action, climax, \
         falling action, resolution? Are all threads resolved?\n\n\
         2. Plot Continuity — Are there contradictions between chapters? Do events \
         flow logically? Are character actions consistent with their motivations?\n\n\
         3. Prose Quality — Is the writing vivid and engaging? Good word choice, \
         imagery, sentence variety? Avoid purple prose and clichés.\n\n\
         4. Character Consistency — Do characters act consistently? Are their voices \
         distinct? Do they develop naturally?\n\n\
         5. Pacing — Is the story well-paced? Are there slow/fast sections \
         appropriate to the narrative? Good scene transitions?\n\n\
         Examples of good critique:\n\
         - 'The climax in Chapter 3 effectively resolves the conflict introduced in Chapter 1'\n\
         - 'Character X's motivation is unclear — why did they suddenly change sides?'\n\
         - 'The prose in Chapter 2 is vivid, with strong sensory details'\n\n\
         Examples of bad critique (avoid these):\n\
         - 'The story is good' (too vague)\n\
         - 'I liked it' (not actionable)\n\
         - 'Needs work' (not specific)\n\n\
         Be specific, actionable, and thorough. Output valid JSON only."
            .to_string()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Revision Generator
// ═════════════════════════════════════════════════════════════════════════════

/// Generates revision instructions based on evaluation.
pub struct RevisionGenerator;

impl RevisionGenerator {
    /// Generate revision instructions from evaluation
    pub fn generate_instructions(eval: &StoryEval) -> String {
        let mut instructions = String::new();

        if eval.recommend_revision {
            instructions.push_str("## Revision Priorities\n\n");
            for (i, priority) in eval.revision_priorities.iter().enumerate() {
                instructions.push_str(&format!("{}. {}\n", i + 1, priority));
            }
            instructions.push('\n');
        }

        // Group findings by category
        let mut issues_by_category: std::collections::HashMap<String, Vec<&EvalFinding>> =
            std::collections::HashMap::new();

        for finding in &eval.findings {
            if finding.finding_type == "issue" {
                issues_by_category
                    .entry(finding.category.clone())
                    .or_default()
                    .push(finding);
            }
        }

        if !issues_by_category.is_empty() {
            instructions.push_str("## Issues to Address\n\n");
            for (category, findings) in &issues_by_category {
                instructions.push_str(&format!("### {}\n", category));
                for finding in findings {
                    let severity = finding.severity.as_deref().unwrap_or("medium");
                    instructions.push_str(&format!(
                        "- [{}] {}\n",
                        severity.to_uppercase(),
                        finding.description
                    ));
                }
                instructions.push('\n');
            }
        }

        // Preserve strengths
        let strengths = eval.strengths();
        if !strengths.is_empty() {
            instructions.push_str("## Strengths to Preserve\n\n");
            for strength in strengths {
                instructions.push_str(&format!("- {}\n", strength.description));
            }
            instructions.push('\n');
        }

        instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_story_eval_critical_issues() {
        let eval = StoryEval {
            passes: true,
            overall_score: 7.0,
            arc_completeness: 8.0,
            plot_continuity: 6.0,
            prose_quality: 7.0,
            character_consistency: 7.0,
            pacing: 7.0,
            findings: vec![
                EvalFinding {
                    category: "continuity".into(),
                    finding_type: "issue".into(),
                    description: "Plot hole in Chapter 2".into(),
                    severity: Some("high".into()),
                },
                EvalFinding {
                    category: "prose".into(),
                    finding_type: "strength".into(),
                    description: "Vivid imagery".into(),
                    severity: None,
                },
            ],
            summary: "Good story with one issue".into(),
            recommend_revision: true,
            revision_priorities: vec!["Fix plot hole".into()],
        };

        assert_eq!(eval.critical_issues().len(), 1);
        assert_eq!(eval.strengths().len(), 1);
    }

    #[test]
    fn test_revision_generator() {
        let eval = StoryEval {
            passes: false,
            overall_score: 5.0,
            arc_completeness: 4.0,
            plot_continuity: 3.0,
            prose_quality: 6.0,
            character_consistency: 5.0,
            pacing: 5.0,
            findings: vec![
                EvalFinding {
                    category: "arc".into(),
                    finding_type: "issue".into(),
                    description: "No clear climax".into(),
                    severity: Some("high".into()),
                },
                EvalFinding {
                    category: "continuity".into(),
                    finding_type: "issue".into(),
                    description: "Character disappears without explanation".into(),
                    severity: Some("medium".into()),
                },
            ],
            summary: "Story needs work on arc and continuity".into(),
            recommend_revision: true,
            revision_priorities: vec![
                "Add a clear climax scene".into(),
                "Explain character's disappearance".into(),
            ],
        };

        let instructions = RevisionGenerator::generate_instructions(&eval);
        assert!(instructions.contains("Revision Priorities"));
        assert!(instructions.contains("Add a clear climax scene"));
        assert!(instructions.contains("HIGH"));
    }
}
