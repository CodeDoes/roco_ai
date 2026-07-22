# Story Mode & Story Validator — Implementation Plan

> **Date:** 2026-07-22 | **Status:** Plan (not yet implemented)
> **Depends on:** `crates/validation/` (committed), `crates/agent-story/`, `crates/cli/src/interact.rs`

---

## Overview

We're introducing a **two-mode interaction system** for the RoCo CLI:

| Mode | Description |
|------|-------------|
| **Default** | General-purpose assistant (chat, code, game, HTML) — all existing commands work as-is |
| **Story** | Locked to a specific story workspace. Natural language maps to story operations — validation, editing, world-building, summarization |

The story validator was the foundation (committed). This plan covers everything above it.

---

## A. Architecture

```
CLI (roco interact / roco story)
  │
  ├── DefaultMode (existing: InteractionHandler, AgentChat, etc.)
  │
  └── StoryMode (NEW)
        ├── IntentClassifier      — parse NL → StoryIntent
        ├── StorySession          — pinned to one .roco/workspaces/<story>/
        ├── StoryToolSet          — grep, find, ls, read, write, edit (find-replace)
        ├── StoryValidatorBridge  — calls ValidationEngine
        ├── StorySummarizer       — condensed data forms, summaries
        ├── StoryPlanner          — outline diffs, modification plans
        └── StoryIdeaGenerator    — brainstorming / outlining new stories
```

### Mode switching

```text
User: "let's work on my fantasy story"
  → IntentClassifier detects "story" + "fantasy"
  → StorySession::new("fantasy")
  → Lock to StoryMode

User: "how about that sci-fi story?"
  → IntentClassifier detects story switch
  → StorySession::switch("sci-fi")

User: "ok that's enough for now"
  → StorySession::unlock()
  → Return to DefaultMode

On restart: StorySession::resume_last() → greet with status
```

---

## B. Intent Classification (StoryMode)

Map natural language to `StoryIntent`:

| NL Pattern | StoryIntent | Priority |
|---|---|---|
| "validate chapter \d+" | `ValidateChapter(num)` | High |
| "check outline" / "is the outline in sync?" | `ValidateOutline` | High |
| "check wiki consistency" / "is wiki in sync?" | `ValidateWiki` | High |
| "summarize chapter \d+" / "give me a summary" | `SummarizeChapter(num)` | Med |
| "summarize the story so far" | `SummarizeStory` | Med |
| "what do we know about [X]" | `FindInfo(query)` | High |
| "change [character]'s name to [Y]" | `ChangeCharacterName { old, new }` | Med |
| "change writing style to [style]" | `ChangeStyle(style)` | Med |
| "change POV to [first/second/third]" | `ChangePOV(pov)` | Med |
| "edit chapter \d+ to [description]" | `EditChapter { num, description }` | Med |
| "make chapter \d+ more like [description]" | `ReviseChapter { num, direction }` | Med |
| "what changed in the outline?" | `OutlineDiff` | Med |
| "plan how to modify the story" | `PlanModification` | Low |
| "update outline to match chapters" | `SyncOutlineToChapters` | Med |
| "help me think of a story idea" / "brainstorm" | `BrainstormStory` | Low |
| "let's work on [story]" | `LockStory(name)` | High |
| "switch to [story]" / "what about [story]" | `SwitchStory(name)` | High |
| "what story was I working on?" | `ResumeLastStory` | High |
| "ok that's enough" / "back to normal" | `UnlockStory` | High |

### Implementation approach

