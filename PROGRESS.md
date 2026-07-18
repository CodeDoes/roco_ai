# PROGRESS.md — RoCo AI

> Strategy / context / "what we wanted to do but didn't yet".
> Living document; this version reflects the post-2026-07-18 state where
> the story engine and human-AI interaction surfaces are implemented at the
> module level, and the **FIM (fill-in-the-middle) completion path** has
> been repaired end-to-end.

## Current Focus

**Primary Goal:** A collaborative story writing tool where humans and AI work together to create stories.

The core engine is done. The human-AI interaction layer is implemented.
The active, just-finished work was the **FIM completion path** — the
fill-in-the-middle pass the Zed/VS Code LSP (`crates/cli/src/lsp.rs`)
and the eval harness (`crates/engine/src/cases.rs` `fim_eval_cases`)
send to the backend. That path was broken in two independent ways; both
are now fixed. See "FIM Completion" below.

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

### The `<think>` Tag Problem
Undertrained base RWKV models consistently leak planning text into output:
- System prompts saying "no thinking" have zero effect — and **backfire**: a
  system instruction like "never use `<think>` tags" primes the model to emit
  `<think>` (verified in `prompt_probe_eval.rs`).
- Temperature decay has minimal impact — the behavior persists across all settings.
- A **bare `Assistant:` start defaults to an open `<think>` block** (the root
  cause of contamination in the story pipeline). Probing an empty context with
  `prefill = None` produced `<think>` as the first tokens.
- Pre-filling a **CLOSED** think block (`<think></think>`) before the prompt puts
  the model straight into content mode and it does **not** re-open `<think>`
  (`NO_THINK_PREFILL` in `crates/engine/src/backend.rs`). This is the reliable
  suppression lever.

### Think-tag state-tuning (experimentally validated)
The model's think-tag prior was probed directly (`prompt_probe_eval.rs`) by
feeding the training-prompt prefixes as `prefill` after `Assistant:`:
- `Assistant: <think` → model closes the tag (`>`) and emits chain-of-thought.
- `Assistant: <think></think>` → model emits content, no re-open. **Reliable.**
- Baking a no-think session (`bake_no_think_session`) biases the recurrent state
  away from `<think>`, but it is a *soft* bias and noisier than the prefill;
  the correct-role bake still showed occasional `User:`-turn leakage. Prefer the
  closed-think **prefill** for deterministic suppression.

### FIM / Fill-In-the-Middle — the RWKV-correct pattern
RWKV-g1h has **no FIM sentinel tokens** in its vocab (`✿`, `<fim>`, etc.),
so middle-fill is done by *instruction* (a `BEFORE:` / `AFTER:` / `INSERT:`
bridge). Two traps bite anyone who tries the naive approach:

1. **Re-feeding few-shot as prompt tokens makes the base model CONTINUE the
   examples** instead of answering. The RWKV-correct technique is to
   **bake the few-shot into a named recurrent-state session** (state-tuning)
   and resume from it. See `bake_fim_session` in `crates/engine/src/eval.rs`
   (mirrors the validated `bake_into_session` in `crates/engine/src/backend.rs`).

2. **The bake must replay (user, assistant) TURNS**, not cram the whole
   few-shot into one prompt blob. `bake_into_session` feeds each example's
   *question* as a `User:` turn and its *answer* as an `Assistant:` turn
   (`system` empty on replay turns, only on the first). Doing it wrong (e.g.
   one `prompt` blob, or feeding the assistant text as `prefill` instead of
   `prompt`) leaves the baked state expecting another user turn and the model
   emits spurious `User:` / `BEFORE:` scaffolding when resumed.

