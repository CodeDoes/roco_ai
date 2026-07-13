# PROGRESS.md — RoCo AI

> Wishlist / strategy / "what we wanted to do but didn't yet".
> Living document; this version reflects the rwkv7-first scope as of 2026-07-12.

## Current scope

The active focus is **rwkv7** — push the local RWKV-7 g1g family as the
single backbone model for everything we can. The only working inference
path today is `crates/core/src/rwkv_backend.rs` (web-rwkv + WGPU +
SafeTensors). Other backends (mock / HTTP) exist for tests but are not
the product.

When the local model isn't good enough, the documented escape hatch is
the Story Agent harness from `~/Documents/dev/ksr/` — its `infer/`
stack inherits `rwkv_backend.rs` unchanged and adds grammar-constrained
generation on top. We don't currently pursue non-rwkv7 engines (candle,
llama.cpp, mistral.rs, LiteRT).

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
crates/core/src/eval_suite::run_suite           <- 10 default cases live here
crates/core/src/engine::ModelBackend::complete   <- trait, code-path-agnostic
        |
        v
crates/core/src/rwkv_backend::RwkvBackend::complete
   sends CompleteReq over mpsc::Sender
        |
        v
RwkvActor thread (LocalSet + current-thread tokio)
   owns Context, TokioRuntime<Rnn>, AnyState, token_strings
   * compiles grammar (schoolmarm) if Grammar is Some
   * resets State before each completion (no leak between requests)
   * prompt tokens -> softmax_one -> sample_token
        + grammar_state.allowed_tokens masks disallowed indices to -inf
        + accept_token(word) advances state after each sample
   * decodes via web_rwkv::Tokenizer
        |
        v