```rust
pub enum StoryIntent {
    // Validation
    ValidateChapter(usize),
    ValidateAllChapters,
    ValidateOutline,
    ValidateWiki,
    ValidateAll,
    EvaluateChapterAgainstPrevious(usize),

    // Summarization
    SummarizeChapter(usize),
    SummarizeAllChapters,
    SummarizeStory,
    CondenseChapter(usize),    // chapter → condensed data form
    CondenseWiki,              // wiki → condensed data form

    // Information retrieval
    FindInfo { query: String },

    // Editing / revision
    EditChapter { num: usize, description: String },
    ReviseChapter { num: usize, direction: String },
    ChangeCharacterName { old: String, new: String },
    ChangeStyle(String),
    ChangePOV(String),

    // Outline management
    OutlineDiff,
    PlanModification,
    SyncOutlineToChapters,

    // Mode
    LockStory(String),
    SwitchStory(String),
    ResumeLastStory,
    UnlockStory,
    StatusUpdate,

    // Creation
    BrainstormStory,
}
```

Classifier strategy:
- Keyword + regex matching (fast, no model needed for common patterns)
- Fallback: model-as-classifier for ambiguous input

---

## C. Condensed Data Forms

### Chapter → Condensed

```rust
pub struct CondensedChapter {
    pub chapter_num: usize,
    pub title: String,
    pub word_count: usize,
    pub characters_mentioned: Vec<String>,     // from wiki cross-ref
    pub settings_mentioned: Vec<String>,
    pub plot_points: Vec<String>,              // key events
    pub tone: String,                          // inferred tone
    pub pov_character: Option<String>,
    pub summary_2_sentences: String,
    pub themes: Vec<String>,
}
```

Generation:
1. **Classic extraction**: word count, character names (wiki cross-ref from `WikiValidator`)
2. **Inference-backed**: plot points, tone, themes, 2-sentence summary
3. Cached per session; invalidated when chapter changes

### Wiki → Condensed

```rust
pub struct CondensedWiki {
    pub characters: Vec<WikiEntry>,    // name, role, key traits, relationships
    pub settings: Vec<WikiEntry>,      // name, type, description
    pub lore_items: Vec<WikiEntry>,    // items, magic, history
    pub entry_count: usize,
    pub total_word_count: usize,
}
```

---

## D. Story Summarizer

### `summarize_story(chapters, wiki, outline) → StorySummary`

```rust
pub struct StorySummary {
    pub title: String,
    pub genre: String,
    pub chapter_count: usize,
    pub total_word_count: usize,
    pub characters: Vec<String>,
    pub synopsis: String,          // 3-5 paragraphs
    pub arc_status: String,        // "beginning", "middle", "end", "complete"
    pub latest_chapter_preview: String,
    pub last_updated: String,
}
```

Strategy:
- Model-generated synopsis (grammar-constrained JSON)
- Classic data for word counts, character lists, chapter count
- Arc status derived from outline position

---

## E. StoryToolSet — File Operations for Story Editing

The `StoryToolSet` wraps the workspace filesystem and provides safe operations:

```rust
pub struct StoryToolSet {
    workspace_path: PathBuf,
    wiki_dir: PathBuf,
    chapters_dir: PathBuf,
    outline_path: PathBuf,
}

impl StoryToolSet {
    // Reading
    pub fn read_wiki(&self) -> Result<String>;
    pub fn read_chapter(&self, num: usize) -> Result<String>;
    pub fn read_all_chapters(&self) -> Result<Vec<String>>;
    pub fn read_outline(&self) -> Result<String>;
    pub fn grep_wiki(&self, pattern: &str) -> Result<Vec<GrepMatch>>;
    pub fn grep_chapters(&self, pattern: &str) -> Result<Vec<GrepMatch>>;
    pub fn find_in_wiki(&self, query: &str) -> Result<Vec<FileLocation>>;

    // Writing / editing
    pub fn write_chapter(&self, num: usize, content: &str) -> Result<()>;
    pub fn edit_chapter(&self, num: usize, old: &str, new: &str) -> Result<()>;
    pub fn edit_wiki(&self, old: &str, new: &str) -> Result<()>;
    pub fn find_replace_chapters(&self, old: &str, new: &str) -> Result<Vec<EditResult>>;
    pub fn find_replace_wiki(&self, old: &str, new: &str) -> Result<EditResult>;
    pub fn grep(&self, pattern: &str, paths: &[&str]) -> Result<Vec<GrepMatch>>;
}
```