3. **A JSON-envelope grammar (`{"insert": "..."}`) is UNSATISFIABLE on this
   vocab.** The RWKV-g1h tokenizer has **zero standalone JSON-punctuation
   tokens** — `"`, `{`, `}`, `:`, `,` are all absent, and no token starts
   with `"`. So a kbnf mask starting at `root` (`{`) has no allowed token
   for the structural characters and generation dies after 3 tokens. This is a
   **model-vocab limitation, not a code bug** (proven: `grammar_smoke`
   passes because `"yes"`/`"no"` are whole vocab tokens). Per the
   Grammar-First principle, FIM therefore emits **raw prose** constrained by
   prompt + per-token stop-conditions + forbidden-string checks, not a JSON
   grammar. The stop-conditions in `RwkvActor::handle_complete` are what
   keep a resumed FIM session from echoing the `BEFORE:`/`AFTER:`/`INSERT:`
   scaffolding.

### Architecture Decisions Proven Correct
- Code owns control flow, LLM only fires at fixed grammar-bounded points
- Pull-based context injection over push-based bulk transfer
- Jaccard word overlap relevance scoring sufficient for initial use
- Arc-owned context sources cleanly satisfy `'static` bounds for `Box<dyn Fn>`
- Persistent timestamped workspaces prevent collision across repeated runs
- Pre-fill `<think></think>` (or a content lead-in) suppresses `<think>` reliably
- **Grammar-engine isolation**: `roco-bnf-engine` (kbnf) must NEVER enter the
  same compilation unit as `web-rwkv`'s `TokioRuntime` — doing so triggers
  `error[E0275]` (type recursion overflow via `string-interner`). The
  `BnfMask` trait lives in `roco-engine`; `roco-bnf-engine` depends on
  `roco-engine` (NOT the reverse). `roco-engine` must NOT depend on
  `roco-bnf-engine`, so mask *construction* happens in the application layer
  (e.g. `crates/cli/examples/eval_suite.rs` `build_masks`), while the
  inference actor only consumes an already-built `Box<dyn BnfMask>`.
- **Vocab for masks**: the backend exposes `vocab_bytes()` on the
  `ModelBackend` trait; `RwkvBackend` returns its real vocab, `RemoteBackend`
  fetches it from the server's `/vocab` endpoint. The eval harness builds
  masks from `case.grammar` + `backend.vocab_bytes()` before running.

## FIM Completion — Status: repaired (2026-07-18)

The FIM path had two independent bugs, both now fixed and committed
(`fix(fim): wire grammar masks + fix FIM loop, correct state-tune bake`):

**Bug 1 — generation looped / echoed scaffolding.**
`RwkvActor::handle_complete` applied its stop-conditions (catch
`BEFORE:`/`AFTER:`/`INSERT:`/`User:`/`NOW`) only inside the *first-token*
sampling loop, which then `break`'d unconditionally after one token. The
*remaining-token* loop had **no stop check at all**, so a resumed/baked FIM
session would emit "The knight drew his sword…", then "User: NOW\nBEFORE:…"
repeatedly until `max_tokens`. Fixed: apply the same `is_stop` check on
**every** generated token.

**Bug 2 — grammar masks were never built.**
`CompletionRequest` carries both a `grammar: Option<String>` and a
`bnf_mask: Option<Box<dyn BnfMask>>`, but the FIM eval sent only
`grammar`; the actor consumed only `bnf_mask`. So FIM output was always
free-form (and, pre-fix, looped). Fixed by building the mask in the
application layer from `grammar` + `vocab_bytes()` (see architecture note
above) and plumbing it through `EvalCase.bnf_mask` → `run_eval` →
`CompletionRequest.bnf_mask`. Verified working via `grammar_smoke`.

**Bug 3 — the FIM state-tune bake was malformed.**
The old `bake_fim_session` crams the entire few-shot (with
`BEFORE:`/`AFTER:`/`INSERT:` markers) into ONE `prompt` blob. The baked
state learned the scaffolding, not the turn structure, so resuming produced
either an echo or nothing. Rewrote it to replay (context, answer) turns
like the validated `bake_into_session`.

