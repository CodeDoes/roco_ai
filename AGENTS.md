# RoCo AI — System Onboarding

> **This document is the onboarding contract.** It is the first thing a coder reads and the last thing they should need. If something needs to be known to work on this project — an invariant, a constraint, a decision, a gotcha, a link — it belongs here, at least as a pointer. When you learn something the hard way, add it here so the next person doesn't. Docs in `docs/` hold the detail; AGENTS.md is the entry point and the index (see §14).

AI-assisted collaborative writing tool. Backend: RWKV-7 SSM (2.9B params, 10k trained context). Think of it as an agentic scaffold around a recurrent state-space language model — the model generates tokens, everything else is orchestration, formatting, and state management.

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"
roco story --resume              # find and continue latest workspace
roco story --fix chapter 3       # regenerate chapter 3 in latest workspace
roco story --phase synopsis      # run a single phase, resume from there
roco story --mock                # mock backend, no real model (tests)
roco gui                         # desktop app
roco session create              # create a persistent chat session
roco session <session_id> -p "p" # execute a prompt in a saved chat session
```

## 1. What This Thing Actually Is

Two processes communicating over HTTP:

```
frontend (CLI / GUI / gateway)  ←→  inferd (token engine)
```

- **inferd** (`crates/inferd/` + `crates/engine-gpu/`): loads the model into VRAM (via Vulkan), accepts a prompt → runs inference → returns generated text. It knows nothing about messages, sessions, or user personas. It receives: text, state_id, grammar, temperature, max_tokens, stop. It returns: text.

- **frontend** (`crates/cli/`, `crates/gateway/`, `crates/app/`, `crates/ui/`): orchestrates the pipeline. Decides what text to send, what state to load/save, which grammar to apply, and what to do with the output.

RWKV-7 is a **State Space Model** (not a Transformer). Recurrent — linear memory in sequence length, constant VRAM per token. Because it's recurrent, "session" isn't a connection — it's a **vector of floats** (the model's latent state after processing N tokens). Save that vector → you've saved the session. Load it into the model → you're continuing from where you left off.

## 2. Directory Layout — What Goes Where and Why

Every command works relative to the current directory. All artifacts go in `.roco/` under that directory. That means:

- You `cd` into a project directory → `.roco/` is your project's scratch space
- You can have different `.roco/` in different directories → isolated projects
- No files leak into `~/.config/roco/` or `~/.local/share/roco/`
- The entire project state is a single directory you can delete or archive

Override `.roco/` location with `$ROCO_DIR`. If `ROCO_DIR=/data/roco`, then config loads from `/data/roco/config.toml`, workspaces under `/data/roco/workspaces/`, stories under `/data/roco/stories/`. This is for headless/CI/docker setups where cwd isn't meaningful.

```
.roco/                              # ROCO_DIR env var overrides this
├── config.toml                     # model path, server ports, template settings
├── agent-journal.md                # runtime log (TODO: migrate to JSONL)
├── workspaces/
│   └── {timestamp}_{slug}/
│       ├── 01-OUTLINE.md
│       ├── 02-WIKI.md
│       ├── 03-CHAPTER_*.md
│       ├── 04-VALIDATION.md
│       ├── 05-SYNOPSIS.md
│       └── 06-STORY.md
├── sessions/
│   └── {session_id}.json           # persistent conversation/chat session transcript
└── stories/
    └── {slug}.md                   # compiled final story
