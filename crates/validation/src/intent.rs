//! Story intent classification — maps natural language → `StoryIntent`.
//!
//! # Design
//!
//! Every natural language input goes through the **model first** to determine
//! intent AND extract parameters. The model is the primary classifier.
//!
//! The only bypass is **slash commands** (`/validate`, `/summarize`, etc.),
//! which are parsed directly and dispatch to the corresponding intent.
//!
//! # Flow
//!
//! ```text
//! User input
//!   ├── Slash command? → parse directly → StoryIntent
//!   └── Natural language → model classifies → StoryIntent + params
//! ```
//!
//! # Intents
//!
//! See the `StoryIntent` enum for the full list. Every variant documents
//! the NL patterns that should trigger it and the expected parameters.

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use serde::{Deserialize, Serialize};

/// Internal model response type for intent classification.
#[derive(Debug, Deserialize)]
struct ModelResponse {
    intent: String,
    params: serde_json::Value,
    confidence: f32,
    explanation: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// StoryIntent enum
// ═════════════════════════════════════════════════════════════════════════════

/// Classified intent from user natural language input in story mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StoryIntent {
    // ── Validation ──────────────────────────────────────────────────────
    /// Validate a specific chapter by number.
    ValidateChapter(usize),
    /// Validate all chapters.
    ValidateAllChapters,
    /// Validate the outline.
    ValidateOutline,
    /// Validate wiki/world-building.
    ValidateWiki,
    /// Run all validations.
    ValidateAll,
    /// Evaluate a chapter against the previous one (continuity).
    EvaluateChapterAgainstPrevious(usize),

    // ── Summarization ───────────────────────────────────────────────────
    /// Summarize a specific chapter.
    SummarizeChapter(usize),
    /// Summarize all chapters.
    SummarizeAllChapters,
    /// Summarize the entire story so far.
    SummarizeStory,
    /// Condense a chapter to a data form.
    CondenseChapter(usize),
    /// Condense wiki to a data form.
    CondenseWiki,

    // ── Information retrieval ───────────────────────────────────────────
    /// Find information about a topic/entity in the story.
    FindInfo { query: String },

    // ── Editing / revision ──────────────────────────────────────────────
    /// Edit a chapter with a description of what to change.
    EditChapter {
        num: usize,
        description: String,
    },
    /// Revise a chapter in a given direction.
    ReviseChapter {
        num: usize,
        direction: String,
    },
    /// Change a character's name throughout the story.
    ChangeCharacterName {
        old: String,
        new: String,
    },
    /// Change the writing style.
    ChangeStyle(String),
    /// Change the point of view.
    ChangePOV(String),

    // ── Outline management ──────────────────────────────────────────────
    /// Show what changed in the outline.
    OutlineDiff,
    /// Plan how to modify the story based on outline changes.
    PlanModification,
    /// Sync the outline to match current chapters.
    SyncOutlineToChapters,

    // ── Mode management ─────────────────────────────────────────────────
    /// Lock into a specific story workspace.
    LockStory(String),
    /// Switch to a different story workspace.
    SwitchStory(String),
    /// Resume the last active story.
    ResumeLastStory,
    /// Return to default (non-story) mode.
    UnlockStory,
    /// Get current story status.
    StatusUpdate,

    // ── Creation ────────────────────────────────────────────────────────
    /// Brainstorm a new story idea.
    BrainstormStory,
    /// Expand a premise into a full outline.
    ExpandPremise(String),
}

impl StoryIntent {
    /// Schema for model-based intent classification.
    ///
    /// The model outputs a JSON object with:
    /// - `intent`: the intent name (snake_case)
    /// - `params`: parameters specific to that intent
    pub fn classifier_schema() -> Schema {
        Schema::object()
            .prop("intent", Schema::string())
            .prop("params", Schema::object().build())
            .build()
    }

