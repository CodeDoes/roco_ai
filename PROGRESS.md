# PROGRESS.md — RoCo AI

> Strategy / context / "what we wanted to do but didn't yet".
> Living document; this version reflects the human-AI collaboration focus as of 2026-07-17.

## Current Focus

**Primary Goal:** A collaborative story writing tool where humans and AI work together to create stories.

The core engine is done. Now the priority is **human-AI interaction** — making the tool feel natural, intuitive, and empowering for the human author.

## Philosophy: Human Controls Pace, Not Reviews Output

**The human should not be burdened with reviewing everything.**

Instead of:
- Agent generates everything → Human reviews everything → Agent revises everything

Do this:
- Agent completes **one task** → Human sees result → Human decides: accept, modify, skip, stop

This is a conversation, not a review process. The human controls the pace by:
- **Accepting** — move to next task
- **Modifying** — give feedback, agent revises
- **Skipping** — jump ahead
- **Stopping** — end and publish

No mandatory review. No approval gates. Just natural flow.

Every design decision should ask:
- Does this give the human more control?
- Does this make the human feel like the author?
- Does this respect the human's creative vision?
- Does this make the interaction natural and intuitive?
- Does this avoid burdening the human with review?

## Lessons Learned

### The Grammar-First Principle
Live generation runs on undertrained RWKV models (1B–2.9B g1h) revealed a fundamental truth:
**free-form prompting cannot prevent meta-commentary contamination**. No amount of system
prompting, temperature decay, or post-processing reliably stops the model from outputting
`<think>` planning text.

The correct architectural pattern is **grammar-constrained decoding at every call site**:
- When output must satisfy a BNF grammar, the sampler rejects non-conforming tokens at every step
- Contamination literally cannot occur — the grammar doesn't allow it
- No stripping, no retries, no fallbacks needed
- Error recovery reduces to timeout/retry logic only

Post-processing approaches (strip_think_blocks, pre-fill workarounds) are **interim signals**
marking where proper grammars still need to be added.

### The Story Pipeline Gap (2026-07-17 Analysis)
Deep review of the story pipeline revealed the **real gaps** between "works" and "ready":

1. **Fixed chapter count** — hardcoded to 3; can't write a novel with that ✅ FIXED
2. **No plot state tracking** — chapters get raw text, not structured understanding ✅ FIXED
3. **No interactive feedback** — batch-only, no human creative direction ✅ FIXED
4. **Grammar coverage gap** — JSON envelope constrained, prose content free-form 🔴 TODO
5. **Shallow validation** — binary pass/fail, not multi-dimensional quality ✅ FIXED

### The Human-AI Interaction Gap (2026-07-17 Analysis)
Core engine is solid, but the human experience needs work:

1. **Outline editing is clunky** — human can't easily modify outline 🔴 TODO
2. **Feedback is command-based** — human must memorize commands 🔴 TODO
3. **No real-time preview** — human waits for full chapter generation 🔴 TODO
4. **Revision is opaque** — human can't see what changed 🔴 TODO
5. **Direction isn't persistent** — human's creative vision not captured 🔴 TODO
6. **No mid-chapter steering** — human can't pause and redirect 🔴 TODO

These are the barriers between "tool" and "collaborative partner".

### Mechanistic Agent Live Testing Results
The `crates/cli/examples/story.rs` pipeline demonstrated the full mechanistic agent pattern end-to-end:
- Outline → wiki → chapter×3 (with validate + self-correction) → synopsis → publish
- Persists artifacts to `.roco/workspaces/story_/`
- Self-correction loops detect validation failures and retry with tighter prompts
- Context budgeting gates snippet inclusion per inference call
- All stages work structurally; content quality limited by model size when not grammar-constrained

