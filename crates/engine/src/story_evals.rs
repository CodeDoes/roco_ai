//! Story Pipeline Evals — GPU-backed evaluation of the structured story pipeline.
//!
//! Each eval tests one stage of the story pipeline against a real model backend:
//!
//! 1. **Outline** — generates a structured outline with title, genre, tone, chapters
//! 2. **Worldbuilding** — builds character bios and setting lore from premise + outline
//! 3. **Chapter Write** — writes prose chapter with write/edit/read tool access
//! 4. **Validation** — critiques a chapter for quality (runs once per chapter)
//! 5. **Revising** — revises a chapter based on validation feedback
//! 6. **Natural Language Mode Selection** — infers intent from user utterance
//!
//! # Usage
//!
//! ```ignore
//! use roco_engine::story_evals::run_story_pipeline_evals;
//! let results = run_story_pipeline_evals(backend, &premise).await;
//! ```
//!
//! Each eval returns an [`EvalResult`] compatible with the eval report system,
//! so results can be aggregated, reported, and snapshotted the same way as
//! simple eval cases.

use crate::eval::{CheckResult, EvalCategory, EvalResult};
use crate::{CompletionRequest, ModelBackend};
use serde::Deserialize;

// ═════════════════════════════════════════════════════════════════════════════
// Story Eval Types
// ═════════════════════════════════════════════════════════════════════════════

/// Per-eval configuration for story pipeline stages
#[derive(Debug, Clone)]
pub struct StoryEvalConfig {
    pub premise: String,
    pub temperature: f32,
    pub max_tokens_outline: usize,
    pub max_tokens_wiki: usize,
    pub max_tokens_chapter: usize,
    pub max_tokens_validation: usize,
    pub max_tokens_revision: usize,
}

impl Default for StoryEvalConfig {
    fn default() -> Self {
        Self {
            premise: "A lighthouse keeper discovers a message in a bottle.".into(),
            temperature: 0.7,
            max_tokens_outline: 400,
            max_tokens_wiki: 500,
            max_tokens_chapter: 800,
            max_tokens_validation: 200,
            max_tokens_revision: 800,
        }
    }
}

/// Structured outline result from the model
#[derive(Debug, Deserialize)]
pub struct EvalOutline {
    pub title: String,
    pub genre: String,
    pub tone: String,
    pub chapters: Vec<EvalChapterDef>,
}

#[derive(Debug, Deserialize)]
pub struct EvalChapterDef {
    pub number: u32,
    pub title: String,
    pub summary: String,
}

/// Wiki/worldbuilding result
#[derive(Debug, Deserialize)]
pub struct EvalWiki {
    pub characters: Vec<EvalCharacter>,
    pub setting: String,
}

#[derive(Debug, Deserialize)]
pub struct EvalCharacter {
    pub name: String,
    pub description: String,
}

