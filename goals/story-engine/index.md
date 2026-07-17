# Goals: story-engine

The story generation engine — transforms RoCo AI from a demo into a collaborative storytelling tool.

## Core Philosophy: Human-AI Collaboration

**The human is the author. The AI is the tool.**

The story engine exists to amplify human creativity, not replace it. Every design decision should ask:
- Does this give the human more control?
- Does this make the human feel like the author?
- Does this respect the human's creative vision?
- Does this make the interaction natural and intuitive?

## Grammar-First Principle

Every model call in the story pipeline must go through a BNF grammar. The current pipeline uses `schema_to_gbnf` for JSON structure (outline, wiki, chapter envelope), but the prose content inside those envelopes is still free-form. This is the critical gap.

See: `goals/mechanistic-agent/task_grammars.md` for the grammar coverage audit.

## Prerequisites

Prerequisite order (top to bottom):

### Phase 1: Core Engine (DONE)
1. ✅ **dynamic_outline** — generate N chapters, then expand outline to N+M chapters; no fixed limit
2. ✅ **plot_state_tracking** — structured plot state extraction after each chapter (grammar-constrained JSON)
3. ✅ **context_assembly** — pass plot state + last 2 chapters as context (not full history); solve context window limits
4. ✅ **chapter_continuation** — resume writing from where a chapter left off (mid-scene, mid-dialogue)

### Phase 2: Human-AI Interaction (CURRENT FOCUS)
5. **collaborative_outline** — human and AI co-create the outline together
6. **natural_feedback** — human gives feedback in natural language, AI understands and applies it
7. **real_time_preview** — show what's being generated as it's generated
8. **easy_revision** — one-command revision with clear before/after
9. **story_direction** — human sets tone, style, themes; AI respects them throughout
10. **chapter_steering** — human can steer a chapter mid-generation

### Phase 3: Quality & Polish (FUTURE)
11. **prose_quality_metrics** — multi-dimensional scoring: pacing, dialogue density, show-don't-tell
12. **per_handler_grammars** — domain-specific BNF grammars for chapter prose, wiki, validation, synopsis

### Phase 4: Persistence & Sharing (FUTURE)
13. **session_persistence** — save/load story state; resume days later with continuity
14. **export_formats** — export to markdown, PDF, epub, docx
15. **story_sharing** — share stories with others

## Human-AI Interaction Design

### The Collaborative Workflow

```
Human: "I want to write a dark fantasy about a fallen knight"
   ↓
AI: Generates initial outline (3 chapters)
   ↓
Human: "I like it, but add a chapter about the knight's past"
   ↓
AI: Adds Chapter 2 (backstory), renumbers rest
   ↓
Human: "Let's start writing"
   ↓
AI: Generates Chapter 1
   ↓
Human: "The pacing is too slow in the middle"
   ↓
AI: Regenerates middle section with faster pacing
   ↓
Human: "Perfect. Continue to Chapter 2"
   ↓
AI: Generates Chapter 2
   ↓
Human: "I want the knight to meet a mysterious stranger here"
   ↓
AI: Rewrites Chapter 2 to include the stranger
   ↓
... and so on
```

### Key Interaction Patterns

**1. Natural Language Feedback**
- Human says what they want in plain English
- AI understands intent and applies it
- No commands to memorize, no syntax to learn

**2. Incremental Control**
- Human can intervene at any point
- Small changes don't require regenerating everything
- Undo/redo support

**3. Transparent Process**
- Human sees what AI is "thinking" (outline, plot state)
- Human understands why AI made certain choices
- Human can override any decision

**4. Respect for Vision**
- AI asks before making major changes
- AI explains trade-offs (e.g., "Adding this subplot will require 2 more chapters")
- AI never contradicts established facts

### Interaction Modes

**Mode 1: Guided (Default)**
- AI generates, human reviews and approves
- Human can request changes at any point
- Best for: beginners, casual writers

**Mode 2: Collaborative**
- Human and AI alternate creating content
- Human writes key scenes, AI fills in transitions
- Best for: experienced writers, co-creation

**Mode 3: Directed**
- Human gives detailed instructions, AI executes
- Human specifies exactly what should happen
- Best for: writers with clear vision

### Feedback Types