CompletionResponse.text -> caller
```

The actor-thread split exists because `web-rwkv`'s async methods
produce non-`Send` futures (they embed wgpu resources). The
`build_v7` weight upload happens once at thread spawn; afterwards
the actor is a request server. `mpsc::Sender::send` from the calling
thread is `Send`, so callers can be anywhere.

If you ever write Rust that touches wgpu directly from outside this
file, you're probably picking the wrong layer.

### Decisions baked into the rwkv critical path

Non-obvious choices that sit in the file and the next contributor
would otherwise re-litigate:

| Decision | Why |
|---|---|
| `std::fs::read`, not `memmap2` | Mmap crashed producer on the AMD iGPU; Vec is small by 2026 budgets and disk-cache makes re-reads effectively free. |
| `LocalSet` + current-thread tokio on a dedicated OS thread | web-rwkv's async methods produce non-Send futures (they embed `wgpu::Device`). `mpsc::Sender` is Send across threads, so callers can be anywhere. |
| State reset on every `complete()` | Independent evaluations need clean state. Cheaper than running a per-call session tracker on the actor. |
| Grammar state is fresh per `complete()` (no caching) | `schoolmarm::GrammarState` isn't `Sync` and only meaningful for one turn. Trying to cache complicates the API without measurable benefit. |
| Bytes -> UTF-8 PUA mapping (`U+E000..U+E07F`) | Schoolmarm expects UTF-8 strings for tokens; non-ASCII BPE bytes only round-trip through that PUA range. Matches what rwkv-harness' schoolmarm consumer does. |
| NF4 / Int8 / no-quant policy driven by on-disk file size | wgpu's `max_buffer_size` over-reports (200x on RTX 2050); on-disk size is ground truth. |
| Filter tokens with `logits[i] = NEG_INFINITY` and `sample_token` unchanged | Avoids a second sample implementation; `-inf` rank-orders below any real probability. |
| Allow passthrough on `schoolmarm::accept_token` failure | BPE chunkings straddle literal boundaries; the right semantics is "didn't advance", not fatal. |

## What fits on this hardware

Real-world measurements on the dev-kit (NVIDIA RTX 2050, 4 GB VRAM):
the 2.9 B model (~5.5 GB FP16) needs NF4 to land in VRAM. Smaller rwkv7
checkpoints (0.1 B / 1.5 B) are still GGUF; converting them to ST
hits a known tensor-layout bug in `scripts/gguf_to_st_converter/`
(see AGENTS.md "Next things").

## Eval framework (`eval_suite.rs`)

`crates/core/src/eval_suite.rs` + `crates/core/examples/eval_suite.rs`.
The harness runs `EvalCase` records against any `ModelBackend`
(`mock`, `rwkv`, …) and writes a structured JSON report.

Built-in categories: smoke, instruction following, coherence,
repetition, throughput, format, context. The example binary takes
`--backend` and `--filter` flags.

Per-each-call runtime artifacts land under `.roco/evals/<name>/result.json`
(via `crates/core/src/eval.rs::run_eval`). At the time of writing only
`.roco/evals/delegate/` is populated — anchor for a future where this
fills up.

### Grammar-constrained variant (in progress)

`rococore::engine::CompletionRequest::grammar: Option<String>` is wired
through end-to-end when the `grammar-rwkv` feature is enabled:
- `crates/core/src/rwkv_backend.rs` compiles the GBNF via `schoolmarm`,
  masks logits at every sample step, and advances the state through
  `accept_token`.
- `RWKV_GRAMMAR` env var is the fallback for scripts that don't want to
  thread a grammar explicitly.
- `crates/core/src/eval_suite.rs::EvalCase` carries a `grammar: Option<String>`
  slot so future grammar-pinning eval cases can ship as data.

What's still missing: a JSON-Schema → GBNF converter
(so eval cases can pin a JSON Schema and have GBNF generated for them)
plus a `grammar_eval_cases()` fixture. Tracked in AGENTS.md "Next
things".

## Next things

Lives in AGENTS.md under "Next things to consider". Kept out
of this file by design: AGENTS.md is the operator's view (build
flags, env vars, run commands); this file is the *strategy* context
that survives across sessions. Dragging one into the other creates
drift on every commit. If something here contradicts AGENTS.md,
AGENTS.md wins — it's the one we read more often when something
is broken.

Top three currently (from AGENTS.md, verbatim):

1. JSON-Schema → GBNF converter + `eval_suite::grammar_eval_cases()`
   so the `grammar-rwkv` feature gets exercised end-to-end.
2. GGUF → ST shape fix in `scripts/gguf_to_st_converter/`
   (`a0/k_a/k_k/v0/w0/x_*` to `[1,1,emb]`, `r_k` to
   `(clock_count,head_dim)`).
3. Decide whether `audio` stays or gets removed now that
   `resource` and `infer` are gone.

## Open questions / unresolved

- The cleanup-time `free(): invalid size` segfault at process exit is
  documented under "Things we tried that didn't work" below. Doesn't
  affect inference — output is correct before the segfault fires.
- `RWKV_PIPELINE_CACHE_DIR` and `RWKV_QUANT_CACHE_DIR` env overrides
  are now honored (overridable fallbacks to `/tmp/roco-pipeline-cache`
  and `/tmp/roco-quant-cache`); we used to have to `rm -rf` those
  paths by hand to redirect caching elsewhere.
- Should we delete the remaining low-traffic scaffolding module
  (`audio`) or carry it as future-proofing for a second engine?
  Decision can wait until we know whether we're going to pursue a
  non-rwkv7 FFN backend. (`resource` and `infer` already removed
  since they had no callers.)

## Things we tried that didn't work

A log of dead-ends so we don't repeat the experiment. Written for the
next contributor who shows up with the same instinct.

### Debug-mode rwkv build hangs

`build_v7()` hangs indefinitely in **debug** on most consumer GPUs
(RTX 2050 / AMD RADV RENOIR confirmed). Cause is wgpu's
debug-build validation layers + slow unoptimized shader compilation
interacting with the GPU driver's Timeout Detection & Recovery. We
print a warning at runtime offering `--release`, which is the only
fix. `RWKV_ADAPTER=llvmpipe` is a usable debug fallback (extremely
slow, ~0.5 tok/s on the 2.9B), but not a debugging experience.

### 2.9B at FP16 OOMs the RTX 2050

The model's FP16 file is 5.6 GB; the card has 4 GB VRAM. We initially
trusted wgpu's `max_buffer_size` to drive the auto-quant heuristic.
The RTX 2050 reports `1048576` (1 TB) — clearly wrong, but can't be
overridden per-adapter. We now read **on-disk file size** as the
source of truth: files ≥ 1.5 GB always quantize (NF4 with coop-matrix,
Int8 otherwise). The 2.9B loads cleanly end-to-end after this change
("capital of France?" → "Paris", ~20 tok/s).

### GGUF → ST converter drops 3-D / matrix shapes

`scripts/gguf_to_st_converter/convert.py` (vendored from rwkv-harness)
converts the 0.1 B / 1.5 B model files into SafeTensors, but the
resulting tensors carry 1-D vectors where web-rwkv expects 3-D
`[1, 1, emb]` matrices for `a0 / k_a / k_k / v0 / w0 / x_*`, and flat
`[emb]` where web-rwkv expects `(clock_count, head_dim)` for `r_k`.
Inference blows up with
`TensorError(Shape([emb,1,1,1]), Shape([1,emb,1,1]))`. Ad-lib can be
cleanly fixed upstream with a `rwkv7.embedding_length` +
`rwkv7.wkv.head_size` read from GGUF metadata, but it's a separate
small change. The 2.9B `.st` shipped on disk *is* in the right layout —
that's why the 2.9B path works end-to-end today.

### mmap-based model load crashed producer on AMD iGPU

`memmap2` Mmap of the 5 GB model file worked on NVIDIA but hung or
segfaulted on the AMD RADV iGPU. Switched to `std::fs::read`. The
8 MB loaded file is still small by 2026 standards and disk-resident
caching makes subsequent reads effectively free.

## Run book (commands that currently work)

Verified on the dev-kit on Mon 2026-07-13. Don't trust these against
later commits without re-checking.

```bash
# Build everything
cargo build --workspace --release