For the "change character name" flow:
```
grep_all_chapters("OldName") → locations
grep_wiki("OldName") → locations
confirm changes with user
find_replace(old, new) for each file
```

For "edit chapter to be like X":
```
read_chapter(N)
model_revision(chapter, instruction)
show diff
write_chapter(N, revised)
```

---

## F. Outline Diff & Plan

### OutlineDiff

```rust
pub struct OutlineDiff {
    pub changes: Vec<OutlineChange>,
    pub summary: String,
}

pub enum OutlineChange {
    ChapterAdded { number: usize, title: String },
    ChapterRemoved { number: usize, title: String },
    ChapterRenamed { number: usize, old_title: String, new_title: String },
    ChapterSummaryChanged { number: usize, summary_old: String, summary_new: String },
    PlotArcChanged { description: String },
    MetadataChanged { field: String, old: String, new: String },
}
```

Implementation: track outline snapshots per session. Compare current vs last-known.

### PlanModification

Given an outline diff, generate a modification plan:

```rust
pub struct ModificationPlan {
    pub affected_chapters: Vec<usize>,
    pub changes_required: Vec<String>,
    pub preserves_continuity: bool,
    pub recommended_approach: String,
    pub estimated_effort: String, // "minor", "moderate", "major rewrite"
}
```

Model-generated, grammar-constrained JSON.

---

## G. Story Session & Mode Management

```rust
pub struct StorySession {
    pub story_name: String,
    pub workspace: Workspace,
    pub tool_set: StoryToolSet,
    pub validator: ValidationEngine,
    pub summarizer: StorySummarizer,
    pub outline_snapshot: Option<String>,    // for diffing
    pub chapter_cache: HashMap<usize, String>,
    pub wiki_cache: String,
}
```

### StorySessionManager (persisted across CLI sessions)

```rust
pub struct StorySessionManager {
    active_session: Option<StorySession>,
    session_history: Vec<(String, Instant)>, // most recent first
}

impl StorySessionManager {
    pub fn lock(&mut self, name: &str) -> Result<()>;
    pub fn unlock(&mut self);
    pub fn switch(&mut self, name: &str) -> Result<()>;
    pub fn resume_last(&mut self) -> Option<&StorySession>;
    pub fn status(&self) -> String;           // "Working on 'fantasy' — 3 chapters, 4500 words"
    pub fn list_stories(&self) -> Vec<String>;
}
```

Persistence: `~/.roco/story_sessions.json` — stores last-active story per workspace + timestamps.

### Status update on greet

```rust
fn greet_with_status(manager: &StorySessionManager) -> String {
    if let Some(session) = manager.active_session() {
        format!(
            "📖 Working on **{}** — {} chapters, ~{} words. {}/{} checks passing.",
            session.story_name,
            session.chapter_count(),
            session.total_word_count(),
            session.last_report().passed_count(),
            session.last_report().total_count(),
        )
    } else {
        "✨ RoCo ready. Use 'let's work on [story]' to start writing.".to_string()
    }
}
```

---

## H. Brainstorm / Story Idea Generator

```rust
pub struct StoryIdeaGenerator {
    backend: Arc<dyn ModelBackend>,
}

impl StoryIdeaGenerator {
    /// Generate story ideas from a prompt
    pub fn brainstorm(&self, prompt: &str) -> Result<Vec<StoryIdea>>;

    /// Expand a premise into a full outline
    pub fn expand_premise(&self, premise: &str) -> Result<StoryIdea>;
}

pub struct StoryIdea {
    pub title: String,
    pub genre: String,
    pub tone: String,
    pub premise: String,
    pub protagonist: String,
    pub central_conflict: String,
    pub suggested_chapters: Vec<String>,
    pub themes: Vec<String>,
}
```

Model-generated, grammar-constrained JSON. Output can be saved as a new workspace.

---

## I. Implementation Order (Phases)

