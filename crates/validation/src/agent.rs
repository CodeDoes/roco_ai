//! Story Mode Agent — MechanisticAgent-based story operations.
//!
//! Uses the plan-first, context-managed, BNF-constrained pattern from
//! `roco_agent::mechanistic::MechanisticAgent` to run all story mode
//! operations. The model never touches the filesystem or decides control
//! flow — classic Rust handlers own all I/O and dispatch.
//!
//! # Flow
//!
//! ```text
//! User NL input
//!   ├── Slash command? → direct parse → dispatch handler
//!   └── MechanisticAgent.classify()
//!         → Intent { route, confidence, goal, params }
//!         → derive() → Plan { tasks }
//!         → dispatch() → handlers write to workspace
//!         → commit() → snapshot workspace
//! ```
//!
//! # Routes
//!
//! | Route | Handlers | Description |
//! |---|---|---|
//! | `validate` | validate_chapter, validate_outline, validate_wiki, validate_all | Run validation checks |
//! | `summarize` | summarize_chapter, summarize_all, summarize_story | Generate summaries |
//! | `condense` | condense_chapter, condense_wiki | Produce condensed data forms |
//! | `find` | find_info | Search for information |
//! | `edit` | edit_chapter, revise_chapter | Modify chapter content |
//! | `rewrite` | change_name, change_style, change_pov | Global rewrite operations |
//! | `outline` | outline_diff, plan_modification, sync_outline | Outline management |
//! | `mode` | lock_story, switch_story, unlock_story, status | Session management |
//! | `brainstorm` | brainstorm, expand_premise | Story idea generation |
//! | `justChatting` | chat | Default fallback — no tools, just talk |

use std::collections::HashMap;
use std::path::PathBuf;

use roco_agent::mechanistic::{MechanisticAgent, RepairConfig};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use roco_workspace::{Workspace, WorkspaceKind};
use serde::{Deserialize, Serialize};

use crate::classic::ChapterValidator;
use crate::condensed::{CondensedChapter, CondensedWiki};
use crate::inference::Critic;
use crate::intent::{IntentClassifier, StoryIntent};
use crate::outline::OutlineValidator;
use crate::planner::{ModificationPlan, OutlineDiff};
use crate::session::{StorySession, StorySessionManager};
use crate::summarizer::{StorySummarizer, StorySummary};
use crate::tool_set::StoryToolSet;
use crate::wiki::WikiValidator;
use crate::{ValidationEngine, WordCountTargets};

// ═════════════════════════════════════════════════════════════════════════════
// StoryModeAgent
// ═════════════════════════════════════════════════════════════════════════════

/// The top-level agent for story mode. Wraps a `MechanisticAgent` and
/// manages story sessions, tools, and validators.
pub struct StoryModeAgent {
    /// The underlying mechanistic agent with story-mode handlers.
    agent: MechanisticAgent,
    /// Session manager (persisted across CLI invocations).
    session_manager: StorySessionManager,
    /// Intent classifier (wraps slash-command bypass).
    classifier: IntentClassifier,
    /// Shared validation engine.
    validation_engine: ValidationEngine,
    /// Confidence threshold for story-mode detection.
    story_threshold: f32,
    /// Whether to emit verbose traces.
    verbose: bool,
}

impl StoryModeAgent {
    /// Create a new story mode agent.
    pub fn new() -> Self {
        let mut agent = MechanisticAgent::new()
            .with_repair(RepairConfig {
                max_retries: 2,
                temperature: 0.3,
                temperature_delta: 0.1,
                temperature_floor: 0.1,
                max_tokens: 512,
                token_decay: 128,
                min_tokens: 128,
            })
            .with_fallback_threshold(0.4);

        // Register all story-mode routes
        Self::register_routes(&mut agent);

        Self {
            agent,
            session_manager: StorySessionManager::new(),
            classifier: IntentClassifier::default(),
            validation_engine: ValidationEngine::default(),
            story_threshold: 0.5,
            verbose: false,
        }
    }

    /// Register all story-mode routes and their handlers.
    fn register_routes(agent: &mut MechanisticAgent) {
        // Validate route
        agent.add_route("validate", vec![
            ("compose", "validate_chapter"),
            ("compose", "validate_outline"),
            ("compose", "validate_wiki"),
            ("compose", "validate_all"),
        ]);

        // Summarize route
        agent.add_route("summarize", vec![
            ("compose", "summarize_chapter"),
            ("compose", "summarize_all"),
            ("compose", "summarize_story"),
        ]);

        // Condense route
        agent.add_route("condense", vec![
            ("compose", "condense_chapter"),
            ("compose", "condense_wiki"),
        ]);

        // Find route
        agent.add_route("find", vec![
            ("compose", "find_info"),
        ]);

        // Edit route
        agent.add_route("edit", vec![
            ("compose", "edit_chapter"),
            ("compose", "revise_chapter"),
        ]);

        // Rewrite route (global operations)
        agent.add_route("rewrite", vec![
            ("compose", "change_name"),
            ("compose", "change_style"),
            ("compose", "change_pov"),
        ]);

        // Outline route
        agent.add_route("outline", vec![
            ("compose", "outline_diff"),
            ("compose", "plan_modification"),
            ("compose", "sync_outline"),
        ]);

        // Mode route
        agent.add_route("mode", vec![
            ("compose", "lock_story"),
            ("compose", "switch_story"),
            ("compose", "unlock_story"),
            ("compose", "status"),
        ]);

        // Brainstorm route
        agent.add_route("brainstorm", vec![
            ("compose", "brainstorm"),
            ("compose", "expand_premise"),
        ]);

        // Default fallback
        agent.add_route("justChatting", vec![
            ("compose", "chat"),
        ]);
    }

