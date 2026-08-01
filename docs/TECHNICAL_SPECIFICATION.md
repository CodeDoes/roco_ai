# RoCo AI — Technical Specification (Complete Reconstruction Blueprint)

> **STATUS: HISTORICAL SNAPSHOT** — Spec written on branch `arena/019fb78c-roco-ai` (base `edf13bf`), before the Phase 1-7 migration. Canonical architecture lives in **AGENTS.md** (§1-8); sections 5-16 here are superseded by it. Keep this file only as the reasoning record — do not use as living state.

> **Status:** Code-free architectural specification
> **Branch:** `arena/019fb78c-roco-ai` (base `edf13bf`)
> **Scope:** Every subsystem, design constraint, failure mode, and reasoning path required to recreate RoCo AI from first principles. No implementation code is shown; the reasoning behind every decision is stated explicitly.

---

## Table of Contents

1. [What This App Actually Is (Conceptual Identity)](#1-what-this-app-actually-is-conceptual-identity)
2. [The Physical Constraints That Shape Every Decision](#2-the-physical-constraints-that-shape-every-decision)
3. [Core Design Philosophy](#3-core-design-philosophy)
4. [Architectural Concept Map (Text Diagram)](#4-architectural-concept-map-text-diagram)
5. [The Two-Process Contract — Orchestration vs. Inference](#5-the-two-process-contract--orchestration-vs-inference)
6. [Why RWKV-7 (State-Space Model) — Not a Transformer](#6-why-rwkv-7-state-space-model--not-a-transformer)
7. [Engine Split — Pure vs. GPU](#7-engine-split--pure-vs-gpu)
8. [Workspace-Local Design — `.roco/` as Project State](#8-workspace-local-design--roco-as-project-state)
9. [Configuration Hierarchy — Priority & Overrides](#9-configuration-hierarchy--priority--overrides)
10. [The Creative Pipeline — Structured Phases as Quality Gates](#10-the-creative-pipeline--structured-phases-as-quality-gates)
11. [Agent Framework — Two Modes, One Reason](#11-agent-framework--two-modes-one-reason)
12. [Grammar & Token Constraints — BNF at the Decoder](#12-grammar--token-constraints--bnf-at-the-decoder)
13. [Session & State Management — The Vector as Memory](#13-session--state-management--the-vector-as-memory)
14. [State Pool — Bounded, Thread-Safe, LRU](#14-state-pool--bounded-thread-safe-lru)
15. [Atomic Inference Operations — Complete, Bake, Save, Load](#15-atomic-inference-operations--complete-bake-save-load)
16. [Inference Parameter Chemistry — Composable Knobs](#16-inference-parameter-chemistry--composable-knobs)
17. [Interface Surfaces — CLI, Desktop GUI, Gateway, Server](#17-interface-surfaces--cli-desktop-gui-gateway-server)
18. [Streaming & Rendering Pipeline — Monotonic Invariant & SSE Reassembly](#18-streaming--rendering-pipeline--monotonic-invariant--sse-reassembly)
19. [Conversation Context Budgeting — Whole Turns, Rolling Summary](#19-conversation-context-budgeting--whole-turns-rolling-summary)
20. [Session Tree Architecture — Sub-Sessions & Branch History](#20-session-tree-architecture--sub-sessions--branch-history)
21. [Sandbox Security — Path Boundaries & Tool Confinement](#21-sandbox-security--path-boundaries--tool-confinement)
22. [Quality, Verification & Rollback — Transactional Generation](#22-quality-verification--rollback--transactional-generation)
23. [Mock Backend — Offline Development & CI](#23-mock-backend--offline-development--ci)
24. [Tracing & Observability — Structured, Surface-Specific](#24-tracing--observability--structured-surface-specific)
25. [Desktop Pet / State Machine — Presentational Feedback](#25-desktop-pet--state-machine--presentational-feedback)
26. [Deployment Philosophy — Local-First, No Fallback](#26-deployment-philosophy--local-first-no-fallback)
27. [Version Control & Reversibility — Workspace Snapshots](#27-version-control--reversibility--workspace-snapshots)
28. [Failure Modes & Degradation Paths](#28-failure-modes--degradation-paths)
29. [Reproduction Checklist — From Zero to Story](#29-reproduction-checklist--from-zero-to-story)
30. [References — Source Documents & External Sources](#30-references--source-documents--external-sources)

---

## 1. What This App Actually Is (Conceptual Identity)

RoCo AI is **not** a chat bot, a code generator, or a generic LLM wrapper. It is a **structured collaborative writing environment** that uses a local recurrent language model (RWKV-7, 2.9B parameters, ~10k trained-context window) to turn a single premise into a multi-file workspace containing an outline, character wiki, sequentially generated chapters, validation reports, synthesized synopsis, and a final compiled story.

**The thinking:** Traditional generative AI produces scattered text in a single thread. Creative writing requires coherence across thousands of words, continuity of characters, and adherence to structural arcs. By wrapping the model in a pipeline with discrete phases — each phase feeding explicitly into the next — we transform a token predictor into an architecture for long-form narrative. The local-only constraint is deliberate: writers handle private material; remote APIs introduce latency variability, cost unpredictability, and data exposure risk.

**What the user actually does:**
- Types `roco story "A lighthouse keeper finds a hidden message in the fog"`
- Waits 3–5 minutes while the pipeline runs phases 1–6
- Receives 8 files under `.roco/workspaces/` plus `.roco/stories/`
- Can intervene at any phase (`--phase synopsis`, `--fix chapter 3`, `--resume`)
- Never sees the model, the daemon, or the grammar engine

---

## 2. The Physical Constraints That Shape Every Decision

Before explaining any subsystem, understand the hard limits that drive design:

| Constraint | Value / Behavior | Impact on Design |
|---|---|---|
| **Parameter count** | 2.9B | Too small for reliable long-context reasoning; needs external memory (wiki, outline) |
| **Context window** | ~10k tokens (trained) | Chapters must be generated sequentially; full novel cannot fit in state alone |
| **Repetition threshold** | ≥0.7 temperature causes rapid repetition | Temperature must stay low (≤0.5) for structured phases; slightly looser only for prose |
| **VRAM footprint** | Model loaded once; constant per token | Enables long sequences; makes session-state caching practical |
| **Inference mode** | Recurrent (RNN-style) at decode time; parallel only at training | Session state is a float vector, not a conversation history |
| **Offline requirement** | No remote API fallback permitted | All artifacts must be local; failure = explicit error, not silent cloud switch |
| **Hardware target** | Consumer GPU (8–16 GB VRAM typical) | Model split, mock mode, and bounded caches are required |

**The thinking:** All design choices are responses to these constraints. The 2.9B size explains why grammar is mandatory for structured output; the 10k window explains why the workspace holds explicit memory (wiki files) rather than relying on the model to remember everything; the recurrence explains why session persistence is a vector save/load rather than a transcript replay; the repetition threshold explains why temperature is exposed as a knob rather than hidden behind a slider.

---

## 3. Core Design Philosophy

Five principles govern every layer:

1. **Local-first, offline-mandatory.** No cloud fallback. The model weights, session states, and generated stories never leave the machine.
2. **Orchestration is not inference.** The engine knows nothing about stories; the frontend knows nothing about Vulkan or state tensors. The only contract is the HTTP request/response format.
3. **Phase separation over monolithic generation.** A model that cannot reliably hold 10k tokens must be given focused prompts with external memory.
4. **Hard constraints over prompt engineering.** BNF grammar masks enforce format at the token decoder, because a 2.9B model is too unreliable for pure prompt-based structuring.
5. **Graceful degradation, not catastrophic failure.** Every phase has verification, rollback, and mock-mode alternatives.

---

## 4. Architectural Concept Map (Text Diagram)

```
User Input (premise / command / steer direction)
         |
         v
[ CLI / Desktop GUI / Gateway / Server ]
         |  (constructs AppContext; hides backend/daemon details)
         v
[ AppContext ]  ←  shared primitive; connects to backend + workspace + session
         |
    +----+----+
    |         |
    v         v
[ Agent ]  [ Workspace / Session ]
    |         |
    |    (file persistence, versioning, trace logs)
    |
    +---- MechanisticAgent (structured phases, plan-first, verification)
    +---- CommonAgent (ReAct, tool use, interactive steering)
         |
    +---- Grammar / Validation (BNF mask, verifiers, rollback)
    |
    v
[ Engine (pure traits + mock) ]
    |
    +---- Engine-GPU (Vulkan / web-rwkv, state tensors, tokenization)
    |
    v
[ Inferd Daemon — HTTP interface ]
    |  Operations: Complete | Bake | SaveState | LoadState
    |  State pool: HashMap<String, Option<Tensor>> (max 8, LRU)
    v
[ Generated Text + Updated State Vector ]
    |
    v
Back through Validation → Workspace File Write → User Display
```

**Key insight:** The data flow is bidirectional but asymmetric. Downstream (user → workspace), the flow is rich — files, steering, versions, traces. Upstream (workspace → user), the flow is a text response plus a saved state. The inference path is narrow by design: it only moves text + state vectors across HTTP.

---

## 5. The Two-Process Contract — Orchestration vs. Inference

### Orchestrator (Frontend)
- Decides what text to generate
- Picks grammar, temperature, max_tokens, stop sequences
- Manages workspace files, session trees, agent loops
- Formats output (markdown, JSON, compiled story)

### Inferd (Inference Daemon)
- Receives text, state_id, grammar, temperature, max_tokens, stop tokens
- Loads state from pool (or blank), feeds text through model, generates tokens
- Applies grammar mask at decoder level
- Saves resulting state to pool (or discards if save_as is None)
- Returns only the generated text string

**The thinking:** If the inference engine handled stories, it would need to know about file paths, chapter numbering, wiki formats, and validation rules — making it impossible to swap for a different model or deploy on a server without all that context. By reducing the contract to “text in, text + state out,” we achieve complete separation of concerns. The daemon can be restarted, upgraded, or moved to another host; the frontend only needs the HTTP endpoint.

---

## 6. Why RWKV-7 (State-Space Model) — Not a Transformer

RWKV-7 (codename “Goose”) is a Receptance-Weighted Key-Value model that operates in two modes:
- **Parallel (training):** computes all token outputs simultaneously, like a Transformer.
- **Recurrent (inference):** processes one token at a time using only the previous hidden state.

### The State Vector
The “session” in RWKV is not a conversation history. It is a float vector (the model’s hidden recurrence state after processing N tokens). When you save `state_slot = "chapter_3_resume"`, you save that float vector to disk. When you load it, you inject it into the model before generating the next token — continuation is instantaneous, regardless of how long the previous text was.

**Why this matters for writing:**
- A Transformer needs to replay all previous tokens (or use a KV cache that grows with length) to continue a story.
- RWKV needs only the state vector — memory is constant, compute is linear, and resume time does not grow with chapter length.
- This makes long-form chapter sequencing practical on consumer hardware.

### Linear Complexity
RWKV avoids the quadratic attention matrix of Transformers. For a sequence of length T:
- Transformer attention: O(T²) time, O(T²) memory (or O(T) memory with KV cache, but still quadratic compute for long contexts)
- RWKV inference: O(T) time, O(1) additional memory per step (only the state vector)

**The thinking:** A writer generating an 8-chapter novel quickly exceeds practical Transformer windows. RWKV’s recurrent nature turns context management into state management — which aligns perfectly with the pipeline’s need to save, resume, and steer chapters independently.

---

## 7. Engine Split — Pure vs. GPU

| Crate | Dependencies | Contains |
|---|---|---|
| `engine` | Pure Rust, no GPU | Traits (`ModelBackend`), types (`CompletionRequest`, `TokenizeResult`), mock backend, grammar strategies, JSON helpers, eval framework |
| `engine-gpu` | Vulkan (`web-rwkv`), WGPU | `RwkvActor` (tokio actor managing model), `RwkvBackend` (implements `ModelBackend`), tokenization, state tensor handling |

**The thinking:** If GPU dependencies were required for every build, the project could not compile in CI, could not generate documentation, and could not run tests on laptops without discrete GPUs. The split ensures that `engine` compiles everywhere. Tests against the mock backend validate the entire pipeline logic without a model load. The GPU crate only enters the dependency graph when `engine-gpu` is explicitly included — which happens when real inference is needed.

---

## 8. Workspace-Local Design — `.roco/` as Project State

Every command operates relative to the current working directory. All artifacts go into `.roco/` under that directory:

```
.roco/
├── config.toml              # model path, ports, template settings
├── agent-journal.md         # runtime log (migrating to JSONL)
├── trace.log                # global timeline, all events
├── roco.log                 # structured tracing output
├── workspaces/
│   └── {timestamp}_{slug}/
│       ├── 01-OUTLINE.md
│       ├── 02-WIKI.md
│       ├── 03-CHAPTER_*.md
│       ├── 04-VALIDATION.md
│       ├── 05-SYNOPSIS.md
│       └── 06-STORY.md
├── stories/
│   └── {slug}.md            # compiled final story
└── sessions/
    └── {session_id}/
        ├── session.log
        ├── meta.json
        ├── history-{branch}.jsonl
        └── trace.txt
```

**The thinking:** Global directories (`~/.config/roco/`, `~/.local/share/roco/`) leak across projects and make archiving impossible. By scoping everything to `.roco/`, a writer can:
- `tar` one folder to archive an entire project
- `rm -rf .roco/` to reset without affecting other work
- Run multiple story projects in parallel (different directories = isolated states)
- Version-control the workspace with external git (if desired)

The `ROCO_DIR` environment variable overrides this for headless/CI/docker environments where `cwd` is meaningless.

---

## 9. Configuration Hierarchy — Priority & Overrides

Config loading follows strict priority:

1. `$ROCO_CONFIG` (explicit file path)
2. `.roco/config.toml` (current directory, project-scoped)
3. `~/.config/roco/config.toml` (XDG standard, user-scoped)
4. `~/.roco/config.toml` (legacy dotfile, backward compatibility)

**Override shield:** Any environment variable explicitly set (`RWKV_MODEL`, `RWKV_VOCAB`) takes absolute precedence over any file value.

**The thinking:** A writer should not edit a global config to switch between projects. Project-scoped config lives with the work. CI servers can inject paths via environment rather than editing files. The override shield prevents temporary test configurations from accidentally becoming persistent.

---

## 10. The Creative Pipeline — Structured Phases as Quality Gates

The story pipeline executes phases sequentially. Each phase writes files to `.roco/workspaces/{slug}/`.

### Phase Sequence

| Phase | Output File | Purpose | Strategy (typical) |
|---|---|---|---|
| **Outline** | `01-OUTLINE.md` | Premise → chapter arcs | Schema / strict grammar |
| **Wiki** | `02-WIKI.md` | Characters, world rules, continuity | State-tuned or loose JSON |
| **Chapter N** | `03-CHAPTER_N.md` | Narrative text, sequential | Low temp, state-tuned, long max_tokens |
| **Validation** | `04-VALIDATION.md` | Quality check vs outline/wiki | Schema grammar |
| **Synopsis** | `05-SYNOPSIS.md` | Compressed summary of draft | State-tuned |
| **Story** | `06-STORY.md` | Final compiled document | Low temp, format-predefined |

### Human Steering Points
At any phase, the user can:
- Edit files directly (outline, wiki, chapter drafts)
- Use `--fix chapter 3` to regenerate a single chapter using current workspace state
- Use `--phase synopsis` to skip ahead to that phase from current state
- Use `--resume` to find the latest workspace and continue
- Use `--steer` to inject direction mid-generation (chapter steerer with pause/resume)

**The thinking:** A 2.9B model can generate coherent paragraphs but struggles with plot consistency over 2,000+ words. By splitting creation, each prompt is focused: “Generate chapter 3 given the wiki and outline.” The external workspace holds the long-range memory (character names, world rules, previous chapter events) that the model cannot reliably maintain in its recurrent state alone. Human steering ensures that if the model drifts, the user can correct it before corruption propagates to later phases.

---

## 11. Agent Framework — Two Modes, One Reason

The agent layer has two implementations:

### MechanisticAgent (Plan-First)
- Builds an explicit plan before acting
- Executes steps sequentially with verification checkpoints
- Supports rollback if a step fails verification
- Used for structured phases (outline, validation, synopsis, chapter generation with targets)

### CommonAgent (ReAct / Chat)
- Open-ended conversation loop
- Uses tools (file read, bash, edit, search, write, edit) based on natural language
- No rigid plan; reacts to user input
- Used for interactive steering, commentary, and open-ended exploration

**The thinking:** A single agent design cannot serve both rigorous structured output (where correctness is verifiable) and flexible conversation (where the user might change direction). Separating them lets the pipeline use the right mode per phase. The mechanistic agent supports the pipeline’s quality gates; the common agent supports human collaboration.

---

## 12. Grammar & Token Constraints — BNF at the Decoder

The system uses BNF (Backus-Naur Form) grammar strings compiled through a `kbnf`-like mechanism to create token-level masks.

### Strategy Presets
- **Schema:** Strict JSON grammar (e.g., valid JSON object with required fields). Forces exact format.
- **Loose-JSON:** Relaxed grammar accepting JSON-like output with some tolerance.
- **State-tuned:** No grammar; rely on recurrent state primed by baking few-shot examples through the model.
- **Grammar:** User-provided custom BNF string.

**How it works conceptually:** The grammar defines legal token sequences. At each generation step, the decoder only considers tokens that keep the output valid according to the grammar. This is not post-processing — it is a hard constraint during generation.

**The thinking:** At 2.9B parameters, prompt engineering alone is unreliable. The model will occasionally emit prose instead of JSON, forget closing braces, or invent fields. A grammar mask eliminates these failure modes physically: the model literally cannot emit an invalid token. The trade-off is rigidity — inside the grammar, the model has less creative freedom. This is acceptable for metadata phases (outline, wiki, validation) and is why narrative chapters use more flexible strategies (low temp, state-tuned) rather than strict JSON.

---

## 13. Session & State Management — The Vector as Memory

In RWKV, session persistence is not text replay. It is explicit state management:

- **Load:** `state_id` points to a cached tensor (or blank if None)
- **Generate:** Model runs with loaded state; tokens are produced
- **Save:** `save_as` names the slot for the resulting state (or None to discard)
- **Bake:** Feed text through model with no generation; save resulting state. Used to prime format expectations.

### Why This Changes Everything
Traditional LLM APIs manage context by including the full conversation history in every request. For a 10k-token conversation, that is 10k tokens of input per request — growing linearly with conversation length. RWKV manages context with a fixed-size state vector: resume is constant-time regardless of previous length.

**The thinking:** For a writer resuming chapter 3 after a day, replaying chapters 1–2 (potentially thousands of tokens) is slow and wasteful. Loading a saved state makes resume nearly instantaneous. It also means session storage is compact (a float tensor, not a text transcript), enabling many saved states within bounded memory.

---

## 14. State Pool — Bounded, Thread-Safe, LRU

Inferd maintains a `HashMap<String, Option<Tensor>>` representing cached states.

- **Max entries:** 8
- **Eviction:** FIFO + LRU combined
- **Access:** `Arc<RwLock<...>>` for thread-safe concurrent access
- **Keys:** User-provided strings (`state_slot` names)

**The thinking:** Unbounded caches grow until they exhaust VRAM or disk. A writer might generate dozens of chapters over hours; keeping every state in memory is unsustainable. The 8-entry cap is a design compromise: large enough for a typical pipeline (outline, several chapter states, resume state, validation state, backups) but small enough to stay resident. The LRU ensures frequently accessed states (current chapter resume, latest wiki) stay available.

---

## 15. Atomic Inference Operations — Complete, Bake, Save, Load

Inferd exposes atomic operations rather than partial steps:

| Operation | Inputs | Outputs | Use Case |
|---|---|---|---|
| **Complete** | text, init_state, state_slot, grammar, temp, max_tokens | generated text + saved state | Normal generation |
| **Bake** | text, init_state, state_slot | saved state (text not returned) | Priming / state tuning |
| **SaveState** | state_slot, tensor | confirmation | Explicit persistence |
| **LoadState** | state_slot | tensor | Explicit retrieval |

**The thinking:** Partial operations (load state, generate, crash before save) corrupt session continuity. Atomicity ensures the state is either fully updated or unchanged. Bake is critical for the “state-tuned” strategy: feeding a JSON snippet through the model without generating primes the recurrent state with format expectations, reducing the need for long prefix prompts that consume context.

---

## 16. Inference Parameter Chemistry — Composable Knobs

No hidden “quality slider.” The pipeline exposes explicit knobs:

- **Temperature:** 0.0 (greedy, deterministic) to 1.0 (random). At ≥0.7 in this model, repetition accelerates rapidly.
- **Top-p (nucleus):** Only sample from tokens whose cumulative probability exceeds p.
- **Top-k:** Only sample from the k highest-probability tokens.
- **Prefill:** Initial token sequence to jump-start output format (e.g., `"{\n"` for JSON).
- **Stop:** Token sequences that halt generation (e.g., `"}\n"`).
- **Max tokens:** Hard limit on output length.
- **Grammar:** BNF mask applied at decoder.

**Composition, not presets:** The `--strategy` flag selects a preset (state-tuned, schema, loose-json, grammar), but each preset is just a composition of the knobs above. Users and pipeline phases can override individual values.

**The thinking:** Small models are highly sensitive to sampling parameters. Hiding them behind a single slider makes debugging impossible when output drifts. By exposing all knobs, the pipeline can be conservative (low temp, strict grammar) for structured data and slightly looser for prose, with clear accountability for each choice.

---

## 17. Interface Surfaces — CLI, Desktop GUI, Gateway, Server

All surfaces share `AppContext` and call the same agent/workspace pipeline.

- **CLI (`crates/cli`):** Terminal-first. Commands: `story`, `interact`, `eval`, `desktop`, `server`, `router`, `inspect`. Supports `--phase`, `--fix`, `--resume`, `--mock`, `--strategy`, `--pace`.
- **Desktop GUI (`crates/ui`):** egui-based widgets — chat panel, markdown editor, session browser, desktop pet state machine. Uses the same `AppContext` but renders incrementally.
- **Gateway (`crates/gateway`):** HTTP router. Constructs requests, passes to `AppContext`. Thin layer — contains no business rules.
- **Server (`crates/server`):** HTTP server with routes for external tools / editor plugins.

**The thinking:** Writers have different workflows. Some prefer terminal speed; some want a visual workspace with chapter browsers; some integrate with editors via HTTP. Rather than duplicating logic, all surfaces construct `AppContext` once and call capability methods. The gateway is thin by design — if it contained business rules, updating the pipeline would require updating both frontend and gateway.

---

## 18. Streaming & Rendering Pipeline — Monotonic Invariant & SSE Reassembly

Earlier versions had critical streaming bugs:
- Callbacks wrote to buffers never read (user watched a spinner, then got entire text at once)
- SSE chunks split events at arbitrary byte boundaries (lines split across chunks dropped data)
- Responses printed twice (once via callback, once after return)

The fix introduces:

### StreamPrinter (Monotonic Invariant)
- Keeps raw token stream
- Re-renders visible text after each delta
- Writes only bytes not yet written
- **Monotonic prefix:** the rendered text never shrinks (critical for incremental display safety)

### SSE Line Buffer
- Carries partial lines across HTTP chunks
- Asserts nothing is lost when splitting at every byte offset
- Fixes the 73-of-78 split-point failure rate of the old reader

### Filtering
- Hides `<think>...</think>` blocks
- Holds back trailing partial markers (half-received `<thi` never flashes)
- Cuts at hallucinated `\nUser:` (most common failure mode of raw completion in chat loops)
- Buffers `{"result": ...}` MockBackend envelopes until complete

**The thinking:** Users expect live feedback. But incremental rendering is hard because tokens split markers, and HTTP chunk boundaries are arbitrary. The monotonic invariant makes incremental output safe; the line buffer fixes a real protocol-level data loss bug; filtering prevents the model from displaying its own reasoning or continuing a fake conversation.

---

## 19. Conversation Context Budgeting — Whole Turns, Rolling Summary

The chat session (`ChatSession`) manages context with two rules:
- **Whole-turn admission:** messages are included whole or not at all (no truncation of individual turns)
- **Character budget:** turns admitted newest-first while they fit a budget (e.g., 8000 chars, max 16 turns)
- **Rolling summary:** evicted turns are folded into a bounded rolling summary rather than truncated individually

Earlier design truncated each message to 300 chars — destroying long replies and making follow-up impossible.

**The thinking:** A writer might ask, “Can you adjust chapter 3 based on the new wiki entry?” If the wiki change was truncated away, the request fails. Whole-turn preservation ensures coherence. The rolling summary ensures graceful degradation instead of abrupt forgetting — the conversation gets compressed but never mutilated.

---

## 20. Session Tree Architecture — Sub-Sessions & Branch History

Sessions form a tree, not a flat list:

```
Root Session (abc123)
  ├── Sub-Session (def456) — agent analysis
  │     └── Joins back to Root with result
  ├── Sub-Session (ghi789) — chapter steering
  └── Branch history files: history-branch.jsonl
```

Files per session:
- `.roco/sessions/{id}/session.log` — conversation turns at that level
- `.roco/trace.log` — global timeline of all events
- `.roco/sessions/{id}/meta.json` — parent_id, session_type, active_branch
- `.roco/sessions/{id}/history-{branch}.jsonl` — branch checkpoints
- `.roco/sessions/{id}/trace.txt` — raw I/O transcript

**The thinking:** A flat conversation history cannot represent agent sub-tasks (e.g., “analyze wiki consistency” as a child of the main story session). Tree architecture lets sub-sessions write their own logs and rejoin with results, while the global trace provides a unified reconstruction view. This mirrors how a writer works: they have a main draft, occasional research notes, and steering directions — not a single linear chat.

---

## 21. Sandbox Security — Path Boundaries & Tool Confinement

When agents use tools (file read, bash, edit, search, write), they operate inside a `Workspace` with `is_safe_relative_path` checks. Tools cannot access paths outside the workspace directory.

**The thinking:** Agents that can run arbitrary bash are dangerous. A writer’s project may contain credentials, private notes, or system files. Path boundary checks ensure that even if the model hallucinates a malicious command (`rm -rf /`), it is confined to the project directory. The workspace abstraction also enables reversible actions: versions, snapshots, and rollback.

---

## 22. Quality, Verification & Rollback — Transactional Generation

Every phase includes verification layers:
- **Grammar verification:** output must match BNF
- **Classic verifiers:** forbidden/required words, minimum length
- **Inference verification:** format matches expected shape
- **Outline verification:** chapters match outline structure
- **Wiki verification:** continuity rules hold

If verification fails, the pipeline rolls back to the previous state and retries (up to `max_retries`, default 3). State is restored from the saved vector.

**The thinking:** The model is unreliable by design — it is a 2.9B statistical model trained on general text, not a reliable structured-data engine. Without verification and rollback, a single bad generation (e.g., a chapter that forgets a character’s motivation, or a JSON response that misses a field) would corrupt the entire workspace. Treating each phase as a transaction (generate → verify → accept/rollback) makes the pipeline robust enough for production use.

---

## 23. Mock Backend — Offline Development & CI

`MockBackend` implements the same `ModelBackend` interface as the real GPU backend. It generates deterministic outputs from format strings and supports full pipeline testing, including streaming and grammar paths.

**The thinking:** Not every developer has an 8 GB GPU, and downloading a multi-gigabyte model takes time. Mock mode allows users to test command syntax, workspace behavior, agent loops, and UI flows instantly. It ensures CI passes without GPU access. The interface parity means switching between mock and real requires only configuration — no code changes.

---

## 24. Tracing & Observability — Structured, Surface-Specific

All crates use `tracing` spans instead of `println!`. Subscriptions are configured per surface:
- **CLI / TUI:** info/debug writes to `.roco/roco.log` or stderr; never overlaps with user-facing natural language
- **Daemon:** serialized to disk for offline diagnosis
- **GUI:** filtered to avoid polluting interface

**The thinking:** A writer’s terminal should show the story, not debugging noise. Structured tracing allows developers to diagnose failures (e.g., “why did chapter 3 fail validation?”) without exposing trace statements to users. The subscriber routing is configurable per surface, so a headless server can log aggressively while an interactive GUI stays silent.

---

## 25. Desktop Pet / State Machine — Presentational Feedback

The desktop GUI includes a “pet” widget that operates as a state machine: idle, typing, thinking, error, happy, sleep. States transition based on pipeline events (generation started, verification passed, error occurred, user input received).

**The thinking:** Creative tools can feel mechanical. A lightweight visual state machine gives ambient feedback about what the system is doing without requiring users to read logs. It is purely presentational — it does not influence inference — keeping UI concerns separate from pipeline concerns.

---

## 26. Deployment Philosophy — Local-First, No Fallback

The protocol enforces:
- Mandatory local model path (`.st` weights or equivalent)
- `roco-inferd` local HTTP daemon only
- Explicit error if model or daemon is missing
- No cloud API fallback permitted

**The thinking:** Writing is private. A hidden cloud fallback would violate expectations, introduce variable latency, and cost money. Making failure explicit (clear error message) rather than silent (fallback that changes behavior) ensures users understand what to fix. The local daemon keeps data off remote servers — weights, session states, stories never leave the machine.

---

## 27. Version Control & Reversibility — Workspace Snapshots

The workspace module provides:
- `Snapshot` — captured state of workspace at a point
- `ReversibleAction` — an action that can be undone
- `SnapshotSummary` — metadata about captured state
- `VersionControl` — tracking of changes

**The thinking:** Generative AI is destructive — it overwrites files, changes outlines, deletes drafts. Without reversibility, a bad generation or an accidental user command is permanent. Internal versioning (not solely relying on external git) ensures recovery at every step, which is especially important when the underlying model is unreliable.

---

## 28. Failure Modes & Degradation Paths

| Failure | Cause | Degradation / Recovery |
|---|---|---|
| **Repetition** | Temperature ≥0.7; model loops on patterns | Lower temperature; use strict grammar; bake example tokens |
| **Format escape** | Model outputs prose instead of JSON | Grammar mask enforces format; if fails, rollback and retry |
| **Truncation** | Max tokens reached; model stops early | Increase max_tokens; use stop sequences; resume with saved state |
| **State corruption** | Crash during generation; partial save | Atomic operations prevent partial states; load previous checkpoint |
| **Daemon disconnect** | Inferd stopped; model unloaded | Auto-restart chain via `AppContext`; mock mode available |
| **Context overflow** | Conversation exceeds budget | Rolling summary compresses old turns; whole-turn policy prevents mutilation |
| **Grammar escape** | Model finds token path outside BNF | BNF mask prevents this at decoder; if bug in mask, verification catches output |

**The thinking:** The pipeline is designed for the worst case because the model is unpredictable at 2.9B. Every phase has explicit verification and retry loops; failures surface as **loud errors, never silent fallbacks** (per AGENTS.md §9 — the no-fallback rule). Mock mode is an explicit user choice, not an automatic degradation.

---

## 29. Reproduction Checklist — From Zero to Story

To recreate RoCo AI from this specification:

1. **Environment:** Install Rust (edition 2021), Vulkan SDK, and `web-rwkv` dependencies.
2. **Model:** Obtain RWKV-7 2.9B weights (`.st` format). Place according to `RWKV_MODEL` or `.roco/config.toml`.
3. **Build:** Compile pure `engine` crate first; then `engine-gpu`; then workspace.
4. **Config:** Create `.roco/config.toml` with model path, inference port, gateway port.
5. **Mock test:** Run `roco story --mock "Test premise"` to verify pipeline logic without model.
6. **Daemon start:** Launch `roco-inferd` (or let `AppContext` auto-start via `daemon.rs`).
7. **First story:** Run `roco story "A premise"`. Verify `.roco/workspaces/` files appear.
8. **Steer:** Edit `03-CHAPTER_1.md`; use `--fix chapter 1` to regenerate with updates.
9. **Resume:** Delete `.roco/` partially; use `--resume` to continue from latest workspace.
10. **Archive:** `tar` `.roco/`; move to storage; verify state files are preserved.

---

## 30. References — Source Documents & External Sources

### Internal Source Documents (from this repository)
- `AGENTS.md` — System onboarding; architecture overview; pipeline descriptions
- `docs/CLI_STREAMING_AND_LEAKS.md` — Streaming bugs, SSE reassembly, conversation fixes
- `docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` — Conceptual audit; dual-agent paradox; state-tuning; tracing
- `docs/TMUX_ROCO_GUIDE.md` — Operational guide
- `docs/rfc/0001-local-ai-harness.md` — Harness trait spec
- `docs/rfc/0008-offline-inference-protocol.md` — Offline protocol rules
- `USE_CASES_AND_GAPS.md` — Crate usage mapping; missing test analysis
- `SESSION_INSTRUCTIONS.md` — Operating constraints (non-technical user, worst-case build, natural language interface)
- `CHANGELOG.md` — Migration history; validation module updates

### Web / Academic Sources (retrieved during specification)
- ArXiv `2504.14260v1` — RWKV-7 cross-attention, state sensitivity, prompt stability, linear scaling (2025)
- CallSphere (2026) — RWKV vs Mamba vs Transformer comparison; linear attention; RNN inference mode
- Internet-Pros (2026) — RWKV-7 “Goose” 14B/32B multilingual; hybrid architectures; state-space evolution
- Blog.gopenai — RWKV mechanism (time mixing, channel mixing, parallel vs recurrent modes)
- ArXiv `2504.08247v1` — RWKV-7 state-driven architecture; Meta-State layer; state-autoregressive framework

---

*End of Specification*

> **Note:** This document is current to branch `arena/019fb78c-roco-ai`. All reasoning paths, design constraints, and failure-mode analyses are derived directly from the codebase architecture (`crates/cli`, `crates/app`, `crates/engine`, `crates/inferd`, `crates/agent`, `crates/workspace`, `crates/session`, `docs/`, `AGENTS.md`) and from external technical sources regarding RWKV-7 state-space model behavior, inference characteristics, and scaling properties (2025–2026).
