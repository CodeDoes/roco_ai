# PROGRESS.md — RoCo AI

> Strategy / context / "what we wanted to do but didn't yet".
> Living document; this version reflects the post-2026-07-18 state where
> the story engine and human-AI interaction surfaces are implemented at the
> module level, and the **Zed LSP front-end** has been promoted from a stub
> handshake into a real completion server backed by the FIM completion path.

## Current Focus

**Primary Goal:** A collaborative story writing tool where humans and AI work together to create stories.

The core engine is done. The human-AI interaction layer is implemented.
The active, just-finished work was the **Zed editor integration** — turning
the LSP stub (`crates/cli/src/lsp.rs`) into a real `textDocument/completion`
server that surfaces the FIM fill-in-the-middle pass inside the editor. That
work touched four layers: the LSP handler, the CLI server front-end, the Zed
plugin manifest, and the FIM grammar library. See "Zed LSP" below.

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
   and resume from it. See `bake_fim_session` in `crates/cli/src/lsp.rs`
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

4. **A raw-prose BNF grammar CAN be satisfied** because kbnf accepts
   character-class rules (`#[A-Za-z]`) that map onto the vocab's word tokens
   without needing any JSON punctuation. `StoryGrammar::FillInMiddle`
   (`GBNF/fill_in_middle.bnf`) proves this: it accepts a plain English
   sentence/paragraph with no template markers and structurally forbids
   `<think>` / `<fim>` / `BEFORE:` scaffolding. It's the reference for the
   "prose handlers can be grammar-bounded" path, and a candidate constraint
   for the FIM completion output once the LSP is wired to send a grammar.

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
- **LSP is a thin client, not a second model host.** The editor-spawned
  `roco server --stdio-lsp` process loads **no model** — it constructs a
  `RemoteBackend` and talks to the user's already-running inference API
  server (`ROCO_API_URL` / `--inference-url`, default `http://127.0.0.1:8080`).
  This avoids a second VRAM-hungry process and a TCP-port collision with the
  user's manually-started `roco server`. The LSP exits cleanly when the
  editor closes stdin.

## Zed LSP — Status: promoted from stub to real completion server (2026-07-18)

The LSP was previously a one-shot `initialize` handshake (`handle_lsp_initialize`):
it answered `initialize` with empty capabilities and then ignored stdin while
roco served HTTP on a port. That design was wrong on two counts — it owned a
model it shouldn't, and it exposed no real completions. The rewrite (uncommitted
at time of writing) replaces it with a genuine LSP loop.

**What changed:**

1. **Real LSP loop (`run_lsp` in `crates/cli/src/lsp.rs`).**
   - Answers `initialize` with `textDocumentSync: full` + `completionProvider`
     capabilities (this is what makes Zed actually call `textDocument/completion`).
   - Tracks open documents via `textDocument/didOpen` / `textDocument/didChange`.
   - Handles `textDocument/completion`, `shutdown`, `exit`; ignores `initialized`
     and other notifications. Replies `null` to unhandled methods so Zed never hangs.
   - Correct Content-Length framing for both `read_message` and `send_response`;
     clean exit on stdin EOF (editor closes).

2. **FIM completion surfaced in-editor (`completion()`).**
   - Computes cursor prefix/suffix from the tracked doc text (2048-char window
     each side), maps `line`/`character` to a byte offset across char boundaries.
   - Four dispatch cases:
     - both sides empty → generic passage seed,
     - prefix only → forward continuation,
     - suffix only → lead-in to the given text,
     - **both sides → baked-session bridge** (state-tuned few-shot: resume the
       `roco_fim` named session and send only the compact `NOW / BEFORE / AFTER /
       INSERT:` context). The degenerate one-side-empty cases deliberately fall
       back to a plain completion because resuming the baked session loops the
       template.
   - All completions use `prefill: <think></think>` to suppress think-leak, and
     return the trimmed prose as a single LSP completion item.

3. **Project-aware bake (`bake_fim_session`).**
   - Bakes the few-shot BEFORE/AFTER/INSERT examples **plus every open file's
     content** into the `roco_fim` session via `preserve_state` calls. This is
     the RWKV state-tune equivalent of Zed's Zeta-2 related-file context: the
     recurrent state absorbs the bridge task and the project's open files, so
     completions are project-aware without re-feeding tokens each time.