# Run all tests (84 expected, 0 failures)
cargo test --workspace --release

# Spot-check GPU adapters + cooperative matrix availability
cargo run -p roco-core --features local-rwkv --example gpu_check

# Load the 2.9 B rwkv7 with stage-by-stage timeouts; deduplicated
# by /tmp/roco-pipeline-cache/key.bin on subsequent runs
cargo run -p roco-core --features local-rwkv --example rwkv_load_test --release

# Smoke-test: ask the 2.9 B model "capital of France?"
cargo run -p roco-core --features local-rwkv --example rwkv_test --release

# Run the eval suite against the same backend (writes JSON report)
cargo run -p roco-core --features local-rwkv --example eval_suite --release -- --backend rwkv

# Grammar-constrained smoke: feed the model a GBNF and confirm it
# never breaks the grammar (uses RWKV_GRAMMAR env var once a grammar
# file exists)
RWKV_GRAMMAR="$(cat path/to/grammar.gbnf)" \
cargo run -p roco-core --features grammar-rwkv --example rwkv_test --release
```

### What's not yet working (and the failure mode)

- Conversational eval under `crates/core/examples/eval_suite.rs`
  finishes with `free(): invalid size` segfault at exit. Inference
  result is correct; the segfault is in shutdown ordering of
  wgpu-on-the-actor-thread vs. main thread. See
  `Things we tried that didn't work` for the descriptive details.
- `crates/cli/src/main.rs` is 922 LOC; only a fraction runs in
  the rwkv path today. Worth keeping until we know which subcommand
  surface is real.
- Web frontend (`apps/web`) compiles; gateway (`crates/gateway`)
  ships axum; neither has been exercised against the live rwkv
  path in current checkout.
- 0.1 B / 1.5 B rwkv7 models can't be inference-tested; only the
  2.9 B works end-to-end on disk. Converter bug — see AGENTS.md.
</content>