    /// Human-readable label for this intent.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ValidateChapter(_) => "validate chapter",
            Self::ValidateAllChapters => "validate all chapters",
            Self::ValidateOutline => "validate outline",
            Self::ValidateWiki => "validate wiki",
            Self::ValidateAll => "validate all",
            Self::EvaluateChapterAgainstPrevious(_) => "evaluate continuity",
            Self::SummarizeChapter(_) => "summarize chapter",
            Self::SummarizeAllChapters => "summarize all chapters",
            Self::SummarizeStory => "summarize story",
            Self::CondenseChapter(_) => "condense chapter",
            Self::CondenseWiki => "condense wiki",
            Self::FindInfo { .. } => "find info",
            Self::EditChapter { .. } => "edit chapter",
            Self::ReviseChapter { .. } => "revise chapter",
            Self::ChangeCharacterName { .. } => "change character name",
            Self::ChangeStyle(_) => "change style",
            Self::ChangePOV(_) => "change POV",
            Self::OutlineDiff => "outline diff",
            Self::PlanModification => "plan modification",
            Self::SyncOutlineToChapters => "sync outline",
            Self::LockStory(_) => "lock story",
            Self::SwitchStory(_) => "switch story",
            Self::ResumeLastStory => "resume story",
            Self::UnlockStory => "unlock story",
            Self::StatusUpdate => "status",
            Self::BrainstormStory => "brainstorm",
            Self::ExpandPremise(_) => "expand premise",
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Classifier
// ═════════════════════════════════════════════════════════════════════════════

/// Result of intent classification.
#[derive(Debug)]
pub struct ClassifiedIntent {
    /// The classified intent.
    pub intent: StoryIntent,
    /// Confidence score (0.0 - 1.0) from the model.
    pub confidence: f32,
    /// Raw explanation from the model (if any).
    pub explanation: String,
}

/// NL intent classifier powered by the model.
///
/// Slash commands bypass the model entirely. Everything else goes through
/// the model for classification + parameter extraction.
pub struct IntentClassifier {
    /// Temperature for classification (lower = more consistent).
    pub temperature: f32,
    /// Max tokens for classifier output.
    pub max_tokens: usize,
}

impl Default for IntentClassifier {
    fn default() -> Self {
        Self {
            temperature: 0.2,
            max_tokens: 300,
        }
    }
}

impl IntentClassifier {
    /// Classify a user message into a `StoryIntent`.
    ///
    /// 1. If input starts with `/`, parse as slash command (no model needed).
    /// 2. Otherwise, send to model for classification.
    pub fn classify(
        &self,
        backend: &dyn ModelBackend,
        input: &str,
        available_stories: &[String],
        current_story: Option<&str>,
    ) -> Result<ClassifiedIntent, String> {
        let trimmed = input.trim();

        // Slash command fast path — no model call
        if trimmed.starts_with('/') {
            return self.parse_slash_command(trimmed, available_stories);
        }

        // Model-based classification
        self.classify_with_model(backend, trimmed, available_stories, current_story)
    }

    // ═════════════════════════════════════════════════════════════════════
    // Slash commands — direct parse, no model
    // ═════════════════════════════════════════════════════════════════════