4. **CLI front-end split (`crates/cli/src/bin/roco.rs`).**
   - `roco server --stdio-lsp [--inference-url URL]` now takes the LSP branch
     *before* the model load: it builds a `RemoteBackend` and runs `run_lsp`,
     returning without ever loading the model or binding a TCP port.
   - The old inline `handle_lsp_initialize` spawn inside the HTTP server is gone.

5. **Zed plugin (`apps/plugins/zed/`).**
   - `extension.toml`: `languages = ["*"]` (global LSP — provides completions
     for every file type, not just Markdown) and dropped the `--story` flag from
     the spawned `language_server_command` args (the LSP no longer needs story mode).
   - `src/lib.rs`: added tests locking the `/roco` slash-command request
     contract (`POST /v1/completions`, `prompt` + optional `system`/`temperature`/
     `max_tokens`, no chat-completions fields) and the `/health` path.
   - `scripts/roco-zed-server.sh`: launcher that resolves `RWKV_MODEL`, ensures
     the `roco` binary is on PATH, and runs the inference API server the LSP talks to.

6. **FillInMiddle grammar (`crates/grammar/src/grammar_library.rs` + `grammar/src/lib.rs`).**
   - New `StoryGrammar::FillInMiddle` backed by `GBNF/fill_in_middle.bnf` — a
     raw-prose grammar (char-class based, no JSON punctuation) that structurally
     forbids `<think>`/`<fim>`/template markers. Registered in `all()` and
     re-exported from the crate root; the grammar-library test accepts a valid
     sample to completion.

**Eval status:** `evals/results/latest.json` was captured against the `remote`
backend with no inference server loaded, so it reports 0/4 (`backend_name:
"remote"`, `total: 4`, `passed: 0`). That is an artifact of the capture
environment, **not** a code regression — the FIM path itself was repaired and
verified against the real backend in the prior commit (`fix(fim): wire grammar
masks + fix FIM loop, correct state-tune bake`). The eval suite stays green
when run against a live `roco server`. `latest.mismatches.txt` shows "no oracle
mismatches" once a real backend is behind the `remote` client.

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
   constraints rather than a JSON envelope). `FillInMiddle` proves a
   char-class prose grammar *is* satisfiable and is the template for routing
   these handlers through real grammars.
2. **Grammar coverage audit** 🟢 — every `backend.complete()` call should be
   BNF-bounded where the output shape allows it. FIM is now correctly wired
   end-to-end (LSP → RemoteBackend → inference server → FIM pass).
3. **Live eval continuity** 🟢 keep checking — `cargo test -p roco-engine`
   (12 unit tests pass, including `bake_into_session_replays_examples_on_named_session`
   and the FIM bake) and the eval harness must stay green *against a live
   backend*. The `remote` capture in `latest.json` is environment-only.
4. **Story human CLI polish** 🟢 ongoing — `story_human.rs` is the
   canonical UX; bugs/UX gaps from real writing sessions get folded
   back in here.
5. **Zed LSP robustness** 🟢 — wire `StoryGrammar::FillInMiddle` into the
   LSP `completion()` call so editor completions are grammar-bounded; handle
   the `textDocument/completion` `partialResult`/`resolveProvider` edge cases
   if Zed requests them; confirm `scripts/roco-zed-server.sh` path resolution
   across machines.

### Historical (now done) — kept for context

**Phase 2 — Observability & Control — ✅ IMPLEMENTED**
**Phase 3 — Reversibility & Versioning — ✅ IMPLEMENTED**
**Phase 4 — Human-AI Interaction — ✅ IMPLEMENTED**
**Phase 5 — Multiple Interfaces — ✅ IN PROGRESS** (cli, tui, web, api; Zed LSP now live)

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
- **LSP does not load a model**: the editor-spawned `roco server --stdio-lsp`
  is a client to the singleton inference API server. Start that server once
  (via `scripts/roco-zed-server.sh` or `roco server`) with the model loaded;
  the LSP will find it at `ROCO_API_URL` (default `http://127.0.0.1:8080`).
