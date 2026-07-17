//! Story Engine — dynamic, interactive story generation.
//!
//! Extends the basic story pipeline with:
//! - Dynamic outline expansion (no fixed chapter limit)
//! - Plot state tracking (structured, not raw text)
//! - Context assembly (plot state + recent chapters)
//! - Chapter continuation (resume from where left off)
//! - Interactive mode (human-in-the-loop)
//!
//! # Architecture
//!
//! ```text
//! premise → outline → wiki → [chapter → plot_state → expand_outline]* → publish
//!                          ↑           ↓
//!                          └───────────┘ (dynamic loop)
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{schema_to_gbnf, Schema};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::{Deserialize, Serialize};

use super::mechanistic::{HandlerResult, MechanisticAgent, Plan, Task};
use super::quality::{QualityAnalyzer, QualityScore, StoryCritique};
use super::interaction::{InteractionMode, InteractionState, HumanAction};

// ═════════════════════════════════════════════════════════════════════════════
// Revision Record — tracks revisions made to chapters
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionRecord {
    pub chapter_num: usize,
    pub revision_num: usize,
    pub reason: String,
    pub quality_before: f32,
    pub quality_after: f32,
    pub timestamp: u64,
}

// ═════════════════════════════════════════════════════════════════════════════
// Plot State — structured representation of story state
// ═════════════════════════════════════════════════════════════════════════════

/// Structured plot state extracted after each chapter.
/// This is the key to maintaining coherence across long stories.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlotState {
    /// Current chapter number
    pub chapter_count: u64,
    /// Characters and their current states
    pub characters: Vec<CharacterState>,
    /// Active conflicts/tensions
    pub active_conflicts: Vec<String>,
    /// Resolved conflicts
    pub resolved_conflicts: Vec<String>,
    /// Planted foreshadowing (Chekhov's guns)
    pub foreshadowing: Vec<String>,
    /// Current location/setting
    pub current_location: String,
    /// Recent events (last 2-3 chapters)
    pub recent_events: Vec<String>,
    /// Recurring themes/motifs
    pub themes: Vec<String>,
    /// Story arc stage: setup | rising_action | climax | falling_action | resolution
    pub arc_stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterState {
    pub name: String,
    pub current_status: String,
    pub last_seen: String,
    pub knowledge: Vec<String>,
}