### Architecture Decisions Proven Correct
- Code owns control flow, LLM only fires at fixed grammar-bounded points
- Pull-based context injection over push-based bulk transfer
- Arc-owned context sources cleanly satisfy `'static` bounds for `Box<dyn Fn>`
- Jaccard word overlap relevance scoring sufficient for initial use
- Persistent timestamped workspaces prevent collision across repeated runs
- Pre-fill `<think>>...plan...` tricks model into clean output when grammars unavailable

### What Didn't Work
- System prompts alone preventing `<think>>` leakage — zero effect regardless of strength
- Temperature decay (0.6→0.3) stopping contamination — model leaks at all temperatures
- Character-by-character think block stripping — closing tags never detected, open-ended blocks dominate
- Fallback returning raw unstripped text — defeats the purpose entirely
- Over-engineered regex/state-machine parsers — simple string replace works better

## Current scope

The active focus is **human-AI collaboration** — making the story engine feel like a
natural writing partner. The core engine (dynamic chapters, plot state, quality evaluation,
revision support, session persistence) is done. Now we need to make it **feel right**.

The product roadmap lives in `goals/` — indexed by `goals/index.md` and
AGENTS.md — as prerequisite-ordered layers: `infer`, `message`,
`workspace`, `agent`, `agent_chat`, `browser_use`, `testing`, plus
future `coder`. This file is the strategy/context layer (the "why",
dead-ends, run book); the actionable roadmap is `goals/`.

### Completed priorities

**BNF / Grammar-constrained decoding — ✅ DONE.** The `BnfConstraint`
module (`crates/grammar/src/bnf.rs`) wraps `bnf_sampler` (v0.3.8)
with a `qp-trie` vocabulary built from the model's tokenizer. It is the
primary grammar engine in `rwkv_backend.rs`, with schoolmarm as a
transparent fallback for GBNF grammars that use features `bnf_sampler`
can't parse (character classes `[...]`, quantifiers `*`). The GBNF→BNF
converter wraps nonterminal names in angle brackets so `bnf_sampler`'s
parser accepts them.

**State-mixing / State pool — ✅ Phase 1 DONE.** Session-based
save/restore is wired through the entire pipeline:
`CompletionRequest::session` → `CompleteReq::session` →
`RwkvActor::handle_complete`. Before generation the actor loads the saved
session state (or blank initial state); after generation it reads the
state back via `AnyState::back()` and stores it in the LRU pool. The pool
evicts least-recently-used sessions when it exceeds `max_sessions`
(default 8). Phase 2 (multi-slot GPU pool with concurrent batching) and
Phase 3 (tensor-level state blending) are forward work.

**Chat CLI — ✅ DONE.** `crates/cli/examples/chat.rs` provides a terminal
REPL with streaming output, session persistence (`session: "chat"`),
grammar constraints (`/grammar <gbnf>`), temperature control, and Ctrl+C
interrupt. Invoked via `cargo run -p roco-cli --example chat --release`.
There is also a `roco` binary (`crates/cli/src/bin/roco.rs`).

**Monorepo restructuring — ✅ DONE.** The monolithic `crates/core` was
split into 13 focused crates: `engine`, `grammar`, `inference`, `message`,
`session`, `tools`, `workspace`, `agent`, `chat-common`, `cli`, `tui`,
`server`, `gateway`. `infer` layer is complete (raw model, tokenization,
quantize, inference, streaming, GBNF, structured output + objects, thinking,
state save/load/mix, interrupt, continue). `testing/eval_harness` is done.

**Message layer — ✅ DONE (core).** `crates/message/src/gbnf.rs` generates
the structured chat GBNF (`message_format_gbnf` + `assistant_response_gbnf`,
schoolmarm-compatible, with think / tool_tag variants). `crates/tools` has 6
built-in tools (read/write/search/list/bash/now) with JSON schemas, a
`ToolRegistry`, and `parse` helpers that extract `<tool>` blocks and
segment assistant output. `crates/message/src/error.rs` provides
`complete_with_retry` (grammar fallback, truncation handling, backoff).

