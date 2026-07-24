//! Natural Language Feedback — parse human feedback into structured directives.
//!
//! The human says things like:
//! - "make it darker"
//! - "add more dialogue"
//! - "the pacing is too slow"
//! - "I want the knight to hesitate before drawing his sword"
//!
//! The agent parses this into structured directives that can be applied.

use roco_engine::ModelBackend;
use roco_grammar::{schema_to_gbnf, Schema};
use crate::util::structured_complete;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Feedback Types
// ═════════════════════════════════════════════════════════════════════════════

/// Parsed feedback from natural language
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFeedback {
    /// The original feedback text
    pub original: String,
    /// The intent: revise, continue, stop, skip, direction
    pub intent: FeedbackIntent,
    /// Specific directives to apply
    pub directives: Vec<Directive>,
    /// Confidence in parsing (0-1)
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FeedbackIntent {
    /// Revise the current chapter
    Revise,
    /// Continue to next chapter
    Continue,
    /// Stop and publish
    Stop,
    /// Skip to next chapter
    Skip,
    /// Set direction for next chapter
    Direction,
    /// Give general feedback (not actionable yet)
    General,
}

/// A specific directive from feedback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    /// Type: tone, pacing, character, plot, style, content
    pub directive_type: String,
    /// What to change
    pub target: String,
    /// How to change it
    pub action: String,
    /// Specific instruction
    pub instruction: String,
}

impl ParsedFeedback {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("original", Schema::string())
            .prop(
                "intent",
                Schema::enum_values(vec![
                    serde_json::json!("revise"),
                    serde_json::json!("continue"),
                    serde_json::json!("stop"),
                    serde_json::json!("skip"),
                    serde_json::json!("direction"),
                    serde_json::json!("general"),
                ]),
            )
            .prop(
                "directives",
                Schema::array(
                    Schema::object()
                        .prop(
                            "directive_type",
                            Schema::enum_values(vec![
                                serde_json::json!("tone"),
                                serde_json::json!("pacing"),
                                serde_json::json!("character"),
                                serde_json::json!("plot"),
                                serde_json::json!("style"),
                                serde_json::json!("content"),
                            ]),
                        )
                        .prop("target", Schema::string())
                        .prop("action", Schema::string())
                        .prop("instruction", Schema::string())
                        .build(),
                ),
            )
            .prop("confidence", Schema::number())
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("ParsedFeedback schema is valid")
    }

    /// Check if this feedback requires action
    pub fn requires_action(&self) -> bool {
        self.intent == FeedbackIntent::Revise || self.intent == FeedbackIntent::Direction
    }

    /// Get a human-readable summary of the directives
    pub fn summary(&self) -> String {
        if self.directives.is_empty() {
            return "No specific directives".to_string();
        }

        let mut summary = String::new();
        for directive in &self.directives {
            summary.push_str(&format!(
                "- {}: {} ({})\n",
                directive.directive_type, directive.action, directive.instruction
            ));
        }
        summary
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Feedback Parser
// ═════════════════════════════════════════════════════════════════════════════

/// Parses natural language feedback into structured directives.
pub struct FeedbackParser;

impl FeedbackParser {
    /// Parse natural language feedback
    pub fn parse(
        backend: &dyn ModelBackend,
        feedback: &str,
        context: &str,
    ) -> Result<ParsedFeedback, String> {
        let parsed: ParsedFeedback = structured_complete(
            backend,
            "You are a feedback parser. Parse natural language feedback into structured directives. Output valid JSON only.",
            &format!(
                "Parse this feedback:\n\n{feedback}\n\n\
                 Context:\n{context}\n\n\
                 Determine the intent and extract specific directives.\n\
                 Output JSON matching the schema."
            ),
            &ParsedFeedback::grammar(),
            0.3,
            400,
        )?;

        Ok(parsed)
    }

    /// Quick parse for simple commands (no model call needed)
    pub fn quick_parse(feedback: &str) -> Option<ParsedFeedback> {
        let lower = feedback.trim().to_lowercase();

        let intent = if lower == "c" || lower == "continue" || lower == "next" {
            FeedbackIntent::Continue
        } else if lower == "s" || lower == "skip" {
            FeedbackIntent::Skip
        } else if lower == "q" || lower == "stop" || lower == "quit" {
            FeedbackIntent::Stop
        } else {
            return None; // Need model to parse
        };

        Some(ParsedFeedback {
            original: feedback.to_string(),
            intent,
            directives: Vec::new(),
            confidence: 1.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_parse_continue() {
        let parsed = FeedbackParser::quick_parse("c").unwrap();
        assert_eq!(parsed.intent, FeedbackIntent::Continue);
        assert_eq!(parsed.confidence, 1.0);
    }

    #[test]
    fn test_quick_parse_skip() {
        let parsed = FeedbackParser::quick_parse("skip").unwrap();
        assert_eq!(parsed.intent, FeedbackIntent::Skip);
    }

    #[test]
    fn test_quick_parse_stop() {
        let parsed = FeedbackParser::quick_parse("q").unwrap();
        assert_eq!(parsed.intent, FeedbackIntent::Stop);
    }

    #[test]
    fn test_quick_parse_unknown() {
        let parsed = FeedbackParser::quick_parse("make it darker");
        assert!(parsed.is_none()); // Needs model to parse
    }
}