### Phase 1 (Current — committed)
- ✅ `crates/validation/` — classic, inference, outline, wiki validators
- ✅ ValidationEngine orchestrator
- ✅ Eval cases for validation
- ✅ 42 unit tests

### Phase 2 (StorySession & IntentClassifier) — NEXT
1. `crates/validation/src/condensed.rs` — chapter→condensed, wiki→condensed
2. `crates/validation/src/summarizer.rs` — `StorySummarizer`
3. `crates/validation/src/planner.rs` — `OutlineDiff`, `ModificationPlan`
4. `crates/validation/src/intent.rs` — `StoryIntent` + classifier
5. `crates/validation/src/tool_set.rs` — `StoryToolSet` (grep, find, read, write, edit)
6. `crates/validation/src/session.rs` — `StorySession`, `StorySessionManager`
7. `crates/validation/src/brainstorm.rs` — `StoryIdeaGenerator`
8. Update `lib.rs` to export new modules

### Phase 3 (CLI Integration)
1. `crates/cli/src/story_mode.rs` — `StoryMode` handler
2. Update `crates/cli/src/cmd/interact.rs` — dispatch to StoryMode or DefaultMode
3. Update `crates/cli/src/cmd/story.rs` — `roco story` subcommand
4. Session persistence (JSON in `.roco/`)

### Phase 4 (Desktop Integration)
1. `crates/ui/src/story_mode.rs` — `StoryModeWidget` for desktop
2. Wire into `desktop_app.rs` right panel

### Phase 5 (Polish)
1. Streaming validation feedback
2. Multi-story management UI
3. Undo/revert for find-replace operations
4. Continuous validation (auto-check on save)

---

## J. File Changes Summary

| File | Action | Description |
|---|---|---|
| `crates/validation/src/condensed.rs` | **NEW** | Chapter & wiki condensed data forms |
| `crates/validation/src/summarizer.rs` | **NEW** | Story summarization |
| `crates/validation/src/planner.rs` | **NEW** | Outline diff & modification plan |
| `crates/validation/src/intent.rs` | **NEW** | StoryIntent enum + NL classifier |
| `crates/validation/src/tool_set.rs` | **NEW** | StoryToolSet (grep/find/read/write/edit) |
| `crates/validation/src/session.rs` | **NEW** | StorySession, StorySessionManager |
| `crates/validation/src/brainstorm.rs` | **NEW** | StoryIdeaGenerator |
| `crates/validation/src/lib.rs` | EDIT | Export new modules, add re-exports |
| `crates/validation/Cargo.toml` | EDIT | Add regex dependency for tool_set |
| `crates/cli/src/story_mode.rs` | **NEW** | StoryMode handler |
| `crates/cli/src/cmd/interact.rs` | EDIT | Dispatch to StoryMode |
| `crates/cli/src/cmd/story.rs` | EDIT | `roco story` subcommand |
| `crates/ui/src/story_mode.rs` | **NEW** | Desktop story mode widget |
| `roadmap/progress.md` | EDIT | Append progress entries |

---

## K. Key Design Decisions

1. **No model needed for basic operations.** Grep, find-replace, read, write are pure filesystem ops. Model only used for: condensed forms, summarization, brainstorming, critique, and ambiguous intent classification.
2. **Grammar-constrained JSON every time.** All model output uses `roco-grammar` Schema → GBNF. No free-form prose from the model for structured data.
3. **Workspace-first.** Every story is a `.roco/workspaces/<name>/` directory with `outline.md`, `wiki.md`, `chapters/`, and `.session.json`.
4. **Caching.** Chapter/wiki cache in `StorySession` avoids re-reading disk. Invalidated on write.
5. **Backward compatibility.** All existing CLI modes (code, game, html, desktop) remain untouched. Story mode is additive.
6. **Undo support.** `StoryToolSet` keeps a backup before each edit operation (`.backup/` in workspace).

---

*See `AGENTS.md` Section J for research synthesis, `EDIT_GUIDE.md` for edit boundaries.*