```

## 3. Engine Split — Why Two Crates

| Crate | Deps | Responsibility |
|---|---|---|
| `engine` | pure Rust, no GPU | Traits (`ModelBackend`), types (`CompletionRequest`, `TokenizeResult`), mock backend, grammar strategies, JSON helpers |
| `engine-gpu` | `web-rwkv` (Vulkan) | `RwkvActor` (tokio actor managing model), `RwkvBackend` (implements `ModelBackend`) |

Split ensures `engine` compiles everywhere (CI, docs, wasm targets). GPU deps only enter the build when `engine-gpu` is a dependency — which only happens when you need real inference. Tests compile against `engine` alone using `MockBackend`.

## 4. inferd Architecture — Atomic Operations

inferd's actor loop handles these operations:

| Operation | Effect |
|---|---|
| `Complete { text, init_state, state_slot, grammar, temp, max_tokens }` | Load `init_state` from cache (or blank if None), generate tokens from `text`, save resulting state as `state_slot` (or don't cache if None), return output text |
| `Bake { text, init_state, state_slot }` | Load `init_state` (or blank), feed `text` through model (no generation), save resulting state as `state_slot`. Primes the recurrent state. |
| `SaveState / LoadState` | Serialize/deserialize the raw state tensor for persistence. |

FeedEOS is NOT an inferd primitive. Callers manage state reset by saving/loading from cache or baking EOS text directly.

State pool: `HashMap<String, Option<Tensor>>`. Max 8 entries. FIFO + LRU eviction. Thread-safe (behind `Arc<RwLock<...>>`).

## 5. Inference Parameters

There is no single "strategy" — there are knobs that get composed:

| Knob | What it does |
|---|---|
| **Grammar** | BNF grammar string → `kbnf`-compiled token mask. Forces token sequences to match a grammar (e.g. valid JSON). |
| **Temperature** | Sampling temperature. 0.0 = greedy (always pick highest probability). Our 2.9B model starts repeating at ≥0.7. |
| **Top-p** | Nucleus sampling. Only sample from tokens whose cumulative probability exceeds p. |
| **Top-k** | Only sample from the k highest-probability tokens. |
| **Prefill** | Initial tokens to feed before sampling. For JSON, `"{\n"` jump-starts the output. |
| **Stop** | Token sequences that halt generation (e.g. `"}\n"` for JSON). |
| **Bake** (state tune) | Feed few-shot examples through the model (no generation) to prime the recurrent state with format expectations. |

The CLI `--strategy` flag selects a preset composition of these knobs:
- `state-tuned`: bake examples with no grammar, rely on recurrent state
- `schema`: JSON schema grammar
- `loose-json`: relaxed grammar, accepts JSON-like output
- `grammar`: user-supplied grammar string

## 6. State Management — Sessions Are Vectors

"Session" in RWKV is the model's recurrent state — a vector of floats. You manage it explicitly with `state_id` (load) and `save_as` (save).

Two named states used by the pipeline:

| Name | Used for |
|---|---|
| `story-writer` | Outline, wiki, chapters, synopsis — accumulates the full context |
| `story-validator` | Validation only — reset between chapters to prevent repetition bleed |

The state cache is a performance shortcut; the canonical state is the JSONL interaction log.

## 7. Story Pipeline — 6 Phases

```
outline → wiki → chapter (×N, each validated) → synopsis → publish
```

Each phase:
1. Formulates a prompt with `System:` preamble, `User:` task, `Assistant:` prefix
2. Sends to inferd as raw text
3. Attempts to parse the output as JSON (with grammar-constrained generation)
4. If JSON parsing fails, the phase should **fail loudly** — no prose fallback

## 8. Prompt Format — Who Formats What

inferd does NOT format prompts. It receives raw text and generates more text. The pipeline constructs:

```rust
let full_text = format!("System: {}\n\nUser: {}\n\nAssistant:", system, prompt);
inferd.complete(CompletionRequest {
    prompt: full_text,
    state_id: Some("story-writer"),
    save_as: Some("story-writer"),
    grammar: json_schema_grammar(),
    temperature: 0.6,
    max_tokens: 500,
    // ...
})
```

System prompts are task-specific:

| Phase | System prompt |
|---|---|
| Outline | `"You are a story outliner. Output valid JSON only."` |
| Wiki | `"You are a worldbuilding assistant. Output valid JSON only."` |
| Chapter | `"You are a fiction writer. Output valid JSON only."` |
| Validation | `"You are a quality reviewer. Be strict. Output valid JSON only."` |
| Synopsis | `"You are a literary summarizer. Output valid JSON only."` |

## 9. Eval-First Methodology — The Correct Order

Testing and validation have the highest priority. Prose fallback parsers were deleted in commit f3bab52. They existed because I violated the correct workflow: **evals first, workarounds never.**

A JSON parse failure is a system bug — handle it by fixing the system (prompts, baking, grammar, temperature), never by parsing prose.

### The validation loop (repeat until green)

```
test → eval → check + update PROGRESS.md with what you want to do →
e2e (story) → note problems in PROGRESS.md → fix issues →
targeted-e2e (chapter/wiki/outline/validate) → update PROGRESS.md →
consider whether a lack of info within AGENTS.md caused this problem,
if so update AGENTS.md → repeat
```

Steps in detail:

1. **Tests** (unit + integration) — prove the code works correctly in isolation (mock backend, deterministic, fast). `cargo check --workspace --all-targets` + `cargo test --workspace` must be green before touching the real model.
2. **Pending tests** — add `#[ignore]` tests for features not yet implemented. These build against the current API and can be run with `cargo test -- --ignored` when ready. Pattern: mark with `#[ignore = "pending <feature>"]` and add a comment explaining what's needed.
3. **Evals** — prove the model CAN produce correct output (valid JSON, coherent chapters) given correct inputs from the fixed pipeline. Each phase tested separately against the real model. Use `cargo run --release --example eval_suite -p roco-cli --features net -- http://127.0.0.1:18080 <case>`.
3. **Check + update PROGRESS.md with what you want to do** — BEFORE running the pipeline, update PROGRESS.md with your intent for this iteration: the specific change/experiment you're about to run, what you expect to happen, and what you're looking for. This makes the iteration auditable — if the run fails, the log shows what was expected vs what happened.
4. **E2E (full story)** — run `roco story` end-to-end with the real model. Manually review output quality.
5. **Note problems in PROGRESS.md** — every failure (model behavior, pipeline bug, harness bug) goes in the log, with evidence from logs/artifacts.
6. **Fix issues** — one at a time. Verify with cargo check after each change.
7. **Targeted E2E** — re-run only the affected phase(s) (chapter/wiki/outline/validate) against the real model to confirm the fix without a full pipeline run.
8. **Update PROGRESS.md** — mark what's verified green, what's still red.
9. **AGENTS.md self-check** — consider whether a lack of info within AGENTS.md caused this problem (e.g. a documented invariant that wasn't documented, or a note that would have prevented the mistake). If so, update AGENTS.md so the next iteration doesn't repeat it.
10. **Repeat** until the loop exits clean.

### Using PROGRESS.md — the working log

PROGRESS.md is the **live, lean working log** — the audit trail for the loop above. It is NOT a narrative and NOT the source of truth (AGENTS.md is). Rules:

1. **One entry per change, newest first.** Entry format: `### <date> — <title> (<commit>)` followed by 3-6 bullets: what you're doing, what changed, result, test count.
2. **Write intent BEFORE running** (loop step 3). Every entry opens with what you're about to do and what you expect — so a failed run is auditable as expectation vs. actual.
3. **Record outcomes after** (loop steps 5/8): green/red, evidence, `cargo test --workspace` count, commit hash.
4. **Point, don't duplicate.** Canonical state, plans, and gotchas live in AGENTS.md. A PROGRESS.md entry logs the delta and cites AGENTS.md sections (e.g. "documented red in §13"). If an entry needs more than a few lines, the knowledge belongs in AGENTS.md instead — write it there first.
5. **Keep it lean.** History lives in git (`git log`). When the file grows past ~40 lines, prune old entries; it should stay scannable top-to-bottom.
6. **Known red items stay visible.** Leave red items in their entry until fixed; when fixed, mark it in the entry where the fix landed — don't rewrite old entries.
7. **Every entry carries the test count.** `cargo test --workspace` result with each change, so regressions are traceable to a commit.

### Known Phase 8 verification state

As of the latest full E2E run (2026-07-31), the complete story pipeline passes against the real model:

- Grammar-constrained decoding fixed: `bnf_engine.rs` converts GBNF → kbnf (appends `;` per rule)
- Gateway `/v1/completions` forwards `grammar`/`prefill`/`init_state`/`state_slot`/`seed`
- **Schema enum scoping fixed** (`json_schema.rs`): enums are emitted as named rules, never inlined into object rules (GBNF `|` precedence trap — see §10)
- **Generation loop budget fixed** (`engine-gpu/src/actor.rs`): the first loop samples ONE token then hands off to the main loop (previously ran to `max_tokens` AND the main loop ran `max_tokens−1` more = 2×max_tokens−1 tokens). Grammar-closing token emitted before stopping in both loops.
- **No silent fallbacks anywhere** (commit `3181178`): router `detect_intent` returns `Result` and fails loudly. A failed intent parse, an unknown intent id, or a backend error is surfaced as an error — it is NEVER silently downgraded to chat. Same rule as prose fallback: if something isn't ready, it fails loudly, it doesn't quietly pretend to work.
- **All 11 pending-feature tests cleared** (2026-08-01): `cargo test --workspace` = 1005 passed / 0 failed / 0 ignored. MockBackend now mirrors the real actor: `on_token` emits whitespace-delimited stream chunks; `deadline_ms` → `EngineError::TimedOut`; `interrupt()` cancels in-flight latency; `top_a` truncates the BNF-walk candidate set (uniform-mass analog); `bnf_mask` drives deterministic masked generation; intent-classification prompts return score-based keyword intent JSON.
- **State blending is N-state** (was 2-state): `StateTuning::blend_states(&[(&str, f32)], output_session)` with normalized weighted blend, default = descriptive error. Pure math lives in `roco_engine::blend_weighted` (unit-tested incl. 3-state, normalization, zero-weight and length-mismatch edges); `RwkvBackend` + actor use it.
- **Story editor API routes are real** (were canned stubs): `update_outline`/`save_chapter` via new `StoryEngine::set_outline`/`save_chapter` (validating + persisting); `revise` runs evaluate→critique→revise; `suggestions`/`continue` call `WritingAssistant`; `apply_suggestion` merges into editor text deterministically. `web_rwkv_version` is emitted by `engine-gpu/build.rs` from Cargo.lock (was hardcoded).
- Full E2E: outline → wiki → chapters 1-3 (validated) → synopsis → publish ✅

## 10. Known Legacy Issues

### Agent Journal Format
Currently `.roco/agent-journal.md` in Markdown. Should be JSONL for structured querying (one JSON object per line: timestamp, level, phase, message).

### GBNF `|` precedence — never inline enum alternations into larger rules
`|` has the LOWEST precedence in GBNF/kbnf. Inlining an enum (`"pass" | "fail" | "needs-work"`) into an object rule splits the whole rule into multiple alternatives — the model can then legally emit a bare enum value (`"needs-work","suggestion":...`) that skips the `{`, the key, and sibling fields. `schema_to_gbnf` now emits enums as named rules (`root_quality_enum ::= ...`) and references them. If you hand-write GBNF with an enum inside an object, wrap it in a named rule.

### Generation loop structure (engine-gpu/src/actor.rs)
The complete() path has two loops: the first flushes the prompt and samples the FIRST token, then the main loop generates the rest. Both must share the same sampling path (`sampling::sample_token_masked_with_rng`) so grammar handling stays consistent. The grammar-closing token is emitted BEFORE the break (append-then-check), never dropped.

### Prefill vs grammar mask
Prefill tokens (`{\n`) are fed to the model but deliberately NOT passed through the grammar mask. The mask re-emits `{` as its first generated token, so `resp.text` is complete JSON on its own — callers parse `resp.text` directly without prepending the prefill. Do NOT "fix" this by accepting prefill tokens into the mask; it breaks the pipeline's parse path.

### Model-Specific Behavior
2.9B parameter RWKV-7 model (full model reference incl. architecture, quantization, baking patterns, research links: `docs/rwkv-v7-g1.md`):
- Starts repeating at temperature ≥ 0.7 (empirical)
- Can produce NUL/control characters in output (stripped by `clean_json_output`)
- Sometimes produces ````json``` wrappers or trailing `}` characters
- May truncate output when the state carries too much context momentum
- **Judges instruction-following too literally** (e.g. "crystal sword" ≠ "ancient artifact" → `follows_instructions: false`). The `val_instruction_following_matched` eval fails for this reason — it's a model limitation, not a system bug.
- **In-string content degrades under tight constraints** — with a grammar mask active, string values can contain degenerate tokens (`":[`, `s,s,s`). Structure is always valid JSON; content quality varies. Revision retries handle the worst cases.
- **Chapter 2-3 often need 2-3 revision retries** at temp 0.5 — the retry loop converges, this is expected.