    fn parse_slash_command(
        &self,
        input: &str,
        available_stories: &[String],
    ) -> Result<ClassifiedIntent, String> {
        let input = input.trim();
        // Split into command and the rest (everything after the first space)
        let space_pos = input.find(|c: char| c == ' ' || c == '\t');
        let cmd = match space_pos {
            Some(pos) => input[..pos].to_lowercase(),
            None => input.to_lowercase(),
        };
        let arg = match space_pos {
            Some(pos) => input[pos + 1..].trim().to_string(),
            None => String::new(),
        };

        match cmd.as_str() {
            "/validate" | "/v" => {
                if arg.is_empty() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::ValidateAll,
                        confidence: 1.0,
                        explanation: "Slash command /validate".into(),
                    });
                }
                if arg.eq_ignore_ascii_case("outline") {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::ValidateOutline,
                        confidence: 1.0,
                        explanation: "Slash command /validate outline".into(),
                    });
                }
                if arg.eq_ignore_ascii_case("wiki") || arg.eq_ignore_ascii_case("world") {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::ValidateWiki,
                        confidence: 1.0,
                        explanation: "Slash command /validate wiki".into(),
                    });
                }
                if let Ok(num) = arg.parse::<usize>() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::ValidateChapter(num),
                        confidence: 1.0,
                        explanation: format!("Slash command /validate chapter {num}"),
                    });
                }
                Ok(ClassifiedIntent {
                    intent: StoryIntent::ValidateAll,
                    confidence: 1.0,
                    explanation: format!("Slash command /validate (unknown target: {arg})"),
                })
            }
            "/summarize" | "/s" => {
                if arg.is_empty() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::SummarizeStory,
                        confidence: 1.0,
                        explanation: "Slash command /summarize".into(),
                    });
                }
                if arg.eq_ignore_ascii_case("all") {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::SummarizeAllChapters,
                        confidence: 1.0,
                        explanation: "Slash command /summarize all".into(),
                    });
                }
                if arg.eq_ignore_ascii_case("story") || arg.eq_ignore_ascii_case("full") {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::SummarizeStory,
                        confidence: 1.0,
                        explanation: "Slash command /summarize story".into(),
                    });
                }
                if let Ok(num) = arg.parse::<usize>() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::SummarizeChapter(num),
                        confidence: 1.0,
                        explanation: format!("Slash command /summarize chapter {num}"),
                    });
                }
                Ok(ClassifiedIntent {
                    intent: StoryIntent::SummarizeStory,
                    confidence: 1.0,
                    explanation: format!("Slash command /summarize (unknown arg: {arg})"),
                })
            }
            "/status" | "/st" => Ok(ClassifiedIntent {
                intent: StoryIntent::StatusUpdate,
                confidence: 1.0,
                explanation: "Slash command /status".into(),
            }),
            "/diff" | "/d" => Ok(ClassifiedIntent {
                intent: StoryIntent::OutlineDiff,
                confidence: 1.0,
                explanation: "Slash command /diff".into(),
            }),
            "/plan" | "/p" => Ok(ClassifiedIntent {
                intent: StoryIntent::PlanModification,
                confidence: 1.0,
                explanation: "Slash command /plan".into(),
            }),
            "/brainstorm" | "/b" | "/idea" => {
                let prompt = arg.to_string();
                if prompt.is_empty() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::BrainstormStory,
                        confidence: 1.0,
                        explanation: "Slash command /brainstorm".into(),
                    });
                }
                Ok(ClassifiedIntent {
                    intent: StoryIntent::ExpandPremise(prompt),
                    confidence: 1.0,
                    explanation: format!("Slash command /brainstorm with prompt"),
                })
            }
            "/switch" => {
                if arg.is_empty() && !available_stories.is_empty() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::SwitchStory(available_stories[0].clone()),
                        confidence: 1.0,
                        explanation: "Slash command /switch".into(),
                    });
                }
                Ok(ClassifiedIntent {
                    intent: StoryIntent::SwitchStory(arg.to_string()),
                    confidence: 1.0,
                    explanation: format!("Slash command /switch {arg}"),
                })
            }
            "/lock" => {
                if arg.is_empty() && !available_stories.is_empty() {
                    return Ok(ClassifiedIntent {
                        intent: StoryIntent::LockStory(available_stories[0].clone()),
                        confidence: 1.0,
                        explanation: "Slash command /lock".into(),
                    });
                }
                Ok(ClassifiedIntent {
                    intent: StoryIntent::LockStory(arg.to_string()),
                    confidence: 1.0,
                    explanation: format!("Slash command /lock {arg}"),
                })
            }
            "/unlock" | "/exit" | "/back" | "/q" => Ok(ClassifiedIntent {
                intent: StoryIntent::UnlockStory,
                confidence: 1.0,
                explanation: "Slash command /unlock".into(),
            }),
            "/help" | "/h" | "/?" => Ok(ClassifiedIntent {
                intent: StoryIntent::StatusUpdate,
                confidence: 1.0,
                explanation: "Slash command /help (returning status)".into(),
            }),
            _ => Err(format!("Unknown slash command: {cmd}. Try /help for available commands.")),
        }
    }

    // ═════════════════════════════════════════════════════════════════════
    // Model-based classification
    // ═════════════════════════════════════════════════════════════════════

    fn classify_with_model(
        &self,
        backend: &dyn ModelBackend,
        input: &str,
        available_stories: &[String],
        current_story: Option<&str>,
    ) -> Result<ClassifiedIntent, String> {
        let stories_list = if available_stories.is_empty() {
            "No stories yet.".to_string()
        } else {
            available_stories.join(", ")
        };

        let current = current_story.unwrap_or("none");

        let prompt = format!(
            "You are a story-mode intent classifier. Given the user's message, \
             determine the intent and extract any parameters.\n\n\
             Current story: {current}\nAvailable stories: {stories_list}\n\n\
             User message: {input}\n\n\
             Classify into one of these intents and extract parameters:\n\
             \n\
             VALIDATION:\n\
             - validate_chapter: needs \"num\" (integer)\n\
             - validate_all_chapters\n\
             - validate_outline\n\
             - validate_wiki\n\
             - validate_all\n\
             - evaluate_chapter_against_previous: needs \"num\" (integer)\n\
             \n\
             SUMMARIZATION:\n\
             - summarize_chapter: needs \"num\" (integer)\n\
             - summarize_all_chapters\n\
             - summarize_story\n\
             - condense_chapter: needs \"num\" (integer)\n\
             - condense_wiki\n\
             \n\
             INFORMATION RETRIEVAL:\n\
             - find_info: needs \"query\" (string)\n\
             \n\
             EDITING:\n\
             - edit_chapter: needs \"num\" (integer), \"description\" (string)\n\
             - revise_chapter: needs \"num\" (integer), \"direction\" (string)\n\
             - change_character_name: needs \"old\" (string), \"new\" (string)\n\
             - change_style: needs \"style\" (string)\n\
             - change_pov: needs \"pov\" (string)\n\
             \n\
             OUTLINE:\n\
             - outline_diff\n\
             - plan_modification\n\
             - sync_outline_to_chapters\n\
             \n\
             MODE:\n\
             - lock_story: needs \"name\" (string)\n\
             - switch_story: needs \"name\" (string)\n\
             - resume_last_story\n\
             - unlock_story\n\
             - status_update\n\
             \n\
             CREATION:\n\
             - brainstorm_story\n\
             - expand_premise: needs \"premise\" (string)\n\
             \n\
             Output JSON with:\n\
             - \"intent\": the intent name (snake_case)\n\
             - \"params\": an object with parameters specific to that intent\n\
             - \"confidence\": a number from 0.0 to 1.0\n\
             - \"explanation\": brief explanation of why this classification was chosen",
        );

        let schema = Schema::object()
            .prop("intent", Schema::string())
            .prop("params", Schema::object().build())
            .prop("confidence", Schema::number())
            .prop("explanation", Schema::string())
            .build();

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: "You classify user intent for a story-writing assistant. \
                     Output valid JSON only. No thinking, no reasoning. Only JSON."
                .to_string(),
            prompt,
            grammar: schema.to_gbnf("Classification").ok(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("classifier model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        let response: ModelResponse = serde_json::from_str(&cleaned)
            .map_err(|e| format!("classifier parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))?;

        let intent = self.intent_from_response(&response, input)?;

        Ok(ClassifiedIntent {
            intent,
            confidence: response.confidence,
            explanation: response.explanation,
        })
    }

    /// Convert model response into a `StoryIntent` with extracted parameters.
    fn intent_from_response(
        &self,
        response: &ModelResponse,
        original_input: &str,
    ) -> Result<StoryIntent, String> {
        let params = &response.params;
        let intent_name = response.intent.as_str();

        Ok(match intent_name {
            "validate_chapter" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                StoryIntent::ValidateChapter(num)
            }
            "validate_all_chapters" => StoryIntent::ValidateAllChapters,
            "validate_outline" => StoryIntent::ValidateOutline,
            "validate_wiki" => StoryIntent::ValidateWiki,
            "validate_all" => StoryIntent::ValidateAll,
            "evaluate_chapter_against_previous" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                StoryIntent::EvaluateChapterAgainstPrevious(num)
            }
            "summarize_chapter" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                StoryIntent::SummarizeChapter(num)
            }
            "summarize_all_chapters" => StoryIntent::SummarizeAllChapters,
            "summarize_story" => StoryIntent::SummarizeStory,
            "condense_chapter" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                StoryIntent::CondenseChapter(num)
            }
            "condense_wiki" => StoryIntent::CondenseWiki,
            "find_info" => {
                let query = params
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or(original_input)
                    .to_string();
                StoryIntent::FindInfo { query }
            }
            "edit_chapter" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                let description = params
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(original_input)
                    .to_string();
                StoryIntent::EditChapter { num, description }
            }
            "revise_chapter" => {
                let num = params.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                let direction = params
                    .get("direction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("improve quality")
                    .to_string();
                StoryIntent::ReviseChapter { num, direction }
            }
            "change_character_name" => {
                let old = params
                    .get("old")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let new = params
                    .get("new")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                StoryIntent::ChangeCharacterName { old, new }
            }
            "change_style" => {
                let style = params
                    .get("style")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                StoryIntent::ChangeStyle(style)
            }
            "change_pov" => {
                let pov = params
                    .get("pov")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                StoryIntent::ChangePOV(pov)
            }
            "outline_diff" => StoryIntent::OutlineDiff,
            "plan_modification" => StoryIntent::PlanModification,
            "sync_outline_to_chapters" => StoryIntent::SyncOutlineToChapters,
            "lock_story" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                StoryIntent::LockStory(name)
            }
            "switch_story" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                StoryIntent::SwitchStory(name)
            }
            "resume_last_story" => StoryIntent::ResumeLastStory,
            "unlock_story" => StoryIntent::UnlockStory,
            "status_update" => StoryIntent::StatusUpdate,
            "brainstorm_story" => StoryIntent::BrainstormStory,
            "expand_premise" => {
                let premise = params
                    .get("premise")
                    .and_then(|v| v.as_str())
                    .unwrap_or(original_input)
                    .to_string();
                StoryIntent::ExpandPremise(premise)
            }
            other => {
                return Err(format!("Unknown intent from model: {other}"));
            }
        })
    }
}