impl PlotState {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("chapter_count", Schema::integer())
            .prop("characters", Schema::array(
                Schema::object()
                    .prop("name", Schema::string())
                    .prop("current_status", Schema::string())
                    .prop("last_seen", Schema::string())
                    .prop("knowledge", Schema::array(Schema::string()))
                    .build()
            ))
            .prop("active_conflicts", Schema::array(Schema::string()))
            .prop("resolved_conflicts", Schema::array(Schema::string()))
            .prop("foreshadowing", Schema::array(Schema::string()))
            .prop("current_location", Schema::string())
            .prop("recent_events", Schema::array(Schema::string()))
            .prop("themes", Schema::array(Schema::string()))
            .prop("arc_stage", Schema::enum_values(vec![
                serde_json::json!("setup"),
                serde_json::json!("rising_action"),
                serde_json::json!("climax"),
                serde_json::json!("falling_action"),
                serde_json::json!("resolution"),
            ]))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json())
            .expect("PlotState schema is valid")
    }

    /// Create initial plot state from outline
    pub fn from_outline(outline: &str) -> Self {
        Self {
            chapter_count: 0,
            arc_stage: "setup".to_string(),
            ..Default::default()
        }
    }

    /// Merge new plot state into existing, preserving history
    pub fn merge(&mut self, new: PlotState) {
        self.chapter_count = new.chapter_count;
        self.current_location = new.current_location;
        self.arc_stage = new.arc_stage;

        // Merge characters (update existing, add new)
        for new_char in new.characters {
            if let Some(existing) = self.characters.iter_mut().find(|c| c.name == new_char.name) {
                *existing = new_char;
            } else {
                self.characters.push(new_char);
            }
        }

        // Merge conflicts
        for conflict in new.active_conflicts {
            if !self.active_conflicts.contains(&conflict) {
                self.active_conflicts.push(conflict);
            }
        }
        for conflict in new.resolved_conflicts {
            self.active_conflicts.retain(|c| c != &conflict);
            if !self.resolved_conflicts.contains(&conflict) {
                self.resolved_conflicts.push(conflict);
            }
        }

        // Merge foreshadowing
        for seed in new.foreshadowing {
            if !self.foreshadowing.contains(&seed) {
                self.foreshadowing.push(seed);
            }
        }

        // Keep recent events limited to last 3 chapters
        self.recent_events = new.recent_events;
        if self.recent_events.len() > 3 {
            self.recent_events = self.recent_events[self.recent_events.len() - 3..].to_vec();
        }

        // Merge themes
        for theme in new.themes {
            if !self.themes.contains(&theme) {
                self.themes.push(theme);
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Outline Expansion — dynamic chapter generation
// ═════════════════════════════════════════════════════════════════════════════

/// Output from outline expansion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineExpansion {
    pub new_chapters: Vec<ChapterInfo>,
    pub arc_progression: String,
    pub should_continue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub number: u64,
    pub title: String,
    pub summary: String,
}

impl OutlineExpansion {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("new_chapters", Schema::array(
                Schema::object()
                    .prop("number", Schema::integer())
                    .prop("title", Schema::string())
                    .prop("summary", Schema::string())
                    .build()
            ))
            .prop("arc_progression", Schema::string())
            .prop("should_continue", Schema::boolean())
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json())
            .expect("OutlineExpansion schema is valid")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Engine Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// Configuration for the story engine
#[derive(Debug, Clone)]
pub struct StoryConfig {
    /// Minimum chapters to generate (default: 3)
    pub min_chapters: usize,
    /// Maximum chapters (0 = unlimited)
    pub max_chapters: usize,
    /// Chapters per expansion batch
    pub expansion_batch: usize,
    /// Target words per chapter
    pub words_per_chapter: usize,
    /// Enable interactive mode
    pub interactive: bool,
    /// Interaction mode
    pub interaction_mode: InteractionMode,
    /// Enable plot state tracking
    pub track_plot_state: bool,
    /// Enable quality validation
    pub validate_quality: bool,
    /// Quality threshold for passing (0-10)
    pub quality_threshold: f32,
    /// Maximum revisions per chapter
    pub max_revisions: usize,
}

impl Default for StoryConfig {
    fn default() -> Self {
        Self {
            min_chapters: 3,
            max_chapters: 0,  // unlimited
            expansion_batch: 3,
            words_per_chapter: 400,
            interactive: false,
            interaction_mode: InteractionMode::default(),
            track_plot_state: true,
            validate_quality: true,
            quality_threshold: 6.0,
            max_revisions: 2,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Story Engine — the main orchestrator
// ═════════════════════════════════════════════════════════════════════════════

/// The story engine orchestrates dynamic story generation.
pub struct StoryEngine {
    config: StoryConfig,
    workspace: Workspace,
    plot_state: PlotState,
    outline: Vec<ChapterInfo>,
    chapters: Vec<String>,
    current_chapter: usize,
    /// Quality scores for each chapter
    chapter_scores: Vec<QualityScore>,
    /// Revision history
    revisions: Vec<RevisionRecord>,
    /// Interaction state
    interaction_state: InteractionState,
}

impl StoryEngine {
    /// Create a new story engine with the given configuration.
    pub fn new(config: StoryConfig) -> Result<Self, String> {
        let workspace = create_story_workspace()?;
        let interaction_state = InteractionState::new(
            config.interaction_mode.clone(),
            config.max_chapters,
        );
        Ok(Self {
            config,
            workspace,
            plot_state: PlotState::default(),
            outline: Vec::new(),
            chapters: Vec::new(),
            current_chapter: 0,
            chapter_scores: Vec::new(),
            revisions: Vec::new(),
            interaction_state,
        })
    }

    /// Get the workspace path
    pub fn workspace_path(&self) -> &std::path::Path {
        self.workspace.root()
    }

    /// Get current plot state
    pub fn plot_state(&self) -> &PlotState {
        &self.plot_state
    }

    /// Get current outline
    pub fn outline(&self) -> &[ChapterInfo] {
        &self.outline
    }

    /// Get generated chapters
    pub fn chapters(&self) -> &[String] {
        &self.chapters
    }

    /// Get interaction state
    pub fn interaction_state(&self) -> &InteractionState {
        &self.interaction_state
    }

    /// Get mutable interaction state
    pub fn interaction_state_mut(&mut self) -> &mut InteractionState {
        &mut self.interaction_state
    }

    /// Process a human action
    pub fn process_human_action(&mut self, action: HumanAction) {
        self.interaction_state.process_action(action);
    }

    /// Should we pause for human input?
    pub fn should_pause(&self) -> bool {
        self.interaction_state.should_pause()
    }

    /// Get the human prompt
    pub fn human_prompt(&self) -> String {
        self.interaction_state.human_prompt()
    }

    /// Generate initial outline from premise
    pub fn generate_outline(
        &mut self,
        backend: &dyn ModelBackend,
        premise: &str,
    ) -> Result<(), String> {
        let expansion: OutlineExpansion = structured_complete(
            backend,
            "You are a story outliner. Create a compelling story structure. Output valid JSON only.",
            &format!(
                "Outline a story based on this premise:\n{premise}\n\n\
                 Create {} chapters for the initial arc. Output JSON matching the schema.",
                self.config.expansion_batch
            ),
            &OutlineExpansion::grammar(),
            0.6,
            400,
        )?;

        self.outline = expansion.new_chapters;
        self.plot_state = PlotState::from_outline(premise);

        // Save outline to workspace
        let md = self.render_outline();
        let path = self.workspace.resolve("01-OUTLINE.md").unwrap();
        let _ = write_file(&path, &md);

        Ok(())
    }

    /// Expand the outline for more chapters
    pub fn expand_outline(
        &mut self,
        backend: &dyn ModelBackend,
    ) -> Result<bool, String> {
        // Check if we've hit max chapters
        if self.config.max_chapters > 0 && self.outline.len() >= self.config.max_chapters {
            return Ok(false);
        }

        let current_outline: String = self.outline.iter()
            .map(|ch| format!("Chapter {}: {} - {}", ch.number, ch.title, ch.summary))
            .collect::<Vec<_>>()
            .join("\n");

        let expansion: OutlineExpansion = structured_complete(
            backend,
            "You are a story outliner. Continue the story arc. Output valid JSON only.",
            &format!(
                "Current outline:\n{current_outline}\n\n\
                 Current plot state: {:?}\n\n\
                 What happens next? Add {} more chapters to continue the arc. \
                 Output JSON matching the schema.",
                self.plot_state,
                self.config.expansion_batch
            ),
            &OutlineExpansion::grammar(),
            0.6,
            400,
        )?;

        if !expansion.should_continue {
            return Ok(false);
        }

        // Add new chapters to outline
        let next_num = self.outline.len() as u64 + 1;
        for (i, mut ch) in expansion.new_chapters.into_iter().enumerate() {
            ch.number = next_num + i as u64;
            self.outline.push(ch);
        }

        // Update outline file
        let md = self.render_outline();
        let path = self.workspace.resolve("01-OUTLINE.md").unwrap();
        let _ = write_file(&path, &md);

        Ok(true)
    }

    /// Generate the next chapter
    pub fn generate_chapter(
        &mut self,
        backend: &dyn ModelBackend,
    ) -> Result<String, String> {
        if self.current_chapter >= self.outline.len() {
            return Err("No more chapters in outline".to_string());
        }

        let chapter_info = &self.outline[self.current_chapter];
        let chapter_num = self.current_chapter + 1;

        // Build context from plot state and recent chapters
        let context = self.build_context();

        let chapter: ChapterOutput = structured_complete(
            backend,
            "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
            &format!(
                "Write Chapter {}: {}\n\n\
                 Summary: {}\n\n\
                 Context:\n{context}\n\n\
                 Write approximately {} words of engaging prose. \
                 Output JSON with: title (string), content (string with chapter prose)",
                chapter_num,
                chapter_info.title,
                chapter_info.summary,
                self.config.words_per_chapter
            ),
            &ChapterOutput::grammar(),
            0.8,
            800,
        )?;

        let md = format!("# {}\n\n{}", chapter.title, chapter.content);
        self.chapters.push(md.clone());

        // Save chapter to workspace
        let filename = format!("03-CHAPTER_{}.md", chapter_num);
        let path = self.workspace.resolve(&filename).unwrap();
        let _ = write_file(&path, &md);

        self.current_chapter += 1;

        // Track task completion
        self.interaction_state.task_completed(md.clone());

        // Extract plot state if tracking enabled
        if self.config.track_plot_state {
            self.extract_plot_state(backend, &chapter.content)?;
        }

        Ok(md)
    }

    /// Continue a chapter from where it left off
    pub fn continue_chapter(
        &mut self,
        backend: &dyn ModelBackend,
        chapter_num: usize,
        direction: &str,
    ) -> Result<String, String> {
        if chapter_num == 0 || chapter_num > self.chapters.len() {
            return Err(format!("Chapter {} doesn't exist", chapter_num));
        }

        let existing = &self.chapters[chapter_num - 1];
        let context = self.build_context();

        let continuation: ChapterOutput = structured_complete(
            backend,
            "You are a fiction writer. Continue the story naturally. Output valid JSON only.",
            &format!(
                "Continue this chapter:\n\n{existing}\n\n\
                 Direction: {direction}\n\n\
                 Context:\n{context}\n\n\
                 Continue from where it left off. Don't restart or summarize. \
                 Output JSON with: title (string), content (string with continuation)",
            ),
            &ChapterOutput::grammar(),
            0.8,
            600,
        )?;

        // Append to existing chapter
        let updated = format!("{}\n\n{}", existing, continuation.content);
        self.chapters[chapter_num - 1] = updated.clone();

        // Update file
        let filename = format!("03-CHAPTER_{}.md", chapter_num);
        let path = self.workspace.resolve(&filename).unwrap();
        let _ = write_file(&path, &updated);

        Ok(updated)
    }

    /// Extract plot state from a chapter
    fn extract_plot_state(
        &mut self,
        backend: &dyn ModelBackend,
        chapter_text: &str,
    ) -> Result<(), String> {
        let new_state: PlotState = structured_complete(
            backend,
            "You are a story analyst. Extract the current plot state. Output valid JSON only.",
            &format!(
                "Analyze this chapter and extract the current plot state:\n\n\
                 {chapter_text}\n\n\
                 Current state:\n{:?}\n\n\
                 Output JSON matching the schema.",
                self.plot_state
            ),
            &PlotState::grammar(),
            0.3,
            400,
        )?;

        self.plot_state.merge(new_state);

        // Save plot state to workspace
        let json = serde_json::to_string_pretty(&self.plot_state)
            .unwrap_or_default();
        let path = self.workspace.resolve("07-PLOT-STATE.json").unwrap();
        let _ = write_file(&path, &json);

        Ok(())
    }

    /// Build context for chapter generation
    fn build_context(&self) -> String {
        let mut ctx = String::new();

        // Add plot state summary
        if self.config.track_plot_state {
            ctx.push_str("## Plot State\n");
            ctx.push_str(&format!("Arc stage: {}\n", self.plot_state.arc_stage));
            ctx.push_str(&format!("Location: {}\n", self.plot_state.current_location));

            if !self.plot_state.characters.is_empty() {
                ctx.push_str("\nCharacters:\n");
                for ch in &self.plot_state.characters {
                    ctx.push_str(&format!("- {}: {}\n", ch.name, ch.current_status));
                }
            }

            if !self.plot_state.active_conflicts.is_empty() {
                ctx.push_str("\nActive conflicts:\n");
                for c in &self.plot_state.active_conflicts {
                    ctx.push_str(&format!("- {}\n", c));
                }
            }

            if !self.plot_state.foreshadowing.is_empty() {
                ctx.push_str("\nForeshadowing planted:\n");
                for f in &self.plot_state.foreshadowing {
                    ctx.push_str(&format!("- {}\n", f));
                }
            }

            if !self.plot_state.recent_events.is_empty() {
                ctx.push_str("\nRecent events:\n");
                for e in &self.plot_state.recent_events {
                    ctx.push_str(&format!("- {}\n", e));
                }
            }
        }

        // Add last 2 chapters as recap
        let recap_count = 2.min(self.chapters.len());
        if recap_count > 0 {
            ctx.push_str("\n## Recent Chapters\n");
            for i in (self.chapters.len() - recap_count)..self.chapters.len() {
                let chapter_num = i + 1;
                // Just include first 200 chars as recap
                let recap: String = self.chapters[i].chars().take(200).collect();
                ctx.push_str(&format!("\n### Chapter {} (recap)\n{}...\n", chapter_num, recap));
            }
        }

        ctx
    }

    /// Render outline to markdown
    fn render_outline(&self) -> String {
        let mut md = String::from("Story Outline\n\n");
        for ch in &self.outline {
            md.push_str(&format!("## Chapter {}: {}\n{}\n\n", ch.number, ch.title, ch.summary));
        }
        md
    }

    /// Publish the complete story
    pub fn publish(&self) -> Result<String, String> {
        let mut story = String::new();

        // Add title from first outline entry
        if let Some(first) = self.outline.first() {
            story.push_str(&format!("# {}\n\n", first.title));
        }

        // Add chapters
        for (i, chapter) in self.chapters.iter().enumerate() {
            story.push_str(chapter);
            story.push_str("\n\n---\n\n");
        }

        // Save complete story
        let path = self.workspace.resolve("06-STORY.md").unwrap();
        let _ = write_file(&path, &story);

        Ok(story)
    }

    /// Evaluate the quality of a chapter using the model as judge.
    pub fn evaluate_chapter_quality(
        &mut self,
        backend: &dyn ModelBackend,
        chapter_num: usize,
    ) -> Result<StoryCritique, String> {
        if chapter_num == 0 || chapter_num > self.chapters.len() {
            return Err(format!("Chapter {} doesn't exist", chapter_num));
        }

        let chapter_text = &self.chapters[chapter_num - 1];
        let context = self.build_context();

        let analyzer = QualityAnalyzer::new(self.config.quality_threshold);
        let critique = analyzer.evaluate_chapter(
            backend,
            chapter_text,
            chapter_num,
            &context,
        )?;

        // Store quality score
        if self.chapter_scores.len() < chapter_num {
            self.chapter_scores.resize(chapter_num, QualityScore::default());
        }
        self.chapter_scores[chapter_num - 1] = critique.scores.clone();

        // Save quality report
        let report = format!(
            "# Quality Report — Chapter {}\n\n\
             Overall: {:.1}/10\n\
             Pacing: {:.1}/10\n\
             Show-don't-tell: {:.1}/10\n\
             Character voice: {:.1}/10\n\
             Plot coherence: {:.1}/10\n\
             Engagement: {:.1}/10\n\n\
             ## Issues\n{}\n\n\
             ## Strengths\n{}\n\n\
             ## Suggestions\n{}",
            chapter_num,
            critique.scores.overall,
            critique.scores.pacing,
            critique.scores.show_dont_tell,
            critique.scores.character_voice,
            critique.scores.plot_coherence,
            critique.scores.engagement,
            critique.scores.issues.iter()
                .map(|i| format!("- [{}] {}: {}", i.severity, i.category, i.description))
                .collect::<Vec<_>>()
                .join("\n"),
            critique.scores.strengths.iter()
                .map(|s| format!("- {}", s))
                .collect::<Vec<_>>()
                .join("\n"),
            critique.scores.suggestions.iter()
                .map(|s| format!("- {}", s))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        let path = self.workspace.resolve(&format!("08-QUALITY-{}.md", chapter_num)).unwrap();
        let _ = write_file(&path, &report);

        Ok(critique)
    }

    /// Revise a chapter based on critique.
    pub fn revise_chapter(
        &mut self,
        backend: &dyn ModelBackend,
        chapter_num: usize,
        critique: &StoryCritique,
    ) -> Result<String, String> {
        if chapter_num == 0 || chapter_num > self.chapters.len() {
            return Err(format!("Chapter {} doesn't exist", chapter_num));
        }

        let existing = &self.chapters[chapter_num - 1];
        let context = self.build_context();

        // Build revision instructions
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

        let chapter: ChapterOutput = structured_complete(
            backend,
            "You are a fiction writer revising a chapter. Improve based on feedback. Output valid JSON only.",
            &format!(
                "Revise this chapter based on the feedback.\n\n\
                 Original chapter:\n{existing}\n\n\
                 Feedback:\n{instructions}\n\n\
                 Plot context:\n{context}\n\n\
                 Improve the chapter while preserving its strengths. \
                 Output JSON with: title (string), content (string with revised chapter prose)"
            ),
            &ChapterOutput::grammar(),
            0.7,
            800,
        )?;

        let md = format!("# {}\n\n{}", chapter.title, chapter.content);
        self.chapters[chapter_num - 1] = md.clone();

        // Update file
        let filename = format!("03-CHAPTER_{}.md", chapter_num);
        let path = self.workspace.resolve(&filename).unwrap();
        let _ = write_file(&path, &md);

        // Record revision
        let revision_num = self.revisions.iter()
            .filter(|r| r.chapter_num == chapter_num)
            .count() + 1;

        self.revisions.push(RevisionRecord {
            chapter_num,
            revision_num,
            reason: critique.summary.clone(),
            quality_before: critique.scores.overall,
            quality_after: 0.0, // Will be updated after re-evaluation
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });

        Ok(md)
    }

    /// Get quality scores for all chapters
    pub fn chapter_scores(&self) -> &[QualityScore] {
        &self.chapter_scores
    }

    /// Get revision history
    pub fn revisions(&self) -> &[RevisionRecord] {
        &self.revisions
    }

    /// Get average quality score across all chapters
    pub fn average_quality(&self) -> f32 {
        if self.chapter_scores.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.chapter_scores.iter().map(|s| s.overall).sum();
        sum / self.chapter_scores.len() as f32
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper structs and functions
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ChapterOutput {
    title: String,
    content: String,
}

impl ChapterOutput {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("content", Schema::string())
            .build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json())
            .expect("ChapterOutput schema is valid")
    }
}

/// Call the model with grammar constraint and deserialize output
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

    serde_json::from_str::<T>(&text)
        .map_err(|e| format!("parse error: {e}\nraw: {text}"))
}

/// Write file helper
fn write_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Create a timestamped workspace for the story
fn create_story_workspace() -> Result<Workspace, String> {
    let base = std::env::current_dir()
        .map_err(|e| format!("failed to get cwd: {e}"))?
        .join(".roco")
        .join("workspaces");

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let dir = base.join(format!("story_{ts}"));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create workspace: {e}"))?;

    Workspace::from_existing(dir, WorkspaceKind::Agent)
        .map_err(|e| format!("failed to init workspace: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plot_state_merge() {
        let mut state = PlotState::default();
        state.characters.push(CharacterState {
            name: "Alice".into(),
            current_status: "alive".into(),
            last_seen: "chapter 1".into(),
            knowledge: vec!["knows about the sword".into()],
        });
        state.active_conflicts.push("the dark lord rises".into());

        let new_state = PlotState {
            chapter_count: 2,
            characters: vec![
                CharacterState {
                    name: "Alice".into(),
                    current_status: "wounded".into(),
                    last_seen: "chapter 2".into(),
                    knowledge: vec!["knows about the sword".into(), "knows the dark lord's plan".into()],
                },
                CharacterState {
                    name: "Bob".into(),
                    current_status: "alive".into(),
                    last_seen: "chapter 2".into(),
                    knowledge: vec![],
                },
            ],
            active_conflicts: vec!["the dark lord rises".into(), "alice is wounded".into()],
            resolved_conflicts: vec![],
            current_location: "the forest".into(),
            arc_stage: "rising_action".into(),
            ..Default::default()
        };

        state.merge(new_state);

        assert_eq!(state.chapter_count, 2);
        assert_eq!(state.characters.len(), 2);
        assert_eq!(state.characters[0].current_status, "wounded");
        assert_eq!(state.active_conflicts.len(), 2);
        assert_eq!(state.arc_stage, "rising_action");
    }

    #[test]
    fn test_plot_state_resolve_conflict() {
        let mut state = PlotState::default();
        state.active_conflicts.push("the dark lord rises".into());
        state.active_conflicts.push("alice is wounded".into());

        let new_state = PlotState {
            active_conflicts: vec![],
            resolved_conflicts: vec!["the dark lord rises".into()],
            ..Default::default()
        };

        state.merge(new_state);

        assert_eq!(state.active_conflicts.len(), 1);
        assert_eq!(state.active_conflicts[0], "alice is wounded");
        assert_eq!(state.resolved_conflicts.len(), 1);
    }
}
