# RoCo AI Goals

Roadmap for a local AI agent system that competes with large models through clever architecture.

## Core Philosophy: The Mecha-Agent Pattern

**The mecha-agent is not pretending to be a big model. It's doing something smarter.**

Large models (GPT-4, Claude, etc.) can do everything in one call because they have billions of parameters. The mecha-agent has 2.9B parameters and can't do that. Instead, it uses:

1. **Grammar-constrained decoding** — every output is structurally guaranteed
2. **Code-owned control flow** — the model never decides what happens next
3. **Multi-step decomposition** — complex tasks broken into simple steps
4. **Self-verification** — check output quality before accepting
5. **Repair loops** — retry with tighter constraints on failure
6. **Context management** — pull relevant context, not push everything
7. **Session state** — remember what happened across calls
8. **Incremental delivery** — one task at a time, human controls pace

This is not a limitation. It's an advantage. The mecha-agent is:
- **Predictable** — you know exactly what it will do
- **Debuggable** — every step is logged and reversible
- **Controllable** — human controls pace, not burdened with review
- **Efficient** — uses small model for simple tasks
- **Transparent** — you can see the reasoning, not just the output

## Design Principle: Human Controls Pace, Not Reviews Output

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

**But also:** Let the human choose their level of involvement:
- **Full control** — one task at a time, human reviews each one
- **Moderate control** — batch of tasks (e.g., 3 chapters), human reviews batch
- **No control** — agent runs to completion, human reviews at end
- **"Go ham" mode** — agent runs without stopping, local model so tokens are free

Speed is the bottleneck, not tokens. Let the human choose.

## Architecture: Modes, Routes, and Handlers

The mecha-agent is a **mode-based system**. Each mode defines:
- What the agent can do (routes)
- How it does it (handlers)
- What grammar constrains output (grammars)
- What tools are available (tool registry)
- How to verify quality (evaluators)

### Current Modes
- **storyTeller** — story generation (outline → wiki → chapter → publish)
- **coder** — code generation (plan → implement → test → lint)
- **researcher** — research (search → read → summarize → cite)
- **justChatting** — safe fallback (no tools, just conversation)

### Mode Architecture

```
User Input
    ↓
Intent Classifier (grammar-constrained)
    ↓
Route Selector (mode + route)
    ↓
Plan Generator (grammar-constrained)
    ↓
Task Executor (code-owned loop)
    ↓
── ONE TASK AT A TIME ──────────────────────────────
    ↓
Handler Dispatch (per-task handler)
    ↓
Quality Verifier (grammar-constrained, automatic)
    ↓
Result Committer (reversible, logged)
    ↓
Output to User
    ↓
User Decision: accept / modify / skip / stop
    ↓
(if accept) → Next Task
(if modify) → Revise Task
(if skip) → Jump to specified task
(if stop) → Publish and exit
```

## Core Principles

### 1. One Task at a Time

The agent completes one task, then stops and shows the result:
- Human sees what was done
- Human decides what to do next
- No mandatory review — just natural flow
- Batch size is configurable (1, 3, 5, all)

### 2. Every Action is Reversible

Every change the agent makes is versioned:
- **Workspace snapshots** — before any file change, snapshot the current state
- **Action log** — every action is recorded with timestamp, type, and payload
- **Undo/redo** — any action can be undone
- **Rollback** — revert to any previous state

### 3. Every Decision is Logged

Every model call, every decision, every action is traced:
- **Model calls** — input, output, grammar, temperature, tokens
- **Decisions** — what was decided, why, alternatives considered
- **Actions** — what was done, where, when
- **Quality** — scores, issues, suggestions

### 4. Quality is Automatic, Not Manual

Quality checks happen automatically, not by human review:
- **Grammar gate** — does output match the grammar?
- **Schema gate** — does output parse to expected type?
- **Quality gate** — does output meet quality thresholds?
- **Consistency gate** — does output contradict previous outputs?

Human only intervenes if they want to, not because they have to.

### 5. Multiple Interfaces, Same Core

The mecha-agent core is interface-agnostic:
- **CLI** — command-line interface (current)
- **TUI** — terminal UI with rich widgets (planned)
- **Web** — browser-based UI with streaming (planned)
- **API** — REST/GraphQL API for programmatic access (planned)

All interfaces use the same core: same modes, same handlers, same quality gates.

## Layers

Layers are listed in **prerequisite order** — each layer depends on the ones above it.

### Layer 1: Inference Engine
- Model loading, quantization, inference
- Grammar-constrained decoding
- State save/load/mix

### Layer 2: Agent Core
- Mecha-agent pattern (code-owned control flow)
- Mode system (routes, handlers, grammars)
- Intent classification and routing
- Plan generation and execution