### Persistent Chat Session Subcommand (`roco session`)
The CLI includes a dedicated `roco session` subcommand designed to support stateless orchestration and lightweight multi-turn integrations.
- `roco session create` dynamically generates a unique session ID and registers a blank state under `.roco/sessions/`.
- `roco session <id> -p "p"` loads the specified session, executes the user's prompt (streaming output to terminal), and updates the transcript file.

### Migration Complete

The legacy `session`/`bake_state`/`OpenAiCompletionRequest::session` bridge fields have been removed. All callers use `init_state`/`state_slot` and embed system text directly in the prompt.

## 11. Common User Feedback

### Source
`docs/impressions/v_0_4/common_user.md` — documented after testing with a fresh user perspective.

### Key Findings

**What works well:**
- `roco story "premise"` is a one-command magic experience
- Emoji progress indicators (`✓`, `⚠️`, `📝`) are intuitive
- Automatic retry on validation failure is reassuring
- Resume capability works seamlessly

**Pain points to address:**
1. **No progress bar during generation** — users wonder if app is stuck during 15-45s waits
2. **Silent baking phase** — 3-second pause with no feedback looks frozen
3. **Help text is technical** — lacks "Quick Start" guidance
4. **No first-time setup** — new users don't know about GPU requirements or model setup
5. **Output location not obvious** — `.roco/stories/` path buried in output
6. **No story preview** — story goes to disk with no terminal preview
7. **Error messages are technical** — need actionable hints