/// Print available slash commands to stdout.
pub fn print_slash_help() {
    println!("Available slash commands (bypass model — direct dispatch):");
    println!("  /validate [N|outline|wiki]  Run validation");
    println!("  /summarize [N|all|story]    Generate summary");
    println!("  /status                     Show current story status");
    println!("  /diff                       Show outline diff");
    println!("  /plan                       Generate modification plan");
    println!("  /brainstorm [prompt]        Brainstorm story ideas");
    println!("  /switch <name>              Switch to another story");
    println!("  /lock <name>                Lock into a story workspace");
    println!("  /unlock                     Return to default mode");
    println!("  /help                       Show this help");
    println!();
    println!("Everything else is routed through the model for classification.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_validate() {
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/validate", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::ValidateAll);
        assert_eq!(result.confidence, 1.0);

        let result = classifier.parse_slash_command("/validate 3", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::ValidateChapter(3));

        let result = classifier.parse_slash_command("/validate outline", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::ValidateOutline);

        let result = classifier.parse_slash_command("/validate wiki", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::ValidateWiki);
    }

    #[test]
    fn test_slash_summarize() {
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/summarize", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::SummarizeStory);

        let result = classifier.parse_slash_command("/s 2", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::SummarizeChapter(2));

        let result = classifier.parse_slash_command("/summarize all", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::SummarizeAllChapters);
    }

    #[test]
    fn test_slash_status_and_diff() {
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/status", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::StatusUpdate);

        let result = classifier.parse_slash_command("/diff", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::OutlineDiff);

        let result = classifier.parse_slash_command("/plan", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::PlanModification);
    }

    #[test]
    fn test_slash_brainstorm() {
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/brainstorm", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::BrainstormStory);

        let result = classifier
            .parse_slash_command("/brainstorm a fantasy world", &[])
            .unwrap();
        assert_eq!(
            result.intent,
            StoryIntent::ExpandPremise("a fantasy world".to_string())
        );
    }

    #[test]
    fn test_slash_switch_and_lock() {
        let classifier = IntentClassifier::default();
        let stories = &["fantasy".to_string(), "sci-fi".to_string()];

        let result = classifier.parse_slash_command("/switch sci-fi", stories).unwrap();
        assert_eq!(result.intent, StoryIntent::SwitchStory("sci-fi".to_string()));

        let result = classifier.parse_slash_command("/lock fantasy", stories).unwrap();
        assert_eq!(result.intent, StoryIntent::LockStory("fantasy".to_string()));

        let result = classifier.parse_slash_command("/unlock", stories).unwrap();
        assert_eq!(result.intent, StoryIntent::UnlockStory);
    }

    #[test]
    fn test_unknown_slash_command() {
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/foobar", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_classifier_bypasses_model_for_slash() {
        // This would fail if it tried to call a model backend
        let classifier = IntentClassifier::default();
        let result = classifier.parse_slash_command("/status", &[]).unwrap();
        assert_eq!(result.intent, StoryIntent::StatusUpdate);
    }

    #[test]
    fn test_intent_labels() {
        assert_eq!(StoryIntent::ValidateChapter(1).label(), "validate chapter");
        assert_eq!(StoryIntent::SummarizeStory.label(), "summarize story");
        assert_eq!(StoryIntent::BrainstormStory.label(), "brainstorm");
        assert_eq!(StoryIntent::UnlockStory.label(), "unlock story");
    }
}