**Agent loop — ✅ DONE (core ReAct).** `crates/agent/src/agent.rs` runs the
observe→think→act loop: render prompt → constrained generate → parse
segments → execute tools via `ToolRegistry` → feed `<result>` back →
repeat until final answer or step/budget limit. `AgentConfig` /
`AgentStep` / `AgentTrace` record the run. Runnable via
`cargo run -p roco-cli --example agent --release`.

**Story engine — ✅ DONE (core).** `crates/agent/src/story_engine.rs` implements
dynamic story generation with:
- Dynamic outline expansion (no fixed chapter limit)
- Plot state tracking (structured JSON after each chapter)
- Context assembly (plot state + recent chapters as context)
- Chapter continuation (resume from where chapter left off)
- Quality evaluation (model-as-judge)
- Revision support (critique-based revision)
- Session persistence (save/load story state)

**Story engine interaction — 🟡 IN PROGRESS.** The human-AI interaction layer
needs work. Current interactive mode is command-based (continue/revise/direct/quit).
Needs: natural language feedback, real-time preview, easy revision with diff,
story direction persistence, chapter steering.

**Collaborative story example — ✅ DONE.** `crates/cli/examples/story_collaborative.rs`
demonstrates the conversational, collaborative approach:
- Shows outline and asks for approval before writing
- Asks for feedback after each chapter
- Supports multiple feedback types (good, revise, direct, extend, quit)
- Conversational tone throughout
- Human feels like the author, not just a spectator

**Observability system — ✅ DONE.** `crates/agent/src/observability.rs` implements:
- Model call recording (input, output, grammar, params, latency)
- Decision tracing (what was decided, why, alternatives)
- Action logging (what was done, where, when)
- Quality assessment recording
- Trace/span system for execution tracking
- Summary reports

**Reversibility system — ✅ DONE.** `crates/agent/src/reversibility.rs` implements:
- Workspace snapshots before file changes
- Action history with undo/redo support
- Rollback to any previous state
- Git-like versioning for story state
- Forward/backward payloads for each action

**Commentary system — ✅ DONE.** `crates/agent/src/commentary.rs` implements:
- Agent-generated explanations for artifacts
- Human commentary and annotations
- Why decisions were made
- Alternatives considered
- Trade-offs made
- What the human should review
- Human verdicts (approved, rejected, needs_changes)
- Human notes and annotations
- Markdown comment blocks for transparency

**Writing assistant — ✅ DONE.** `crates/agent/src/writing_assistant.rs` implements:
- Writing analysis (themes, characters, tone, style, sentiment)
- Continuation suggestions
- Fill-in-the-middle suggestions
- Diff analysis between versions
- Cross-referencing with existing content
- Tagging and categorization

**Interaction modes — ✅ DONE.** `crates/agent/src/interaction.rs` implements:
- Interactive: human sees each chapter, can give feedback
- Automatic: agent runs to completion (this IS "go ham")
- Human can switch modes at any time
- Human actions: accept, revise, skip, stop, switch modes

**Natural language feedback — ✅ DONE.** `crates/agent/src/natural_feedback.rs` implements:
- Parse natural language feedback into structured directives
- Quick parse for simple commands (c, skip, stop)
- Model-based parsing for complex feedback ("make it darker")
- Extract intent: revise, continue, stop, skip, direction
- Extract directives: tone, pacing, character, plot, style, content

**Outline editing — ✅ DONE.** `crates/agent/src/outline_editing.rs` implements:
- Collaborative outline creation and modification
- Commands: add, remove, move, modify, regenerate
- Natural language command parsing
- Edit history tracking

**Story direction — ✅ DONE.** `crates/agent/src/story_direction.rs` implements:
- Capture human's creative vision
- Tone, style, themes, pacing, mood, audience
- Focus characters and special instructions
- Natural language parsing
- Consistent application throughout generation

**Chapter steering — ✅ DONE.** `crates/agent/src/chapter_steering.rs` implements:
- Pause generation mid-chapter
- Give direction while paused
- Resume with new direction
- See what's been generated so far
- Checkpoints for pause/resume

