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

See AGENTS.md "Next things". They intentionally live there rather than
duplicating here — AGENTS.md is the operator's view, this file is for
strategy context that survives across multiple working sessions.

## Open questions / unresolved

- The "amateur cleanup segfault" at process exit — wgpu resources
  dropped on the wrong thread. Doesn't affect inference quality.
- Should we delete the scaffolding modules (`audio`, `infer`, `capacity`,
  `resource`) or carry them as future-proofing for a second engine?
  Decision can wait until we know whether we're going to pursue a
  non-rwkv7 FFN backend.
</content>