/// Validation result (run once per chapter)
#[derive(Debug, Deserialize)]
pub struct EvalValidation {
    pub quality: String,
    pub issues: String,
    pub suggestion: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Mode Selection Types
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct EvalModeSelection {
    pub mode: String,
    pub confidence: String,
    pub reasoning: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Outline Generation
// ═════════════════════════════════════════════════════════════════════════════

/// Eval the outline generation stage of the story pipeline.
pub async fn eval_outline<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let system = "You are a story outliner. Output valid JSON only. \
        Do NOT include any thinking or reasoning. Output ONLY the JSON object.";
    let prompt = format!(
        "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
         Output JSON matching the schema: title, genre, tone, chapters \
         (array of 3 objects with number, title, summary).",
        premise = config.premise
    );

    let text = match do_complete(
        backend,
        system,
        &prompt,
        config.temperature.min(0.6),
        config.max_tokens_outline,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: "story_pipeline_outline".into(),
                description: "Generates a structured outline from a premise".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    // Attempt to parse JSON
    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    match serde_json::from_str::<EvalOutline>(&cleaned) {
        Ok(outline) => {
            // Check: title exists and is non-empty
            let has_title = !outline.title.trim().is_empty();
            checks.push(CheckResult {
                name: "has_title".into(),
                passed: has_title,
                detail: if has_title {
                    format!("title: \"{}\"", outline.title)
                } else {
                    "title is empty".into()
                },
            });

            // Check: genre exists
            let has_genre = !outline.genre.trim().is_empty();
            checks.push(CheckResult {
                name: "has_genre".into(),
                passed: has_genre,
                detail: if has_genre {
                    format!("genre: \"{}\"", outline.genre)
                } else {
                    "genre is empty".into()
                },
            });

            // Check: tone exists
            let has_tone = !outline.tone.trim().is_empty();
            checks.push(CheckResult {
                name: "has_tone".into(),
                passed: has_tone,
                detail: if has_tone {
                    format!("tone: \"{}\"", outline.tone)
                } else {
                    "tone is empty".into()
                },
            });

            // Check: has 3 chapters
            let has_three_chapters = outline.chapters.len() == 3;
            checks.push(CheckResult {
                name: "three_chapters".into(),
                passed: has_three_chapters,
                detail: if has_three_chapters {
                    "exactly 3 chapters defined".into()
                } else {
                    format!("{} chapters (expected 3)", outline.chapters.len())
                },
            });

            // Check: each chapter has title and summary
            let all_valid = outline
                .chapters
                .iter()
                .all(|c| !c.title.trim().is_empty() && !c.summary.trim().is_empty());
            checks.push(CheckResult {
                name: "all_chapters_valid".into(),
                passed: all_valid,
                detail: if all_valid {
                    "all chapters have non-empty title and summary".into()
                } else {
                    "some chapters have empty title or summary".into()
                },
            });

            // Check: no thinking contamination
            let no_thinking = !text.contains("thinking") && !text.contains("</think>");
            checks.push(CheckResult {
                name: "no_thinking_tags".into(),
                passed: no_thinking,
                detail: if no_thinking {
                    "no thinking tags found".into()
                } else {
                    "thinking tags present in output".into()
                },
            });

            let passed = checks.iter().all(|c| c.passed);
            EvalResult {
                name: "story_pipeline_outline".into(),
                description: "Generates a structured outline from a premise".into(),
                category: EvalCategory::Coherence,
                passed,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
                oracle: None,
            }
        }
        Err(e) => {
            errors.push(format!("JSON parse error: {e}"));
            EvalResult {
                name: "story_pipeline_outline".into(),
                description: "Generates a structured outline from a premise".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "parse_json".into(),
                    passed: false,
                    detail: format!("could not parse outline JSON: {e}"),
                }],
                errors,
                oracle: None,
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Worldbuilding
// ═════════════════════════════════════════════════════════════════════════════

/// Eval the worldbuilding stage: generates character bios and setting lore.
pub async fn eval_worldbuilding<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
    outline_text: &str,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let system = "You are a worldbuilding assistant. Output valid JSON only. \
        No thinking, no reasoning, no commentary. Only JSON.";
    let prompt = format!(
        "Based on this premise and outline, create character bios and setting lore:\n\n\
         Premise: {premise}\nOutline: {outline}\n\n\
         Output JSON matching the schema: characters (array of objects with name, description), \
         setting (string).",
        premise = config.premise,
        outline = outline_text
    );

    let text = match do_complete(
        backend,
        system,
        &prompt,
        config.temperature.min(0.7),
        config.max_tokens_wiki,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: "story_pipeline_worldbuilding".into(),
                description: "Builds world bible from premise + outline".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    match serde_json::from_str::<EvalWiki>(&cleaned) {
        Ok(wiki) => {
            // Check: at least one character
            let has_characters = !wiki.characters.is_empty();
            checks.push(CheckResult {
                name: "has_characters".into(),
                passed: has_characters,
                detail: if has_characters {
                    format!("{} character(s) defined", wiki.characters.len())
                } else {
                    "no characters defined".into()
                },
            });

            // Check: all characters have name and description
            let all_valid = wiki
                .characters
                .iter()
                .all(|c| !c.name.trim().is_empty() && !c.description.trim().is_empty());
            checks.push(CheckResult {
                name: "characters_valid".into(),
                passed: all_valid,
                detail: if all_valid {
                    "all characters have name and description".into()
                } else {
                    "some characters missing name or description".into()
                },
            });

            // Check: setting exists
            let has_setting = !wiki.setting.trim().is_empty();
            checks.push(CheckResult {
                name: "has_setting".into(),
                passed: has_setting,
                detail: if has_setting {
                    format!("setting defined ({} chars)", wiki.setting.len())
                } else {
                    "setting is empty".into()
                },
            });

            // Check: no thinking contamination
            let no_thinking = !text.contains("thinking") && !text.contains(" response");
            checks.push(CheckResult {
                name: "no_thinking_tags".into(),
                passed: no_thinking,
                detail: if no_thinking {
                    "no thinking tags found".into()
                } else {
                    "thinking tags present in output".into()
                },
            });

            let passed = checks.iter().all(|c| c.passed);
            EvalResult {
                name: "story_pipeline_worldbuilding".into(),
                description: "Builds world bible from premise + outline".into(),
                category: EvalCategory::Coherence,
                passed,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
                oracle: None,
            }
        }
        Err(e) => {
            errors.push(format!("JSON parse error: {e}"));
            EvalResult {
                name: "story_pipeline_worldbuilding".into(),
                description: "Builds world bible from premise + outline".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "parse_json".into(),
                    passed: false,
                    detail: format!("could not parse wiki JSON: {e}"),
                }],
                errors,
                oracle: None,
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Chapter Write (with write/edit/read tool access)
// ═════════════════════════════════════════════════════════════════════════════

/// Eval the chapter writing stage.
///
/// The chapter writer has access to "write", "edit", and "read" tools to
/// produce the chapter prose. This eval verifies that the model can produce
/// a coherent chapter with proper formatting, no thinking contamination,
/// and appropriate paragraph structure.
pub async fn eval_chapter_write<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
    chapter_num: u32,
    chapter_label: &str,
    outline_text: &str,
    wiki_text: &str,
    previous_chapter: &str,
    is_revision: bool,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let label = if is_revision {
        format!("{chapter_label} (revision)")
    } else {
        chapter_label.to_string()
    };

    let system = "You are a fiction writer. Write vivid, engaging prose. \
        Output valid JSON only. NEVER include thinking, reasoning, \
        or meta-commentary in your output. Only the JSON object.";

    let directive = if chapter_num == 1 {
        format!(
            "Write {label}. Introduce the main character and setting. \
             ~400 words of vivid prose.\n\n\
             Rules:\n\
             - Write actual story prose, NOT meta-commentary or planning.\n\
             - Start directly with the narrative.\n\
             - Use paragraph breaks (double newlines) between scenes.\n\
             - Do NOT include thinking, reasoning, or commentary.\n\n\
             You have access to write, edit, and read tools to produce this chapter.\n\n\
             Outline context:\n{outline_text}\n\n\
             World info:\n{wiki_text}\n\n\
             Output JSON with: title (string), content (string, the chapter prose)"
        )
    } else {
        format!(
            "Write {label}. Continue from where the previous chapter left off. \
             Advance the plot. ~400 words of vivid prose.\n\n\
             Rules:\n\
             - Write actual story prose, NOT meta-commentary or planning.\n\
             - Start directly with the narrative.\n\
             - Use paragraph breaks (double newlines) between scenes.\n\
             - Do NOT include thinking, reasoning, or commentary.\n\n\
             You have access to write, edit, and read tools to produce this chapter.\n\n\
             Previous chapter recap:\n{previous_chapter}\n\n\
             Outline context:\n{outline_text}\n\n\
             World info:\n{wiki_text}\n\n\
             Output JSON with: title (string), content (string, the chapter prose)"
        )
    };

    // Note: In the full pipeline, write/edit/read tools are provided via the
    // MechanisticAgent's tool registry. The eval tests that the model can
    // produce prose that respects paragraph structure and avoids contamination.
    let text = match do_complete(
        backend,
        system,
        &directive,
        config.temperature,
        config.max_tokens_chapter,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: format!("story_pipeline_chapter_write_{}", chapter_num),
                description: format!("Writes chapter {chapter_num} with prose"),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {directive}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    // The chapter writer can output either JSON (with content field) or raw prose
    // Attempt JSON parse first, fall back to treating the whole output as content
    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    let (title, content) = match serde_json::from_str::<serde_json::Value>(&cleaned) {
        Ok(val) => {
            let t = val
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(chapter_label);
            let c = val
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or(&cleaned);
            (t.to_string(), c.to_string())
        }
        Err(_) => {
            // Raw prose output
            (chapter_label.to_string(), cleaned)
        }
    };

    // Apply cleaning that the pipeline uses
    let content_cleaned = crate::util::clean_story_text(&content);

    // Run all checks on the prose

    // Check: non-empty content
    let has_content = !content_cleaned.trim().is_empty();
    checks.push(CheckResult {
        name: "has_content".into(),
        passed: has_content,
        detail: if has_content {
            format!(
                "{} words of prose",
                content_cleaned.split_whitespace().count()
            )
        } else {
            "chapter content is empty".into()
        },
    });

    // Check: minimum word count (at least 50 words for an eval)
    let word_count = content_cleaned.split_whitespace().count();
    let min_words = 50;
    let reaches_min = word_count >= min_words;
    checks.push(CheckResult {
        name: "min_word_count".into(),
        passed: reaches_min,
        detail: if reaches_min {
            format!("{} >= {} words", word_count, min_words)
        } else {
            format!("{} < {} words", word_count, min_words)
        },
    });

    // Check: has paragraph breaks (double newlines)
    let has_paragraphs = content_cleaned.contains("\n\n");
    checks.push(CheckResult {
        name: "has_paragraphs".into(),
        passed: has_paragraphs,
        detail: if has_paragraphs {
            "contains paragraph breaks".into()
        } else {
            "no paragraph breaks found (single block of text)".into()
        },
    });

    // Check: no thinking contamination
    let no_thinking = !text.contains("thinking") && !text.contains(" response");
    checks.push(CheckResult {
        name: "no_thinking_tags".into(),
        passed: no_thinking,
        detail: if no_thinking {
            "no thinking tags found".into()
        } else {
            "thinking tags present in output".into()
        },
    });

    // Check: has a title
    let has_title = !title.trim().is_empty() && title != chapter_label;
    checks.push(CheckResult {
        name: "has_title".into(),
        passed: has_title,
        detail: if has_title {
            format!("title: \"{}\"", title)
        } else {
            "no chapter title extracted".into()
        },
    });

    // Check: prose contains actual narrative (not meta-commentary)
    let has_narrative = content_cleaned.len() > 100
        && !content_cleaned.to_lowercase().starts_with("here is")
        && !content_cleaned.to_lowercase().starts_with("i'll write");
    checks.push(CheckResult {
        name: "narrative_not_meta".into(),
        passed: has_narrative,
        detail: if has_narrative {
            "content reads as narrative prose".into()
        } else {
            "content may be meta-commentary rather than story".into()
        },
    });

    let passed = checks.iter().all(|c| c.passed);
    let output_display = if content_cleaned.len() > 500 {
        format!("{}... (first 500 chars)", &content_cleaned[..500])
    } else {
        content_cleaned.clone()
    };

    EvalResult {
        name: format!("story_pipeline_chapter_write_{}", chapter_num),
        description: format!(
            "Writes chapter {chapter_num} with prose (writer has write/edit/read tools)"
        ),
        category: EvalCategory::Coherence,
        passed,
        input: format!("System: {system}\n\nUser: {directive}"),
        output: output_display,
        latency_ms: 0,
        token_usage: Default::default(),
        tokens_per_sec: 0.0,
        checks,
        errors,
        oracle: None,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Chapter Validation (runs once per chapter)
// ═════════════════════════════════════════════════════════════════════════════

/// Eval the chapter validation stage.
///
/// **Note:** Validation runs exactly once per chapter in the pipeline.
/// It does NOT run multiple times per chapter. The validator evaluates
/// quality and suggests improvements, but the pipeline moves on after
/// one validation call. If revision is needed, a separate revise step is used.
pub async fn eval_validation<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
    chapter_num: u32,
    chapter_text: &str,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let system = "You are a quality reviewer. Be strict. Output valid JSON only. \
        Check for meta-commentary and thinking contamination.";
    let prompt = format!(
        "Review this chapter and check for:\n\
         1. Does it read like a coherent story (not meta-commentary)?\n\
         2. Is the prose engaging with proper paragraph breaks?\n\
         3. Does it avoid thinking/reasoning tags?\n\n\
         Chapter:\n{chapter_text}\n\n\
         Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
         issues (string), suggestion (string)."
    );

    let text = match do_complete(backend, system, &prompt, 0.3, config.max_tokens_validation).await
    {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: format!("story_pipeline_validation_{}", chapter_num),
                description: "Validates chapter quality (runs once per chapter)".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    match serde_json::from_str::<EvalValidation>(&cleaned) {
        Ok(val) => {
            // Check: quality field is one of the expected values
            let valid_quality = matches!(val.quality.as_str(), "pass" | "fail" | "needs-work");
            checks.push(CheckResult {
                name: "valid_quality_rating".into(),
                passed: valid_quality,
                detail: if valid_quality {
                    format!("quality: \"{}\"", val.quality)
                } else {
                    format!("unexpected quality value: \"{}\"", val.quality)
                },
            });

            // Check: issues field is non-empty
            let has_issues = !val.issues.trim().is_empty();
            checks.push(CheckResult {
                name: "has_issues".into(),
                passed: has_issues,
                detail: if has_issues {
                    format!("issues: \"{}\"", val.issues)
                } else {
                    "issues field is empty".into()
                },
            });

            // Check: suggestion field is non-empty
            let has_suggestion = !val.suggestion.trim().is_empty();
            checks.push(CheckResult {
                name: "has_suggestion".into(),
                passed: has_suggestion,
                detail: if has_suggestion {
                    format!("suggestion: \"{}\"", val.suggestion)
                } else {
                    "suggestion field is empty".into()
                },
            });

            // Check: no thinking contamination in the validation output itself
            let no_thinking = !text.contains("thinking") && !text.contains(" response");
            checks.push(CheckResult {
                name: "no_thinking_tags".into(),
                passed: no_thinking,
                detail: if no_thinking {
                    "no thinking tags found".into()
                } else {
                    "thinking tags present in output".into()
                },
            });

            let passed = checks.iter().all(|c| c.passed);
            EvalResult {
                name: format!("story_pipeline_validation_{}", chapter_num),
                description: "Validates chapter quality (runs once per chapter)".into(),
                category: EvalCategory::Coherence,
                passed,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
                oracle: None,
            }
        }
        Err(e) => {
            errors.push(format!("JSON parse error: {e}"));
            EvalResult {
                name: format!("story_pipeline_validation_{}", chapter_num),
                description: "Validates chapter quality (runs once per chapter)".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "parse_json".into(),
                    passed: false,
                    detail: format!("could not parse validation JSON: {e}"),
                }],
                errors,
                oracle: None,
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Chapter Revising
// ═════════════════════════════════════════════════════════════════════════════

/// Eval the chapter revising stage.
///
/// Revises a chapter based on validation feedback. This is triggered only
/// when validation returns "fail" or "needs-work".
pub async fn eval_revising<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
    chapter_num: u32,
    original_text: &str,
    validation_feedback: &str,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let system = "You are a fiction writer revising your work. \
        Address the feedback given and improve the chapter. \
        Output valid JSON only. NEVER include thinking, reasoning, \
        or meta-commentary. Only the JSON object.";

    let prompt = format!(
        "Revise Chapter {chapter_num} based on this feedback:\n\n\
         Original chapter:\n{original_text}\n\n\
         Feedback to address:\n{validation_feedback}\n\n\
         Rules:\n\
         - Fix the specific issues mentioned\n\
         - Keep what works\n\
         - Write ~400 words of vivid prose\n\
         - Use paragraph breaks (double newlines) between scenes\n\
         - Do NOT include thinking, reasoning, or commentary\n\n\
         You have access to write, edit, and read tools to produce the revision.\n\n\
         Output JSON with: title (string), content (string, the revised chapter prose)"
    );

    let text = match do_complete(
        backend,
        system,
        &prompt,
        config.temperature,
        config.max_tokens_revision,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: format!("story_pipeline_revising_{}", chapter_num),
                description: "Revises a chapter based on validation feedback".into(),
                category: EvalCategory::Coherence,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    let content = match serde_json::from_str::<serde_json::Value>(&cleaned) {
        Ok(val) => val
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or(&cleaned)
            .to_string(),
        Err(_) => cleaned,
    };

    let content_cleaned = crate::util::clean_story_text(&content);

    // Check: content is non-empty
    let has_content = !content_cleaned.trim().is_empty();
    checks.push(CheckResult {
        name: "has_content".into(),
        passed: has_content,
        detail: if has_content {
            format!(
                "{} words after revision",
                content_cleaned.split_whitespace().count()
            )
        } else {
            "revised content is empty".into()
        },
    });

    // Check: content is different from original (indicating revision happened)
    let is_different = content_cleaned.trim() != original_text.trim();
    checks.push(CheckResult {
        name: "content_changed".into(),
        passed: is_different,
        detail: if is_different {
            "content differs from original (revision applied)".into()
        } else {
            "content unchanged from original".into()
        },
    });

    // Check: no thinking contamination
    let no_thinking = !text.contains("thinking") && !text.contains(" response");
    checks.push(CheckResult {
        name: "no_thinking_tags".into(),
        passed: no_thinking,
        detail: if no_thinking {
            "no thinking tags found".into()
        } else {
            "thinking tags present in output".into()
        },
    });

    // Check: has paragraph breaks
    let has_paragraphs = content_cleaned.contains("\n\n");
    checks.push(CheckResult {
        name: "has_paragraphs".into(),
        passed: has_paragraphs,
        detail: if has_paragraphs {
            "revised version has paragraph breaks".into()
        } else {
            "no paragraph breaks in revision".into()
        },
    });

    let passed = checks.iter().all(|c| c.passed);
    let output_display = if content_cleaned.len() > 500 {
        format!("{}... (first 500 chars)", &content_cleaned[..500])
    } else {
        content_cleaned.clone()
    };

    EvalResult {
        name: format!("story_pipeline_revising_{}", chapter_num),
        description: "Revises a chapter based on validation feedback".into(),
        category: EvalCategory::Coherence,
        passed,
        input: format!("System: {system}\n\nUser: {prompt}"),
        output: output_display,
        latency_ms: 0,
        token_usage: Default::default(),
        tokens_per_sec: 0.0,
        checks,
        errors,
        oracle: None,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Eval: Natural Language Mode Selection
// ═════════════════════════════════════════════════════════════════════════════

/// Test utterances for mode selection evaluation.
pub struct ModeSelectionTestCase {
    pub name: &'static str,
    pub utterance: &'static str,
    pub expected_mode: &'static str,
}

/// Canonical test cases for natural language mode selection.
///
/// These cover the range of user intents the system should recognize:
/// - Story writing requests → "story" or "write"
/// - Chat/conversation → "chat" or "converse"
/// - Editing/revising → "edit" or "revise"
/// - Reading/viewing → "read" or "view"
pub const MODE_SELECTION_CASES: &[ModeSelectionTestCase] = &[
    ModeSelectionTestCase {
        name: "mode_write_story",
        utterance: "Write me a story about a dragon who learns to bake cookies",
        expected_mode: "write",
    },
    ModeSelectionTestCase {
        name: "mode_chat_general",
        utterance: "What do you think about the weather today?",
        expected_mode: "chat",
    },
    ModeSelectionTestCase {
        name: "mode_edit_chapter",
        utterance: "Can you revise chapter 3 to have a stronger ending?",
        expected_mode: "edit",
    },
    ModeSelectionTestCase {
        name: "mode_read_story",
        utterance: "Show me what we wrote yesterday",
        expected_mode: "read",
    },
    ModeSelectionTestCase {
        name: "mode_outline_new",
        utterance: "Help me plan a science fiction novel about time travel",
        expected_mode: "outline",
    },
    ModeSelectionTestCase {
        name: "mode_continue_write",
        utterance: "Continue the story from where we left off",
        expected_mode: "write",
    },
    ModeSelectionTestCase {
        name: "mode_validate_quality",
        utterance: "Check if this chapter is any good",
        expected_mode: "validate",
    },
];

/// Eval natural language mode selection.
///
/// Tests that the model can correctly infer user intent from natural language
/// utterances and map them to system modes (write, chat, edit, read, etc.).
pub async fn eval_mode_selection<B: ModelBackend + Send + Sync>(
    backend: &B,
    test_case: &ModeSelectionTestCase,
) -> EvalResult {
    let mut errors = Vec::new();
    let mut checks = Vec::new();

    let system = "You are a mode classifier. Given a user utterance, determine \
        what the user wants to do. Output valid JSON only with fields: \
        mode (string), confidence (\"high\" | \"medium\" | \"low\"), reasoning (string).\n\n\
        Modes:\n\
        - \"write\": User wants to write new story content, chapters, or prose\n\
        - \"chat\": User wants general conversation, questions, or discussion\n\
        - \"edit\": User wants to revise, change, or improve existing content\n\
        - \"read\": User wants to view, review, or browse existing content\n\
        - \"outline\": User wants to plan, structure, or outline a story\n\
        - \"validate\": User wants quality check, review, or critique";
    let prompt = format!(
        "Classify this user utterance into one of the modes:\n\n\"{}\"",
        test_case.utterance
    );

    let text = match do_complete(backend, system, &prompt, 0.3, 100).await {
        Ok(t) => t,
        Err(e) => {
            errors.push(e);
            return EvalResult {
                name: format!("mode_selection_{}", test_case.name),
                description: format!("Classifies utterance: \"{}\"", test_case.utterance),
                category: EvalCategory::Instruction,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: String::new(),
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "model_call".into(),
                    passed: false,
                    detail: "model call failed".into(),
                }],
                errors,
                oracle: None,
            };
        }
    };

    let cleaned = roco_grammar::strategies::strip_code_fences(&text);
    match serde_json::from_str::<EvalModeSelection>(&cleaned) {
        Ok(sel) => {
            // Check: mode matches expected
            let mode_matches = sel.mode == test_case.expected_mode;
            checks.push(CheckResult {
                name: "correct_mode".into(),
                passed: mode_matches,
                detail: if mode_matches {
                    format!(
                        "mode \"{}\" matches expected \"{}\"",
                        sel.mode, test_case.expected_mode
                    )
                } else {
                    format!(
                        "mode \"{}\" != expected \"{}\" (reasoning: {})",
                        sel.mode, test_case.expected_mode, sel.reasoning
                    )
                },
            });

            // Check: confidence is valid
            let valid_confidence = matches!(sel.confidence.as_str(), "high" | "medium" | "low");
            checks.push(CheckResult {
                name: "valid_confidence".into(),
                passed: valid_confidence,
                detail: if valid_confidence {
                    format!("confidence: \"{}\"", sel.confidence)
                } else {
                    format!("invalid confidence: \"{}\"", sel.confidence)
                },
            });

            // Check: reasoning is present
            let has_reasoning = !sel.reasoning.trim().is_empty();
            checks.push(CheckResult {
                name: "has_reasoning".into(),
                passed: has_reasoning,
                detail: if has_reasoning {
                    format!("reasoning: \"{}\"", sel.reasoning)
                } else {
                    "reasoning is empty".into()
                },
            });

            let passed = checks.iter().all(|c| c.passed);
            EvalResult {
                name: format!("mode_selection_{}", test_case.name),
                description: format!("Classifies utterance: \"{}\"", test_case.utterance),
                category: EvalCategory::Instruction,
                passed,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
                oracle: None,
            }
        }
        Err(e) => {
            errors.push(format!("JSON parse error: {e}"));
            EvalResult {
                name: format!("mode_selection_{}", test_case.name),
                description: format!("Classifies utterance: \"{}\"", test_case.utterance),
                category: EvalCategory::Instruction,
                passed: false,
                input: format!("System: {system}\n\nUser: {prompt}"),
                output: text,
                latency_ms: 0,
                token_usage: Default::default(),
                tokens_per_sec: 0.0,
                checks: vec![CheckResult {
                    name: "parse_json".into(),
                    passed: false,
                    detail: format!("could not parse mode selection JSON: {e}"),
                }],
                errors,
                oracle: None,
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Combined runner
// ═════════════════════════════════════════════════════════════════════════════

/// Run the complete story pipeline eval suite.
///
/// Executes each stage in order (outline → worldbuilding → chapter writes →
/// validations → revising) and returns all results. Mode selection evals are
/// independent and run alongside the pipeline.
pub async fn run_full_story_eval_suite<B: ModelBackend + Send + Sync>(
    backend: &B,
    config: &StoryEvalConfig,
) -> Vec<EvalResult> {
    let mut results = Vec::new();

    // ── 1. Outline ──────────────────────────────────────────────────
    let outline_result = eval_outline(backend, config).await;
    let outline_passed = outline_result.passed;
    results.push(outline_result);

    // ── 2. Worldbuilding ────────────────────────────────────────────
    // Use outline output as context (or a fallback)
    let outline_text = if outline_passed {
        // Extract what we can from the output (clone to avoid borrow issues)
        results[0].output.clone()
    } else {
        "[Outline generation failed — using fallback]".to_string()
    };

    let wiki_result = eval_worldbuilding(backend, config, &outline_text).await;
    results.push(wiki_result);

    // ── 3. Chapter Write (3 chapters) ───────────────────────────────
    let outline_for_chapters = if outline_passed {
        results[0].output.clone()
    } else {
        "A story about a lighthouse keeper.".to_string()
    };

    let wiki_for_chapters = results[1].output.clone();
    let mut chapter_texts: Vec<String> = Vec::new();

    for i in 1..=3 {
        let chapter_label = format!("Chapter {i}");
        let previous = chapter_texts.last().cloned().unwrap_or_default();

        let ch_result = eval_chapter_write(
            backend,
            config,
            i,
            &chapter_label,
            &outline_for_chapters,
            &wiki_for_chapters,
            &previous,
            false,
        )
        .await;

        if ch_result.passed {
            // Extract content for next chapter's context
            let content = extract_content_from_output(&ch_result.output);
            chapter_texts.push(content);
        } else {
            chapter_texts.push(String::new());
        }

        results.push(ch_result);
    }

    // ── 4. Validation (once per chapter) ────────────────────────────
    for i in 1..=3 {
        let idx = i as usize - 1;
        let ch_text = if idx < chapter_texts.len() && !chapter_texts[idx].is_empty() {
            &chapter_texts[idx]
        } else {
            "[No chapter content available for validation]"
        };

        let val_result = eval_validation(backend, config, i, ch_text).await;
        let val_passed = val_result.passed;
        results.push(val_result);

        // ── 5. Revising (if validation failed or found issues) ──────
        // Revision is triggered when validation indicates issues.
        // We test revision capability regardless of validation outcome.
        let validation_feedback = if val_passed {
            "The chapter needs improvement in pacing and description."
        } else {
            "The chapter quality check found issues that need addressing."
        };

        let rev_result = eval_revising(backend, config, i, ch_text, validation_feedback).await;

        results.push(rev_result);
    }

    // ── 6. Mode Selection ──────────────────────────────────────────
    for tc in MODE_SELECTION_CASES {
        let ms_result = eval_mode_selection(backend, tc).await;
        results.push(ms_result);
    }

    results
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

async fn do_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<String, String> {
    let resp = backend
        .complete(CompletionRequest {
            system: system.to_string(),
            prompt: prompt.to_string(),
            grammar: None,
            temperature,
            max_tokens,
            prefill: Some("{\n".into()),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("model error: {e}"))?;
    Ok(resp.text)
}

/// Extract prose content from a chapter write eval output (strip truncated display).
fn extract_content_from_output(output: &str) -> String {
    // The output field may be truncated (first 500 chars). For a real pipeline
    // evaluation, we'd store the full content. For the eval framework, we
    // return what we have.
    if output.contains("... (first 500 chars)") {
        // Remove trailing ellipsis note
        let trimmed = output.trim_end_matches("... (first 500 chars)").trim_end();
        trimmed.to_string()
    } else {
        output.to_string()
    }
}