## Current Priorities

### Phase 2: Observability & Control — 🔴 CURRENT FOCUS

1. **action_logging** 🔴 — every action logged with timestamp and payload
2. **model_call_recording** 🔴 — every model call recorded (input, output, grammar, params)
3. **decision_tracing** 🔴 — every decision logged with reasoning
4. **debug_tools** 🔴 — inspect traces, replay actions, step through execution

### Phase 3: Reversibility & Versioning — ✅ IMPLEMENTED

5. **workspace_snapshots** ✅ — snapshot before any file change (`VersionControl::snapshot`)
6. **action_history** ✅ — complete history of all actions (`VersionControl::action_history`)
7. **undo_redo** ✅ — any action can be undone (`VersionControl::undo/redo`)
8. **rollback** ✅ — revert to any previous state (`VersionControl::rollback`)

### Phase 4: Human-AI Interaction — 🔴 FUTURE

9. **collaborative_outline** 🔴 — human and AI co-create outline
10. **natural_feedback** 🔴 — human gives feedback in natural language
11. **real_time_preview** 🔴 — show generation as it happens
12. **easy_revision** 🔴 — one-command revision with clear before/after
13. **story_direction** 🔴 — human sets tone, style, themes
14. **chapter_steering** 🔴 — steer chapter mid-generation

### Phase 5: Multiple Interfaces — 🔴 FUTURE

15. **cli_enhancements** 🔴 — better CLI with rich output
16. **tui** 🔴 — terminal UI with rich widgets
17. **web** 🔴 — browser-based UI with streaming
18. **api** 🔴 — REST/GraphQL API for programmatic access

## Model loading strategy

```
hardware scan → resolve model path → quantize for VRAM → build context → generate
```

- **Auto-resolution**: `$RWKV_MODEL` env var → first `rwkv7-*.st` under
  `models/` or `../models/` → error listing what was on disk.
- **Auto-quantization**: reads `Loader::info` for layer count + embedding,
  reads the on-disk FP16 file size as ground truth (wgpu's
  `max_buffer_size` over-reports on NVIDIA RTX 2050 by 200×).
  Policy: `< 1.5 GB` → no quant; `≥ 1.5 GB` (and `gpu_coop`) → NF4;
  `≥ 1.5 GB` (no `gpu_coop`) → Int8; otherwise no-quant.
- **Pipeline caches** under `/tmp/roco-pipeline-cache/` keyed by model
  hash speed up subsequent loads.

## Architecture map (the rwkv critical path)

Concrete request flow on the current code, end-to-end.

```
clap / napi / axum (entries)
  |
  v
crates/engine/src/eval::run_suite <- 10 default cases live here
crates/engine/src/backend::ModelBackend::complete <- trait, code-path-agnostic
  |
  v
crates/inference/src/backend::RwkvBackend::complete
  sends CompleteReq over mpsc::Sender
  |
  v
RwkvActor thread (crates/inference/src/actor.rs; LocalSet + current-thread tokio)
  owns Context, TokioRuntime, AnyState, token_stripper
  |
  v
web-rwkv::model::Model (vendored patch at vendor/web-rwkv/)
  |
  v
wgpu (Vulkan / DX12 / Metal / primary GPU backend)
  |
  v
BnfConstraint (crates/grammar/src/bnf.rs; bnf_sampler + qp-trie vocab)
  ← GBNF grammar string from caller (None → free-form)
  ← schoolmarm fallback for complex grammars
  |
  v
Response back over mpsc channel → RwkvBackend returns CompleteResponse
```

## Future Goals (Archived)

See `goals/future/index.md` for features that amplify a working core:
- FAISS graph vector embeddings
- Dreaming pipeline
- Self-training
- TUI/Web app/Dashboard
- Gateway/ORPC/NAPI/ZOD
- Browser use

These move back to active when the story engine works end-to-end with great human-AI interaction.