### User Questions (Unanswered)

| Question | Answer Location |
|----------|----------------|
| How do I install? | Not documented |
| What model do I need? | Not in help |
| Can I run without GPU? | Not obvious |
| Where do stories go? | Only in output |

### Recommendations

Adopted into the current, deduplicated list in §12 — this section is the historical v0.4 record only.

**User Score: 8/10** — The magic is real. Main gaps are onboarding and feedback during long waits.

## 12. Common User Feedback — v0.5

### Source
`docs/impressions/v_0_5/common_user.md` — documented after testing with a fresh user perspective.

### Key Findings

**What works well:**
- `roco -p "make a story"` is a one-command magic experience
- The router's natural-language intent detection is the right mental model
- Emoji progress indicators (`✓`, `⚠️`, `📝`) are intuitive
- Automatic retry on validation failure is reassuring
- Resume capability works seamlessly

**Pain points to address:**
1. **Session/workspace terminology is confusing** — common users don't know what these mean and don't need to. The explicit `session`/`workspace` workflow is NOT recommended for common users; these commands stay in the tool for power users and automated flows, and should be renamed or hidden from default help.
2. **Router intent detection is broken in practice** — with the mock backend it fails, so after the FALLBACK removal natural-language mode switching (`roco "let's play an adventure"`) errors loudly instead of routing. Correct behavior, unfinished feature — top priority, see **§13**.
3. **No `roco quickstart`** — new users don't know how to begin
4. **Help text is technical** — lacks "Quick Start" guidance
5. **No first-time setup guide** — new users don't know about GPU requirements or model setup
6. **Output location not obvious** — `.roco/stories/` path buried in output
7. **No story preview** — story goes to disk with no terminal preview
8. **Error messages are technical** — need actionable hints