**Quick Feedback (one command)**
- "Continue" — keep going
- "Shorter" — make it more concise
- "Longer" — expand this section
- "Darker" — darker tone
- "Lighter" — lighter tone
- "More dialogue" — add more conversation
- "More action" — add more action
- "More description" — add more setting/atmosphere

**Detailed Feedback (natural language)**
- "I want the knight to hesitate before drawing his sword"
- "Make the stranger more mysterious, not threatening"
- "The ending feels rushed, expand the final confrontation"
- "Add foreshadowing about the dragon in Chapter 1"

**Structural Feedback**
- "Add a chapter about the knight's childhood"
- "Move the battle scene to Chapter 3"
- "Split this chapter into two"
- "Merge these two chapters"

## Status & Self-Directed Actions

### Phase 1: Core Engine — ✅ DONE

1. **dynamic_outline** ✅ — `StoryEngine::expand_outline()`
2. **plot_state_tracking** ✅ — `PlotState` with merge/extract
3. **context_assembly** ✅ — `StoryEngine::build_context()`
4. **chapter_continuation** ✅ — `StoryEngine::continue_chapter()`

### Phase 2: Human-AI Interaction — 🔴 CURRENT FOCUS

5. **collaborative_outline** 🔴 — human and AI co-create outline
   - *Self-directed:* Implement outline editing commands (add/remove/reorder chapters)
   - *Self-directed:* AI suggests outline changes based on human feedback
   - *Self-directed:* Show outline and ask for approval before writing

6. **natural_feedback** 🔴 — human gives feedback in natural language
   - *Self-directed:* Parse natural language feedback into structured directives
   - *Self-directed:* Apply directives to chapter generation
   - *Self-directed:* Support feedback like "make it darker", "add more dialogue"

7. **real_time_preview** 🔴 — show generation as it happens
   - *Self-directed:* Stream chapter content to terminal
   - *Self-directed:* Show progress indicators
   - *Self-directed:* Allow pause/resume during generation

8. **easy_revision** 🔴 — one-command revision with clear before/after
   - *Self-directed:* Show diff between original and revised
   - *Self-directed:* Allow human to accept/reject/modify revision
   - *Self-directed:* Support "undo" to revert changes

9. **story_direction** 🔴 — human sets tone, style, themes
   - *Self-directed:* Capture direction at story start
   - *Self-directed:* Apply direction consistently throughout
   - *Self-directed:* Show how direction influences generation

10. **chapter_steering** 🔴 — steer chapter mid-generation
    - *Self-directed:* Allow human to pause generation
    - *Self-directed:* Accept mid-chapter direction
    - *Self-directed:* Resume generation with new direction

### Phase 3: Quality & Polish — 🔴 FUTURE

11. **prose_quality_metrics** 🔴 — multi-dimensional scoring
12. **per_handler_grammars** 🔴 — domain-specific BNF for prose content

### Phase 4: Persistence & Sharing — 🔴 FUTURE

13. **session_persistence** 🔴 — save/load story state
14. **export_formats** 🔴 — markdown, PDF, epub, docx
15. **story_sharing** 🔴 — share with others

---

## Current Pipeline (for reference)

```
premise → outline → wiki → [chapter → plot_state → expand_outline]* → publish
                         ↑           ↓
                         └───────────┘ (dynamic loop)
```

**What works:**
- Dynamic chapter generation
- Plot state tracking
- Context assembly
- Quality evaluation
- Revision support
- Session persistence

**What's missing (human aspect):**
- Collaborative outline editing
- Natural language feedback
- Real-time preview
- Easy revision with diff
- Story direction persistence
- Chapter steering

---

## Done Criteria

### Phase 2 Complete When:
- [ ] Human can edit outline by saying "add a chapter about X"
- [ ] Human can give feedback like "make it darker" and see changes
- [ ] Human can see chapter being written in real-time
- [ ] Human can see diff between original and revised chapter
- [ ] Human can set story direction at start and it's respected throughout
- [ ] Human can pause generation and give mid-chapter direction
- [ ] The interaction feels natural and intuitive
- [ ] The human feels like the author, not just a spectator

---

*Created: 2026-07-17*
*Last updated: 2026-07-17 — Prioritized human-AI interaction*
