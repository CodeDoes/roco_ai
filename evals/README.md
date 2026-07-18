# `evals/` — model evaluation harness

This folder holds the outputs of the RoCo eval suite, which benchmarks the
inference backend (RWKV-7 g1h 2.9B) on concrete capabilities: instruction
following, coherence, repetition avoidance, format adherence, FIM (fill-in-
the-middle) bridging, and throughput.

The harness is the `eval_suite` example in `crates/cli/examples/eval_suite.rs`,
backed by the framework in `crates/engine/src/eval.rs` + the case definitions in
`crates/engine/src/cases.rs`.

## Layout

```
evals/
├── README.md                 # this file
├── run.sh                    # convenience wrappers for common invocations
└── results/
    ├── latest.json           # full structured report of the most recent run
    ├── latest.snapshot.json  # machine-readable output snapshot (bless source)
    ├── latest.oracle.json    # expected outputs for every case that has one
    ├── latest.mismatches.txt # cases whose output diverged from their oracle
    ├── latest_trace.txt      # raw token-by-token generation trace
    ├── fim.oracle.json       # FIM-bridge oracle subset
    ├── fim.mismatches.txt    # FIM-bridge mismatches
    └── baselines/            # historical reference runs (dated, never overwritten)
        ├── rwkv-2.9b.json
        ├── rwkv-2.9b-v2.json
        └── rwkv-2.9b-baseline.json
```

`latest.*` files are overwritten on every run. The `baselines/` directory is a
curated, append-only archive of reference runs used to track model/quant
regressions over time — copy a `latest.json` there with a descriptive name when
you want to pin a milestone.

## Running evals

The model is auto-detected from `models/*.st` (or `$RWKV_MODEL`). Build in
**release** — debug builds hang on most consumer GPUs.

```bash
# Full suite against the local RWKV model (writes evals/results/latest.*)
roco eval                          # devenv script -> eval_suite --backend rwkv
# or directly:
cargo run -p roco-cli --example eval_suite --release -- --backend rwkv

# Stream every case's tokens to stdout, live, as it generates
cargo run -p roco-cli --example eval_suite --release -- --backend rwkv --live

# Run EXACTLY ONE case, streamed token-by-token, with live pass/fail checks.
# Great for "show me it working" or debugging a single failing case.
cargo run -p roco-cli --example eval_suite --release -- --backend rwkv \
    --one coherence_story

# Target the singleton inference server instead of loading a local model
# (useful when `roco server` is already holding the GPU):
cargo run -p roco-cli --example eval_suite --release -- --backend remote --one coherence_story

# Filter the full suite to a category or name substring
cargo run -p roco-cli --example eval_suite --release -- --backend rwkv --filter fim
```

### `--one` vs `--filter`

- `--one <substr>` — picks the single case whose name contains `<substr>`,
  streams it live with an immediate verdict, and exits. One eval, no report file.
- `--filter <substr>` — runs every case whose name/description/category matches,
  writes the usual `latest.*` artifacts.

## Oracle / bless workflow

Every case may carry an `oracle: Some("...")` expected output (see
`crates/engine/src/cases.rs`). After a run, `latest.oracle.json` records the
actual outputs and `latest.mismatches.txt` lists any that diverged.

When the current output is *acceptable* (the model improved or the oracle was
wrong), promote the current outputs to the new reference:

```bash
roco bless            # rewrites the `oracle:` fields in cases.rs from latest.snapshot.json
```

`bless` reads `evals/results/latest.snapshot.json`, finds each case's
`oracle:` line in `crates/engine/src/cases.rs`, and replaces it. Rebuild
afterwards so the new oracles take effect. A specific snapshot can be blessed
with `roco bless --snapshot <path>`.

## Adding a case

Append an `EvalCase` to the relevant builder in `crates/engine/src/cases.rs`
(`default_eval_suite`, `message_eval_cases`, `fim_eval_cases`, …). Fields:

| field | meaning |
|---|---|
| `name` | unique id (substring-matched by `--one`/`--filter`) |
| `category` | `smoke \| instruction \| coherence \| repetition \| throughput \| format \| context \| fim` |
| `system` / `prompt` | the chat turns fed to the model |
| `expected_hints` | substrings that must appear in the output |
| `forbidden_strings` | substrings that must NOT appear (e.g. `<think>` leaks) |
| `min_output_chars` | minimum length gate |
| `max_tokens` / `temperature` | generation params |
| `grammar` | optional GBNF grammar (mask built from vocab + this at runtime) |
| `prefill` | text appended after `Assistant:` (e.g. `<think></think>` to suppress think-leak) |
| `session` / `preserve_state` | recurrent-state resume / persist (state-tuning) |
| `oracle` | optional expected output used by the bless workflow |

## Grammar coverage

Per the AGENTS.md "Grammar-First Principle", every model call should be bounded
by a BNF grammar so contamination (e.g. `<think>` meta-commentary) cannot occur.
Cases that exercise free-form prose (the FIM bridge, story continuation) are
bounded by `max_tokens` + per-token stop-conditions as an interim measure; they
are flagged for replacement with per-handler grammars from `GBNF/`.