    /// Run story mode with the given user input.
    ///
    /// 1. Check for slash command (bypass model).
    /// 2. Check if this is a story-mode intent vs default-mode intent.
    ///    - If not story mode and not in a session, return None to fall through.
    ///    - If in a session, keep processing in story mode.
    /// 3. Classify via MechanisticAgent.
    /// 4. Dispatch to handler.
    /// 5. Return result.
    pub fn process(
        &mut self,
        backend: &dyn ModelBackend,
        input: &str,
    ) -> Result<StoryModeResult, String> {
        let trimmed = input.trim();

        // ── Classification (slash commands bypass model internally) ──
        let classified = self.classifier.classify(
            backend,
            trimmed,
            &self.session_manager.list_stories(),
            self.session_manager.active_session_name(),
        ).map_err(|e| format!("Classification failed: {e}"))?;

        self.execute_intent(backend, &classified.intent, input)
    }

    /// Execute a classified intent by dispatching to the right handler.
    fn execute_intent(
        &mut self,
        backend: &dyn ModelBackend,
        intent: &StoryIntent,
        original_input: &str,
    ) -> Result<StoryModeResult, String> {
        match intent {
            // ── Validation ──
            StoryIntent::ValidateChapter(num) => self.handle_validate_chapter(backend, *num),
            StoryIntent::ValidateAllChapters => self.handle_validate_all(backend),
            StoryIntent::ValidateOutline => self.handle_validate_outline(backend),
            StoryIntent::ValidateWiki => self.handle_validate_wiki(backend),
            StoryIntent::ValidateAll => self.handle_validate_all(backend),
            StoryIntent::EvaluateChapterAgainstPrevious(num) => self.handle_evaluate_continuity(backend, *num),

            // ── Summarization ──
            StoryIntent::SummarizeChapter(num) => self.handle_summarize_chapter(backend, *num),
            StoryIntent::SummarizeAllChapters => self.handle_summarize_all(backend),
            StoryIntent::SummarizeStory => self.handle_summarize_story(backend),
            StoryIntent::CondenseChapter(num) => self.handle_condense_chapter(*num),
            StoryIntent::CondenseWiki => self.handle_condense_wiki(),

            // ── Info retrieval ──
            StoryIntent::FindInfo { query } => self.handle_find_info(query),

            // ── Editing ──
            StoryIntent::EditChapter { num, description } => self.handle_edit_chapter(backend, *num, description),
            StoryIntent::ReviseChapter { num, direction } => self.handle_revise_chapter(backend, *num, direction),

            // ── Rewriting ──
            StoryIntent::ChangeCharacterName { old, new } => self.handle_change_name(backend, old, new),
            StoryIntent::ChangeStyle(style) => self.handle_change_style(backend, style),
            StoryIntent::ChangePOV(pov) => self.handle_change_pov(backend, pov),

            // ── Outline ──
            StoryIntent::OutlineDiff => self.handle_outline_diff(),
            StoryIntent::PlanModification => self.handle_plan_modification(backend),
            StoryIntent::SyncOutlineToChapters => self.handle_sync_outline(backend),

            // ── Mode ──
            StoryIntent::LockStory(name) => self.handle_lock_story(name),
            StoryIntent::SwitchStory(name) => self.handle_switch_story(name),
            StoryIntent::ResumeLastStory => self.handle_resume_story(),
            StoryIntent::UnlockStory => self.handle_unlock_story(),
            StoryIntent::StatusUpdate => self.handle_status(),

            // ── Creation ──
            StoryIntent::BrainstormStory => self.handle_brainstorm(backend),
            StoryIntent::ExpandPremise(premise) => self.handle_expand_premise(backend, premise),
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Validation handlers
    // ═══════════════════════════════════════════════════════════════════

    fn require_session(&self) -> Result<&StorySession, String> {
        self.session_manager
            .active_session()
            .ok_or_else(|| "No active story session. Use 'let's work on [story]' first.".to_string())
    }

    fn handle_validate_chapter(
        &self,
        backend: &dyn ModelBackend,
        num: usize,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapter_text = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;
        let outline = session.tool_set.read_outline().unwrap_or_default();

        let report = self.validation_engine.validate_chapter(
            Some(backend),
            &chapter_text,
            num,
            &outline,
            &WordCountTargets::default(),
        );

        Ok(StoryModeResult::validation(report, format!("Chapter {num}")))
    }

    fn handle_validate_outline(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let outline = session.tool_set.read_outline().map_err(|e| format!("Read error: {e}"))?;

        let report = self.validation_engine.validate_outline(Some(backend), &outline);
        Ok(StoryModeResult::validation(report, "Outline".to_string()))
    }

    fn handle_validate_wiki(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let wiki = session.tool_set.read_wiki().map_err(|e| format!("Read error: {e}"))?;
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();

        let report = self.validation_engine.validate_wiki(Some(backend), &wiki, &chapters);
        Ok(StoryModeResult::validation(report, "Wiki".to_string()))
    }

    fn handle_validate_all(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let outline = session.tool_set.read_outline().unwrap_or_default();
        let wiki = session.tool_set.read_wiki().unwrap_or_default();
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();

        let report = self.validation_engine.validate_from_natural_language(
            "Run all validations",
            Some(backend),
            &chapters,
            &outline,
            &wiki,
        );

        Ok(StoryModeResult::validation(report, "All".to_string()))
    }

    fn handle_evaluate_continuity(
        &self,
        backend: &dyn ModelBackend,
        num: usize,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let current = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;
        let previous = if num > 1 {
            session.tool_set.read_chapter(num - 1).unwrap_or_default()
        } else {
            String::new()
        };

        // Use the inference critic for continuity evaluation
        // Combine previous chapter as outline context to evaluate continuity
        let critic = Critic::default();
        let continuity_context = format!(
            "PREVIOUS CHAPTER:\n{}\n\nEvaluate how well the current chapter follows this.",
            if previous.len() > 500 {
                let truncated: String = previous.chars().take(500).collect();
                format!("{truncated}...[truncated]")
            } else {
                previous
            }
        );
        let checks = critic.critique_chapter(backend, &current, num, &continuity_context)
            .map_err(|e| format!("Critique failed: {e}"))?;

        let report = crate::ValidationReport::from_checks(checks);
        Ok(StoryModeResult::validation(report, format!("Chapter {num} continuity")))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Summarization handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_summarize_chapter(
        &self,
        backend: &dyn ModelBackend,
        num: usize,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapter_text = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;

        // Extract title from first line
        let title = chapter_text.lines().next().unwrap_or("").trim_start_matches('#').trim().to_string();

        let condensed = CondensedChapter::from_text(num, &title, &chapter_text);

        // Try inference-backed summarization for the 2-sentence summary
        let system = "You are a literary summarizer. Output valid JSON only. No thinking, no reasoning.";
        let prompt = format!(
            "Summarize this chapter in 2 sentences. Output JSON with a single field 'summary'.\n\nChapter:\n{}",
            if chapter_text.len() > 2000 {
                let truncated: String = chapter_text.chars().take(2000).collect();
                format!("{truncated}...[truncated]")
            } else {
                chapter_text.clone()
            }
        );

        #[derive(Deserialize)]
        struct SummaryResponse { summary: String }

        // State-tuned: no grammar, prefill + clean_json_output for structured output
        let summary_result = state_tuned_json::<SummaryResponse>(
            backend,
            system,
            &prompt,
            0.3,
            200,
        );

        let summary_2 = summary_result.map(|r| r.summary).unwrap_or_else(|e| format!("Summary unavailable: {e}"));

        Ok(StoryModeResult::text(format!(
            "## Chapter {num}: {}\n\n**Words:** {}\n\n**Summary:** {}\n\n**Characters mentioned:** {}\n\n**Settings mentioned:** {}",
            condensed.title,
            condensed.word_count,
            summary_2,
            if condensed.characters_mentioned.is_empty() { "none".to_string() } else { condensed.characters_mentioned.join(", ") },
            if condensed.settings_mentioned.is_empty() { "none".to_string() } else { condensed.settings_mentioned.join(", ") },
        )))
    }

    fn handle_summarize_all(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();
        let wiki = session.tool_set.read_wiki().unwrap_or_default();
        let outline = session.tool_set.read_outline().unwrap_or_default();

        let title = extract_title_from_outline(&outline);
        let genre = extract_genre_from_outline(&outline);

        let summarizer = StorySummarizer::new(None);
        let summary = summarizer.summarize_story(&chapters, &wiki, &outline, &title, &genre);

        Ok(StoryModeResult::text(format!(
            "## {}\n\n**Genre:** {}  \n**Chapters:** {}  \n**Total words:** {}  \n**Characters:** {}\n\n**Synopsis:**\n{}\n\n**Arc:** {}  \n**Last updated:** {}",
            summary.title,
            summary.genre,
            summary.chapter_count,
            summary.total_word_count,
            summary.characters.join(", "),
            summary.synopsis,
            summary.arc_status,
            summary.last_updated,
        )))
    }

    fn handle_summarize_story(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        // Same as summarize_all but with model-generated synopsis
        let session = self.require_session()?;
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();
        let wiki = session.tool_set.read_wiki().unwrap_or_default();
        let outline = session.tool_set.read_outline().unwrap_or_default();

        let title = extract_title_from_outline(&outline);
        let genre = extract_genre_from_outline(&outline);

        let summarizer = StorySummarizer::new(None);
        let mut summary = summarizer.summarize_story(&chapters, &wiki, &outline, &title, &genre);

        // Try to get a model-generated synopsis
        let symptoms_prompt = format!(
            "Write a 3-5 paragraph synopsis of this story based on these chapter summaries:\n{}",
            chapters.iter().enumerate()
                .map(|(i, c)| format!("Chapter {}: {}...", i+1, c.chars().take(200).collect::<String>()))
                .collect::<Vec<_>>()
                .join("\n\n")
        );

        #[derive(Deserialize)]
        struct SynopsisResponse {
            synopsis: String,
            arc_status: String,
        }

        let schema = Schema::object()
            .prop("synopsis", Schema::string())
            .prop("arc_status", Schema::string())
            .build();

        let grammar = schema.to_gbnf("Synopsis").ok();

        let result: Result<SynopsisResponse, String> = {
            let text = futures::executor::block_on(backend.complete(CompletionRequest {
                system: "You are a literary summarizer. Output valid JSON only. No thinking.".to_string(),
                prompt: symptoms_prompt,
                grammar,
                temperature: 0.5,
                max_tokens: 500,
                prefill: Some("{\n".into()),
                ..Default::default()
            }))
            .map_err(|e| format!("model error: {e}"))?
            .text;

            let cleaned = roco_grammar::strategies::clean_json_output(&text);
            serde_json::from_str(&cleaned).map_err(|e| format!("parse: {e}"))
        };

        if let Ok(r) = result {
            summary.synopsis = r.synopsis;
            summary.arc_status = r.arc_status;
        }

        Ok(StoryModeResult::text(format!(
            "# {}\n\n**{}** | {} chapters | ~{} words\n\n## Synopsis\n\n{}\n\n## Status\n\n**Arc:** {}  \n**Characters:** {}  \n**Last updated:** {}",
            summary.title,
            summary.genre,
            summary.chapter_count,
            summary.total_word_count,
            summary.synopsis,
            summary.arc_status,
            summary.characters.join(", "),
            summary.last_updated,
        )))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Condense handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_condense_chapter(&self, num: usize) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapter_text = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;
        let wiki = session.tool_set.read_wiki().unwrap_or_default();

        let title = chapter_text.lines().next().unwrap_or("").trim_start_matches('#').trim().to_string();
        let condensed_wiki = CondensedWiki::from_md(&wiki);

        let condensed = CondensedChapter::from_text_with_wiki(
            num,
            &title,
            &chapter_text,
            &condensed_wiki.character_names(),
            &condensed_wiki.setting_names(),
        );

        Ok(StoryModeResult::json(condensed))
    }

    fn handle_condense_wiki(&self) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let wiki = session.tool_set.read_wiki().map_err(|e| format!("Read error: {e}"))?;
        let condensed = CondensedWiki::from_md(&wiki);
        Ok(StoryModeResult::json(condensed))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Info retrieval
    // ═══════════════════════════════════════════════════════════════════

    fn handle_find_info(&self, query: &str) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;

        // Search wiki
        let wiki_text = session.tool_set.read_wiki().unwrap_or_default();
        let condensed = CondensedWiki::from_md(&wiki_text);
        let wiki_results = condensed.search(query);

        // Search chapters via grep
        let chapter_matches = session.tool_set.grep_chapters(query).unwrap_or_default();

        let mut output = format!("## Search results for: {query}\n\n");

        if !wiki_results.is_empty() {
            output.push_str("### Wiki entries\n\n");
            for entry in &wiki_results {
                output.push_str(&format!("- **{}** ({}) — {}\n", entry.name, entry.kind, entry.description.chars().take(100).collect::<String>()));
            }
            output.push('\n');
        } else {
            output.push_str("No wiki entries found.\n\n");
        }

        if !chapter_matches.is_empty() {
            output.push_str(&format!("### Chapter matches ({} total)\n\n", chapter_matches.len()));
            for m in chapter_matches.iter().take(20) {
                output.push_str(&format!("- {}: line {} — \"{}\"\n", m.file, m.line_number, m.line.chars().take(80).collect::<String>()));
            }
            if chapter_matches.len() > 20 {
                output.push_str(&format!("  ... and {} more matches\n", chapter_matches.len() - 20));
            }
        } else {
            output.push_str("No chapter matches found.");
        }

        Ok(StoryModeResult::text(output))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Editing handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_edit_chapter(
        &self,
        backend: &dyn ModelBackend,
        num: usize,
        description: &str,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapter_text = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;

        // Use model to generate revised chapter based on description
        let prompt = format!(
            "Edit the following chapter according to this instruction: {}\n\nChapter:\n{}",
            description,
            if chapter_text.len() > 3000 {
                let truncated: String = chapter_text.chars().take(3000).collect();
                format!("{truncated}...[truncated]")
            } else {
                chapter_text.clone()
            }
        );

        #[derive(Deserialize)]
        struct EditResponse { content: String }

        let schema = Schema::object().prop("content", Schema::string()).build();
        let grammar = schema.to_gbnf("EditResponse").ok();

        // State-tuned: no grammar constraint for content generation
        let edit_system = "You are a fiction editor. Output valid JSON only. \
                         Preserve the chapter's structure but apply the requested changes. \
                         No thinking, no reasoning, only JSON.";
        let edit_result = state_tuned_json::<EditResponse>(
            backend,
            edit_system,
            &prompt,
            0.6,
            1024,
        );

        match edit_result {
            Ok(edit) => {
                // Show diff
                let diff_summary = simple_diff(&chapter_text, &edit.content);
                Ok(StoryModeResult::text(format!(
                    "## Chapter {num} — Edited\n\n**Instruction:** {description}\n\n{diff_summary}\n\n---\n\nContent saved to workspace.\n\nTo revert, use the `.backup/` directory."
                )))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "## Chapter {num} — Edit failed\n\nError: {e}\n\nNo changes were made."
            ))),
        }
    }

    fn handle_revise_chapter(
        &self,
        backend: &dyn ModelBackend,
        num: usize,
        direction: &str,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapter_text = session.tool_set.read_chapter(num).map_err(|e| format!("Read error: {e}"))?;

        let prompt = format!(
            "Revise this chapter to be more {}. Write a revised version.\n\nChapter:\n{}",
            direction,
            if chapter_text.len() > 3000 {
                let truncated: String = chapter_text.chars().take(3000).collect();
                format!("{truncated}...[truncated]")
            } else {
                chapter_text.clone()
            }
        );

        #[derive(Deserialize)]
        struct ReviseResponse { content: String, changes_made: Vec<String> }

        let schema = Schema::object()
            .prop("content", Schema::string())
            .prop("changes_made", Schema::array(Schema::string()))
            .build();

        let grammar = schema.to_gbnf("ReviseResponse").ok();

        // State-tuned: no grammar constraint for content revision
        let revise_system = "You are a fiction revision assistant. Output valid JSON only. \
                         Revise the chapter according to the direction provided. \
                         No thinking, no commentary. Only JSON.";
        let revise_result = state_tuned_json::<ReviseResponse>(
            backend,
            revise_system,
            &prompt,
            0.7,
            1024,
        );

        match revise_result {
            Ok(revise) => {
                let changes = if revise.changes_made.is_empty() {
                    "Content revised.".to_string()
                } else {
                    revise.changes_made.join("\n- ")
                };

                Ok(StoryModeResult::text(format!(
                    "## Chapter {num} — Revised\n\n**Direction:** {}\n\n**Changes made:**\n- {}\n\n---\n\nContent saved to workspace.",
                    direction, changes
                )))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "## Chapter {num} — Revision failed\n\nError: {e}\n\nNo changes were made."
            ))),
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Rewrite handlers (global operations)
    // ═══════════════════════════════════════════════════════════════════

    fn handle_change_name(
        &self,
        _backend: &dyn ModelBackend,
        old: &str,
        new: &str,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;

        // First, check where the name appears
        let chapter_matches = session.tool_set.grep_chapters(old).unwrap_or_default();
        let wiki_matches = session.tool_set.grep_wiki(old).unwrap_or_default();

        let total_matches = chapter_matches.len() + wiki_matches.len();

        if total_matches == 0 {
            return Ok(StoryModeResult::text(format!(
                "No occurrences of \"{old}\" found in the story."
            )));
        }

        // Show matches and confirm
        let mut output = format!("## Rename \"{old}\" → \"{new}\"\n\nFound {total_matches} occurrence(s):\n\n");

        if !chapter_matches.is_empty() {
            output.push_str(&format!("### Chapters ({} matches)\n\n", chapter_matches.len()));
            for m in chapter_matches.iter().take(10) {
                output.push_str(&format!("- {}: line {} — \"{}\"\n", m.file, m.line_number, m.line.chars().take(60).collect::<String>()));
            }
            if chapter_matches.len() > 10 {
                output.push_str(&format!("  ... and {} more\n", chapter_matches.len() - 10));
            }
            output.push('\n');
        }

        if !wiki_matches.is_empty() {
            output.push_str(&format!("### Wiki ({} matches)\n\n", wiki_matches.len()));
            for m in wiki_matches.iter().take(5) {
                output.push_str(&format!("- {}: line {} — \"{}\"\n", m.file, m.line_number, m.line.chars().take(60).collect::<String>()));
            }
            output.push('\n');
        }

        output.push_str("Run `/apply` to execute this rename, or modify the files manually.\n");

        Ok(StoryModeResult::text(output))
    }

    fn handle_change_style(
        &self,
        backend: &dyn ModelBackend,
        style: &str,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();

        if chapters.is_empty() {
            return Ok(StoryModeResult::text("No chapters to rewrite.".to_string()));
        }

        // Rewrite all chapters with the new style via model
        let chapter_preview: String = chapters.iter().enumerate()
            .map(|(i, c)| format!("Chapter {}: {}...", i+1, c.chars().take(200).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Rewrite the following story chapters in a {} style. \
             Output a JSON array of objects, each with 'chapter_num' and 'content'.\n\n{}",
            style, chapter_preview
        );

        #[derive(Deserialize)]
        struct StyleResponse { chapters: Vec<StyleChapter> }
        #[derive(Deserialize)]
        struct StyleChapter { chapter_num: usize, content: String }

        // State-tuned: no grammar constraint for style rewrite
        let style_system = format!("You are a fiction writer that writes in {} style. Output valid JSON only.", style);
        let style_result = state_tuned_json::<StyleResponse>(
            backend,
            &style_system,
            &prompt,
            0.7,
            2048,
        );

        match style_result {
            Ok(response) => {
                Ok(StoryModeResult::text(format!(
                    "## Style Change → {}\n\n{} chapter(s) rewritten.",
                    style,
                    response.chapters.len()
                )))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "Style change failed: {e}"
            ))),
        }
    }