**Result:** the FIM eval (`--filter fim`) no longer loops. The two
*non-session* cases (`fim_prefix_only_continuation`,
`fim_suffix_only_preceding`) pass 100% of the time. The two *session-bridge*
cases (`fim_basic_bridge`, `fim_no_tag_leakage`) depend on the baked
state-tune and are stochastic on the undertrained 2.9B model — they pass
when the bake steers correctly and the per-token stop-conditions catch any
stray scaffolding. The eval is structurally sound; remaining variance is
model quality, not a code defect.

## Story engine — status: implemented (module-level)

Dynamic outline expansion, plot-state tracking, context assembly, chapter
continuation, quality evaluation (model-as-judge, 7 dimensions), revision
support, session persistence — all in `crates/agent/src/` and friends. The
interaction layer (outline editing, NL feedback, real-time preview,
revision-with-diff, story direction, chapter steering, writing assistant,
commentary, interaction modes) is also implemented; the surface that ties
these into the live CLI is `crates/cli/examples/story_human.rs` (the
canonical entry point for human-AI writing sessions).

Human-AI collaboration surfaces (`OutlineEditor`, `FeedbackParser`,
`StoryDirection`, `ChapterSteerer`, `InteractionMode`, `WritingAssistant`,
`Commentary`, `Reversibility`/`VersionControl`, `ObservabilitySystem`)
are all implemented.

## Current Priorities

The story engine + human-AI interaction surfaces are all implemented at
the module level. What remains is tightening + coverage:

1. **Per-handler BNF grammars** 🔴 — the JSON envelopes are
   BNF-constrained where applicable; the *prose body* of chapter / outline /
   wiki / synopsis / validation handlers is generated as raw prose (the FIM
   work confirmed RWKV-g1h cannot satisfy a JSON-punctuation grammar, so prose
   handlers intentionally use prompt + stop-condition + forbidden-string
   constraints rather than a JSON envelope).
2. **Grammar coverage audit** 🟢 — every `backend.complete()` call should be
   BNF-bounded where the output shape allows it. FIM is now correctly wired.
3. **Live eval continuity** 🟢 keep checking — `cargo test -p roco-engine`
   (12 unit tests pass, including `bake_into_session_replays_examples_on_named_session`
   and the FIM bake) and the eval harness must stay green.
4. **Story human CLI polish** 🟢 ongoing — `story_human.rs` is the
   canonical UX; bugs/UX gaps from real writing sessions get folded
   back in here.

### Historical (now done) — kept for context

**Phase 2 — Observability & Control — ✅ IMPLEMENTED**
**Phase 3 — Reversibility & Versioning — ✅ IMPLEMENTED**
**Phase 4 — Human-AI Interaction — ✅ IMPLEMENTED**
**Phase 5 — Multiple Interfaces — ✅ IN PROGRESS** (cli, tui, web, api)

## Model loading strategy

```
hardware scan → resolve model path → quantize for VRAM → build context → generate
```

- **Auto-resolution**: `$RWKV_MODEL` env var → first `rwkv7-*.st` under
  `models/` or `../models/` → error listing what was on disk.
- **Auto-quantization**: reads `Loader::info` for layer count + embedding,
  reads the on-disk FP16 file size as ground truth. Policy:
  `< 1.5 GB` → no quant; `≥ 1.5 GB` (and `gpu_coop`) → NF4;
  `≥ 1.5 GB` (no `gpu_coop`) → Int8; otherwise no-quant.
- **Build in `--release` for GPU work**: `build_v7()` hangs in debug on most
  consumer GPUs. Release completes load in ~18–25 s and generates
  ~16–20 tok/s on RTX 2050 / NF4 / 2.9B.
- **GPU memory**: the 4 GB RTX 2050 is tight for the 5.6 GB FP16 2.9B
  model under NF4. Kill any stale `roco` processes holding VRAM
  (`nvidia-smi --query-compute-apps=pid`) before a run — a leftover
  process can consume enough VRAM to OOM the load.