### CLI Workflow (v0.5, current)

The recommended workflow for common users — router-first, state auto-managed:

```bash
# Quick one-shot story
roco -p "Write a story about a cat who loves cheese"

# Structured story pipeline (detached)
roco story "A cat who loves cheese"

# Natural language routing (target state — §13)
roco "write a story about a cat"
roco "let's play an adventure"
roco "generate a webpage about sailing"
```

The explicit session/workspace flow is **not** for common users. It exists for power users and automation:

```bash
roco session new                          # Create session (power user)
roco workspace new                        # Create workspace (power user)
roco session <session_id> -p "Use the workspace <workspace_id>"
```

**Gotcha — top-level `-p`:** the top-level `-p`/`--prompt` handler in `bin/roco.rs` routes to prompt mode, so `roco session <id> -p "…"` must be exempted (it is — `first_sub != "session"` guard). If you add another subcommand that consumes `-p` itself, exempt it there too; otherwise the subcommand becomes unreachable and `session create` prints the bare ID (script-friendly) — don't "fix" it back to a label without updating `test_cli_session_subcommand`.

### Future State — Router NLU Is the Golden Opportunity

Router NLU is the golden opportunity: every message already flows through `detect_intent`; extend it with management intents and auto-managed state and "user says what they want → roco does it" becomes the whole UX. Full current + future state: **§13**.