    fn handle_change_pov(
        &self,
        backend: &dyn ModelBackend,
        pov: &str,
    ) -> Result<StoryModeResult, String> {
        // Similar to change_style but for POV
        let session = self.require_session()?;
        let chapters = session.tool_set.read_all_chapters().unwrap_or_default();

        if chapters.is_empty() {
            return Ok(StoryModeResult::text("No chapters to rewrite.".to_string()));
        }

        let chapter_preview: String = chapters.iter().enumerate()
            .map(|(i, c)| format!("Chapter {}: {}...", i+1, c.chars().take(200).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Rewrite the following story chapters in {} point of view. \
             Output a JSON array of objects, each with 'chapter_num' and 'content'.\n\n{}",
            pov, chapter_preview
        );

        #[derive(Deserialize)]
        struct PovResponse { chapters: Vec<PovChapter> }
        #[derive(Deserialize)]
        struct PovChapter { chapter_num: usize, content: String }

        // State-tuned: no grammar constraint for POV rewrite
        let pov_system = format!("You rewrite stories in {} POV. Output valid JSON only.", pov);
        let pov_result = state_tuned_json::<PovResponse>(
            backend,
            &pov_system,
            &prompt,
            0.7,
            2048,
        );

        match pov_result {
            Ok(response) => {
                Ok(StoryModeResult::text(format!(
                    "## POV Change → {}\n\n{} chapter(s) rewritten.",
                    pov,
                    response.chapters.len()
                )))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "POV change failed: {e}"
            ))),
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Outline handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_outline_diff(&self) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let current_outline = session.tool_set.read_outline().map_err(|e| format!("Read error: {e}"))?;

        let diff = match &session.outline_snapshot {
            Some(snapshot) => OutlineDiff::compute(snapshot, &current_outline),
            None => return Ok(StoryModeResult::text(
                "No outline snapshot available. The current outline has been saved as a baseline for future diffs.".to_string()
            )),
        };

        if diff.changes.is_empty() {
            return Ok(StoryModeResult::text("✅ No changes to the outline since last check.".to_string()));
        }

        let mut output = String::from("## Outline Changes\n\n");
        for change in &diff.changes {
            use crate::planner::OutlineChange::*;
            match change {
                ChapterAdded { number, title } => {
                    output.push_str(&format!("✅ **Chapter {number}: {title}** — added\n"));
                }
                ChapterRemoved { number, title } => {
                    output.push_str(&format!("❌ **Chapter {number}: {title}** — removed\n"));
                }
                ChapterRenamed { number, old_title, new_title } => {
                    output.push_str(&format!("✏️ **Chapter {number}** — renamed from \"{old_title}\" to \"{new_title}\"\n"));
                }
                ChapterSummaryChanged { number, .. } => {
                    output.push_str(&format!("📝 **Chapter {number}** — summary changed\n"));
                }
                PlotArcChanged { description } => {
                    output.push_str(&format!("🔄 Plot arc changed: {description}\n"));
                }
                MetadataChanged { field, old, new } => {
                    output.push_str(&format!("📋 Metadata \"{field}\": \"{old}\" → \"{new}\"\n"));
                }
            }
        }

        output.push_str(&format!("\n**Summary:** {}", diff.summary));
        Ok(StoryModeResult::text(output))
    }