### Layer 3: Quality & Verification
- Quality metrics and scoring
- Model-as-judge evaluation
- Repair loops and self-correction
- Consistency checking

### Layer 4: Observability
- Action logging and tracing
- Model call recording
- Decision audit trail
- Debug and interpretability tools

### Layer 5: Persistence & Versioning
- Workspace snapshots
- Action history
- Undo/redo system
- Session persistence

### Layer 6: Interfaces
- CLI (current)
- TUI (planned)
- Web (planned)
- API (planned)

### Layer 7: Modes & Capabilities
- Story mode
- Code mode
- Research mode
- Chat mode

## Product Roadmap

### Phase 1: Core Engine (DONE)
1. ✅ **inference** — RWKV-7 backend with grammar-constrained decoding
2. ✅ **agent_core** — mecha-agent pattern with mode system
3. ✅ **story_engine** — dynamic outline, plot state, context assembly
4. ✅ **quality** — model-as-judge evaluation, revision support

### Phase 2: Observability & Control (DONE)
5. ✅ **action_logging** — every action logged with timestamp and payload
6. ✅ **model_call_recording** — every model call recorded (input, output, grammar, params)
7. ✅ **decision_tracing** — every decision logged with reasoning
8. ✅ **debug_tools** — inspect traces, replay actions, step through execution

### Phase 3: Reversibility & Versioning (DONE)
9. ✅ **workspace_snapshots** — snapshot before any file change
10. ✅ **action_history** — complete history of all actions
11. ✅ **undo_redo** — any action can be undone
12. ✅ **rollback** — revert to any previous state

### Phase 4: Commentary & Writing Assistance (DONE)
13. ✅ **bidirectional_commentary** — agent explains decisions, human annotates
14. ✅ **writing_analysis** — themes, characters, tone, style, sentiment
15. ✅ **continuation_suggestions** — AI suggests how to continue writing
16. ✅ **fill_in_middle** — AI fills gaps between passages
17. ✅ **diff_analysis** — AI analyzes changes between versions
18. ✅ **cross_referencing** — AI finds connections with existing content

### Phase 5: Human-AI Interaction (DONE)
19. ✅ **collaborative_outline** — human and AI co-create outline
20. ✅ **natural_feedback** — human gives feedback in natural language
21. ✅ **real_time_preview** — show generation as it happens
22. ✅ **easy_revision** — one-command revision with clear before/after
23. ✅ **story_direction** — human sets tone, style, themes
24. ✅ **chapter_steering** — steer chapter mid-generation

### Phase 6: Multiple Interfaces (MOSTLY DONE)
25. **cli_enhancements** — better CLI with rich output
26. **tui** — terminal UI with rich widgets
27. ✅ **web** — browser-based UI with streaming (ProseMirror editor created)
28. ✅ **api** — REST/GraphQL API for programmatic access (story routes implemented)

### Phase 7: Advanced Capabilities
29. **dreaming** — offline memory consolidation
30. **self_training** — learn from feedback
31. **self_prompt_adjustment** — optimize prompts automatically
32. **multi_agent** — multiple agents collaborating

## Per-Layer Status

| Layer | Status | Next self-directed action |
|---|---|---|
| **inference** | ✅ 2.9B path complete | close per-handler grammar gap |
| **agent_core** | ✅ mecha-agent done | wire story mode as first-class mode |
| **story_engine** | ✅ core done, all features done | integrate into mecha-agent as mode |
| **quality** | ✅ model-as-judge done | wire into all handlers |
| **observability** | ✅ done | action logging, model call recording, traces |
| **reversibility** | ✅ done | workspace snapshots, undo/redo, rollback |
| **commentary** | ✅ done | bidirectional agent/human commentary |
| **writing_assistant** | ✅ done | continuation, fill-middle, analysis |
| **interaction** | ✅ done | interactive and automatic modes |
| **natural_feedback** | ✅ done | parse natural language into directives |
| **outline_editing** | ✅ done | collaborative outline creation |
| **story_direction** | ✅ done | capture and apply creative vision |
| **chapter_steering** | ✅ done | pause/resume/steer mid-generation |
| **interfaces** | 🔴 **CURRENT FOCUS** | CLI enhancements, then TUI, Web, API |
| **advanced** | 🔴 not started | dreaming, self-training |

## Meta-layers

- **engineering** — hygiene, warnings, clippy, determinism, no flaky tests
- **observability** — traces, logs, metrics, debugging tools
- **human_first** — every interaction should feel natural
- **reversibility** — every action should be undoable

## Future Goals (Archived)

See `goals/future/index.md` for features that amplify a working core:
- FAISS graph vector embeddings
- TUI/Web app/Dashboard
- Gateway/ORPC/NAPI/ZOD
- Browser use