### Recommendations (Not Yet Implemented)

Router/NLU items (intent detection, keyword routing, rename/hide `session`/`workspace`, auto-management) live in **§13** — the single origin for the router plan. The remaining UX items:
- [ ] Add `docs/README.md` link to `--help`
- [ ] Add `roco quickstart` first-run guide
- [ ] Add progress indicators (spinners) during long waits
- [ ] Improve error messages with actionable hints
- [ ] Show full output path prominently
- [ ] Offer story preview after publishing

**User Score: 6/10** — The magic works when users discover the router or `-p` flag, but the technical terminology (`session`, `workspace`) and the broken natural-language routing create friction. The NLU router is the golden opportunity: it already handles natural language, we just need to extend it with more intents and make it actually work.

## 13. Router & NLU — Current State and Future State

### Current State (as of 2026-08-01)

- Modes ARE implemented as system prompts (`mode_system_prompt` in `crates/cli/src/cmd/router.rs`, ~line 576 Adventure, ~line 588 Coder). They are reachable ONLY via direct subcommands: `roco game`, `roco code`, `roco html`, `roco story`.
- `all_intents()` defines 5 intents: `chat`, `adventure`, `story`, `html`, `coder`.
- `detect_intent()` sends an intent-classification prompt to the backend, expects `{"intent": ..., "prompt": ...}` JSON back, and maps it to a mode.
- **Mock path works now** (item 1a of the plan below landed): `MockBackend` recognizes intent-classification prompts and returns intent JSON via score-based keyword classification (ties prefer the later intent, e.g. "write code" → coder; all-zero → chat). Proven by `test_detect_intent_with_mock_backend` in `crates/cli/src/cmd/router.rs`. `roco "let's play an adventure"` now routes correctly under the mock backend.
- **Real-model path still unverified**: with a live model the intent prompt MAY work (it's a valid completion request), but it has never been verified end-to-end through the router loop. This is unverified state — treat as red until proven.
- **No-fallback rule holds** (commit `3181178`): if neither the mock nor the model produces valid intent JSON, `detect_intent` errors loudly (exit 1) — never a silent chat downgrade.

### Future State — the plan

1. **Make intent detection actually work.** Two options, both needed:
   - ~~Fix `MockBackend` to return valid intent JSON when the prompt asks for classification~~ **DONE (2026-08-01)** — score-based keyword classification; tests + router work without a real model.
   - Add a deterministic **keyword-based router in the CLI** as the primary path (real classifier, not a silent fallback — see below): e.g. `\b(story|write|tale)\b` → story, `\b(adventure|play|game)\b` → adventure, `\b(html|webpage|website|page)\b` → html, `\b(code|program|function|bug|rust)\b` → coder. If keywords don't match, THEN ask the model. If the model fails to produce valid JSON, error loudly (current behavior). **Still future** — the keyword classification currently lives in MockBackend only, not in the CLI.
2. **Extend `all_intents()` with management intents** so the router auto-manages state invisibly:
   - `continue` → find and resume the latest workspace/project
   - `new_project` → start fresh, auto-create state
   - `show_work` → list existing stories/projects
   - This removes the need for common users to know about `session`/`workspace` at all.
3. **Auto-manage state.** The router should create/resume/switch sessions and workspaces on the user's behalf. Explicit `session`/`workspace` commands remain for power users and automation but are hidden from default help.
4. **Keep the no-fallback rule.** The keyword router is a real classification step, not a silent downgrade: if neither keywords nor the model can classify intent, the user gets an explicit error telling them what went wrong.

### Why this is the golden opportunity

Every message already passes through `detect_intent`. Extending it with management intents and auto-managed state turns the CLI into: "user says what they want → roco figures out mode, project, and state → does it". No commands to remember, no `session`/`workspace` jargon, no `roco story -p` detour. The pipeline, grammar, and state management already work (§9) — the only missing piece is the routing layer.

## 14. Documentation Map — Where to Find What

AGENTS.md is the origin: current architecture (§1-8), verification state (§9), known issues (§10), user feedback (§11-12), and the router plan (§13). Deeper material lives in `docs/` — read these when you need the detail, not before:

| Doc | Status | What's useful there (not duplicated here) |
|---|---|---|
| `docs/rwkv-v7-g1.md` | Model reference | RWKV-7 internals: architecture (32L / 2560 emb / 40 heads), quantization, per-phase temperature guide, baking patterns, state-pool notes, research connections (DREAMSTATE, DeltaProduct, GoldFinch) |
| `docs/CLI_STREAMING_AND_LEAKS.md` | Rework record | Chat/streaming layer NOT covered in §1-8: `StreamPrinter` monotonic rendering, SSE line-buffer reassembly, `ChatSession` whole-turn budgeting (8k chars / 16 turns), identity fast-path (`roco whoami`, `:name`, `:remember`), resource audit (SmartCache LRU + byte budget, journal rotation @ 8 MiB, reqwest timeouts) |
| `docs/TMUX_ROCO_GUIDE.md` | Operational | Testing the CLI + mock harness under tmux; `scripts/tmux_roco` emulator for offline environments; maps Rust harness to Python equivalents |
| `docs/impressions/` | User feedback | Common-user journeys v0.4/v0.5 — the evidence behind §11/§12 (session/workspace confusion, router-first UX) |
| `docs/TECHNICAL_SPECIFICATION.md` | Historical snapshot | Pre-migration reasoning behind design decisions; failure-mode table; reproduction checklist. Superseded by §1-8 — read for the *why*, not the *what* |
| `docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` | Historical audit | Sandbox path-containment rationale (`is_safe_relative_path`), crate-consolidation history (19 → current) |
| `docs/rfc/0001-0015` | Decision records | Per-feature design rationale: harness, offline protocol, privacy RAG, security boundary, vision, etc. |
| `docs/future.md` | Completed | All roadmap items landed |

Rule of thumb: **AGENTS.md tells you how the system works today; the docs tell you why it was built this way.** If you're about to add a behavior, check the corresponding RFC/impression first so you don't re-decide a closed question. And if you're about to leave the project without writing down something you now know, write it down here first — see the onboarding contract at the top.