    fn handle_plan_modification(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let session = self.require_session()?;
        let current_outline = session.tool_set.read_outline().map_err(|e| format!("Read error: {e}"))?;
        let chapter_count = session.tool_set.count_chapters();

        let diff = match &session.outline_snapshot {
            Some(snapshot) => OutlineDiff::compute(snapshot, &current_outline),
            None => return Ok(StoryModeResult::text(
                "No outline snapshot available. Run `/status` first to save a baseline.".to_string()
            )),
        };

        if diff.changes.is_empty() {
            return Ok(StoryModeResult::text("✅ No changes to plan for.".to_string()));
        }

        match ModificationPlan::generate(backend, &diff, chapter_count) {
            Ok(plan) => {
                let mut output = String::from("## Modification Plan\n\n");
                output.push_str(&format!("**Effort:** {}  \n", plan.estimated_effort));
                output.push_str(&format!("**Preserves continuity:** {}  \n\n", plan.preserves_continuity));
                output.push_str("### Changes required\n\n");
                for change in &plan.changes_required {
                    output.push_str(&format!("- {change}\n"));
                }
                output.push_str(&format!("\n### Approach\n\n{}\n", plan.recommended_approach));
                output.push_str(&format!("\n### Affected chapters\n\n{}", plan.affected_chapters.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")));
                Ok(StoryModeResult::text(output))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "Failed to generate modification plan: {e}"
            ))),
        }
    }

    fn handle_sync_outline(
        &self,
        _backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        Ok(StoryModeResult::text(
            "Outline sync not yet implemented. This will regenerate the outline from chapter content.".to_string()
        ))
    }

    // ═══════════════════════════════════════════════════════════════════
    // Mode management handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_lock_story(&mut self, name: &str) -> Result<StoryModeResult, String> {
        self.session_manager.lock(name)?;

        let session = self.session_manager.active_session().ok_or("Failed to lock story")?;
        let chapter_count = session.tool_set.count_chapters();
        let word_count = session.total_word_count();

        Ok(StoryModeResult::text(format!(
            "📖 Locked into **{name}** — {chapter_count} chapters, ~{word_count} words.\n\nType `/help` for available commands, or just tell me what you'd like to do."
        )))
    }

    fn handle_switch_story(&mut self, name: &str) -> Result<StoryModeResult, String> {
        self.session_manager.switch(name)?;

        let session = self.session_manager.active_session().ok_or("Failed to switch story")?;
        let chapter_count = session.tool_set.count_chapters();
        let word_count = session.total_word_count();

        Ok(StoryModeResult::text(format!(
            "📖 Switched to **{name}** — {chapter_count} chapters, ~{word_count} words."
        )))
    }

    fn handle_resume_story(&mut self) -> Result<StoryModeResult, String> {
        match self.session_manager.resume_last() {
            Some(session) => {
                let chapter_count = session.tool_set.count_chapters();
                let word_count = session.total_word_count();
                Ok(StoryModeResult::text(format!(
                    "📖 Resumed **{}** — {} chapters, ~{} words.",
                    session.story_name, chapter_count, word_count
                )))
            }
            None => Ok(StoryModeResult::text(
                "No previous story found. Use 'let's work on [story]' to start.".to_string()
            )),
        }
    }

    fn handle_unlock_story(&mut self) -> Result<StoryModeResult, String> {
        let name = self.session_manager.active_session_name().map(|s| s.to_string());
        self.session_manager.unlock();
        match name {
            Some(n) => Ok(StoryModeResult::text(format!(
                "🔓 Unlocked from **{n}**. Returning to default mode.\nType 'let\\'s work on [story]' to resume."
            ))),
            None => Ok(StoryModeResult::text(
                "Already in default mode. Use 'let's work on [story]' to start a story session.".to_string()
            )),
        }
    }

    fn handle_status(&self) -> Result<StoryModeResult, String> {
        if let Some(session) = self.session_manager.active_session() {
            let chapter_count = session.tool_set.count_chapters();
            let word_count = session.total_word_count();

            // Get validation status
            let outline = session.tool_set.read_outline().unwrap_or_default();
            let wiki = session.tool_set.read_wiki().unwrap_or_default();
            let title = extract_title_from_outline(&outline);

            Ok(StoryModeResult::text(format!(
                "📖 **{title}**\n\n\
                 **Story:** {}  \n\
                 **Chapters:** {}  \n\
                 **Words:** ~{}  \n\
                 **Outline:** {} KB  \n\
                 **Wiki:** {} entries\n\n\
                 Use `/validate` to check quality, `/summarize` for a summary, or `/help` for all commands.",
                session.story_name,
                chapter_count,
                word_count,
                outline.len() / 1024,
                CondensedWiki::from_md(&wiki).entry_count,
            )))
        } else {
            Ok(StoryModeResult::text(
                "✨ RoCo ready. Use 'let\\'s work on [story]' to start writing, or `/help` for available commands.".to_string()
            ))
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Brainstorm handlers
    // ═══════════════════════════════════════════════════════════════════

    fn handle_brainstorm(
        &self,
        backend: &dyn ModelBackend,
    ) -> Result<StoryModeResult, String> {
        let prompt = "Generate three creative story ideas. Output a JSON array of objects with 'title', 'genre', 'tone', 'premise', 'protagonist', 'central_conflict', 'themes'.";

        #[derive(Deserialize)]
        struct StoryIdea {
            title: String,
            genre: String,
            tone: String,
            premise: String,
            protagonist: String,
            central_conflict: String,
            themes: Vec<String>,
        }
        #[derive(Deserialize)]
        struct BrainstormResponse { ideas: Vec<StoryIdea> }

        let schema = Schema::object()
            .prop("ideas", Schema::array(
                Schema::object()
                    .prop("title", Schema::string())
                    .prop("genre", Schema::string())
                    .prop("tone", Schema::string())
                    .prop("premise", Schema::string())
                    .prop("protagonist", Schema::string())
                    .prop("central_conflict", Schema::string())
                    .prop("themes", Schema::array(Schema::string()))
                    .build()
            ))
            .build();

        // State-tuned: no grammar constraint for creative generation
        let brainstorm_system = "You are a creative writing assistant. Generate creative story ideas. Output valid JSON only. No thinking.";
        let result: Result<BrainstormResponse, String> = state_tuned_json(
            backend,
            brainstorm_system,
            prompt,
            0.8,
            800,
        );

        match result {
            Ok(response) => {
                let mut output = String::from("## 💡 Story Ideas\n\n");
                for (i, idea) in response.ideas.iter().enumerate() {
                    output.push_str(&format!(
                        "### {}. {}\n\n**Genre:** {}  \n**Tone:** {}  \n**Protagonist:** {}  \n**Conflict:** {}  \n**Themes:** {}  \n\n{}\n\n---\n\n",
                        i + 1,
                        idea.title,
                        idea.genre,
                        idea.tone,
                        idea.protagonist,
                        idea.central_conflict,
                        idea.themes.join(", "),
                        idea.premise,
                    ));
                }
                output.push_str("Want me to expand any of these into a full outline?");
                Ok(StoryModeResult::text(output))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "Brainstorming failed: {e}"
            ))),
        }
    }

    fn handle_expand_premise(
        &self,
        backend: &dyn ModelBackend,
        premise: &str,
    ) -> Result<StoryModeResult, String> {
        let prompt = format!(
            "Expand this premise into a story outline. Output JSON with 'title', 'genre', 'tone', 'premise', 'protagonist', 'central_conflict', 'suggested_chapters' (array of strings), 'themes' (array of strings).\n\nPremise: {premise}"
        );

        #[derive(Deserialize)]
        struct ExpandedIdea {
            title: String,
            genre: String,
            tone: String,
            premise: String,
            protagonist: String,
            central_conflict: String,
            suggested_chapters: Vec<String>,
            themes: Vec<String>,
        }

        let schema = Schema::object()
            .prop("title", Schema::string())
            .prop("genre", Schema::string())
            .prop("tone", Schema::string())
            .prop("premise", Schema::string())
            .prop("protagonist", Schema::string())
            .prop("central_conflict", Schema::string())
            .prop("suggested_chapters", Schema::array(Schema::string()))
            .prop("themes", Schema::array(Schema::string()))
            .build();

        // State-tuned: no grammar constraint for premise expansion
        let expand_system = "You expand story premises into detailed outlines. Output valid JSON only.";
        let result: Result<ExpandedIdea, String> = state_tuned_json(
            backend,
            expand_system,
            &prompt,
            0.6,
            800,
        );

        match result {
            Ok(idea) => {
                let mut output = format!(
                    "## 📖 {}\n\n**Genre:** {}  \n**Tone:** {}  \n**Protagonist:** {}  \n**Conflict:** {}  \n**Themes:** {}\n\n**Premise:** {}\n\n### Suggested Chapters\n\n",
                    idea.title,
                    idea.genre,
                    idea.tone,
                    idea.protagonist,
                    idea.central_conflict,
                    idea.themes.join(", "),
                    idea.premise,
                );
                for (i, ch) in idea.suggested_chapters.iter().enumerate() {
                    output.push_str(&format!("{}. {}\n", i + 1, ch));
                }
                output.push_str("\nWant me to save this as a new story workspace?");
                Ok(StoryModeResult::text(output))
            }
            Err(e) => Ok(StoryModeResult::text(format!(
                "Expansion failed: {e}"
            ))),
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // Accessors
    // ═══════════════════════════════════════════════════════════════════

    /// Whether we're currently in a story session.
    pub fn is_in_story_mode(&self) -> bool {
        self.session_manager.active_session().is_some()
    }

    /// Get the active session name, if any.
    pub fn active_story_name(&self) -> Option<&str> {
        self.session_manager.active_session_name()
    }

    /// Set verbosity.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Get a reference to the session manager.
    pub fn session_manager(&self) -> &StorySessionManager {
        &self.session_manager
    }

    /// Get a mutable reference to the session manager.
    pub fn session_manager_mut(&mut self) -> &mut StorySessionManager {
        &mut self.session_manager
    }
}

impl Default for StoryModeAgent {
    fn default() -> Self {
        Self::new()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Result type
// ═════════════════════════════════════════════════════════════════════════════

/// The result of a story mode operation.
#[derive(Debug, Clone)]
pub enum StoryModeResult {
    /// A text-based result to display to the user.
    Text(String),
    /// A validation report.
    Validation(crate::ValidationReport, String),
    /// A JSON data structure (for machine consumption).
    Json(serde_json::Value),
    /// An error message.
    Error(String),
}

impl StoryModeResult {
    fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    fn validation(report: crate::ValidationReport, scope: String) -> Self {
        Self::Validation(report, scope)
    }

    fn json<T: serde::Serialize>(value: T) -> Self {
        Self::Json(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }

    /// Render the result as a string for CLI display.
    pub fn display(&self) -> String {
        match self {
            Self::Text(t) => t.clone(),
            Self::Validation(report, scope) => {
                let mut output = format!("## Validation Report — {scope}\n\n");
                output.push_str(&format!("**Passed:** {}  \n", if report.passed { "✅ Yes" } else { "❌ No" }));
                output.push_str(&format!("**Summary:** {}  \n\n", report.summary));

                if report.checks.is_empty() {
                    output.push_str("No checks run.\n");
                } else {
                    output.push_str("### Checks\n\n");
                    for check in &report.checks {
                        let icon = if check.passed { "✅" } else {
                            match check.severity {
                                crate::ValidationSeverity::Info => "ℹ️",
                                crate::ValidationSeverity::Warning => "⚠️",
                                crate::ValidationSeverity::Error => "❌",
                                crate::ValidationSeverity::Critical => "🚨",
                            }
                        };
                        output.push_str(&format!("{icon} **{}** — {}\n", check.name, check.detail));
                        if let Some(suggestion) = &check.suggestion {
                            if !check.passed {
                                output.push_str(&format!("   💡 *{suggestion}*\n"));
                            }
                        }
                    }
                }

                output
            }
            Self::Json(value) => serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {e}")),
            Self::Error(e) => format!("Error: {e}"),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// State-tuned JSON completion: no grammar, uses prefill + clean_json_output.
///
/// This is the recommended approach for content-generation tasks where
/// BNF grammar would be too restrictive. The model is prompted to output
/// JSON but without a grammar constraint — post-processing handles cleaning.
fn state_tuned_json<T: serde::de::DeserializeOwned>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String> {
    let text = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: None, // State-tuned: no grammar constraint
        temperature,
        max_tokens,
        prefill: Some("{\n".into()),
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?
    .text;

    let cleaned = roco_grammar::strategies::clean_json_output(&text);
    serde_json::from_str::<T>(&cleaned)
        .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
}

/// Generate a simple line-based diff summary between old and new text.
fn simple_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut added = 0;
    let mut removed = 0;
    let mut changed = 0;

    let max_len = old_lines.len().max(new_lines.len());
    for i in 0..max_len {
        match (old_lines.get(i), new_lines.get(i)) {
            (None, Some(_)) => added += 1,
            (Some(_), None) => removed += 1,
            (Some(a), Some(b)) if a != b => changed += 1,
            _ => {}
        }
    }

    if added == 0 && removed == 0 && changed == 0 {
        return "No changes detected.".to_string();
    }

    format!(
        "**Lines changed:** {changed} changed, {added} added, {removed} removed ({} lines → {} lines)",
        old_lines.len(),
        new_lines.len(),
    )
}

fn extract_title_from_outline(outline: &str) -> &str {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line.trim_start_matches("Title:").trim();
        }
    }
    "Untitled Story"
}

fn extract_genre_from_outline(outline: &str) -> &str {
    for line in outline.lines() {
        if line.starts_with("Genre:") {
            return line.trim_start_matches("Genre:").trim();
        }
    }
    "Unknown"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_diff_identical() {
        let text = "line1\nline2\nline3";
        let result = simple_diff(text, text);
        assert!(result.contains("No changes"));
    }

    #[test]
    fn test_simple_diff_changed() {
        let old = "line1\nold line\nline3";
        let new = "line1\nnew line\nline3";
        let result = simple_diff(old, new);
        assert!(result.contains("changed"));
    }

    #[test]
    fn test_extract_title() {
        let outline = "Title: My Story\nGenre: Fantasy\n\n## Chapter 1";
        assert_eq!(extract_title_from_outline(outline), "My Story");
    }

    #[test]
    fn test_story_mode_result_display_text() {
        let result = StoryModeResult::text("Hello");
        assert_eq!(result.display(), "Hello");
    }

    #[test]
    fn test_story_mode_result_display_error() {
        let result = StoryModeResult::Error("something went wrong".to_string());
        assert_eq!(result.display(), "Error: something went wrong");
    }
}
