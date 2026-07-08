# RoCo AI — Progress Log

_Last updated: 2026-07-08_

This document tracks what has been built in the RoCo AI Rust agent framework,
how the pieces fit together, and what remains.

## Environment & setup

- The project was copied from the Windows `C:` drive
  (`/run/media/kit/4997C4E96CC40CF7/Users/Kit/Documents/dev/roco_ai`) to
  `/run/media/kit/EXTHD/dev/roco_ai` (the `EXTHD` NTFS volume). The 1.3 GB
  `target/` build cache was excluded; all 98 source files were copied and
  verified byte-identical in file lists.
- **devenv is not currently usable here.** A stale/invalid GitHub PAT in
  `~/.config/nix/nix.conf` caused 401s (fixed by commenting it out), but the
  `languages.rust` rustup channel then tried to build Rust from source plus
  gdb/valgrind/clang over a very slow binary cache and failed. The committed
  `devenv.nix` uses `channel = "nixpkgs"` (prebuilt binary), which is correct
  and will build once the nix binary cache is healthy.
- **Day-to-day dev uses the system `cargo`** (1.96.0 at `~/.cargo`), which
  builds and tests the project directly on the NTFS drive without issue.

## Architecture built so far

| Module | Responsibility | Status |
|--------|---------------|--------|
| `engine.rs` | `ModelBackend` trait (the model seam) + `MockBackend`, token budget | Done |
| `agent.rs` | Orchestrator-Worker: 4K `ContextBudget`, decomposition, verification gates, escalation cascade, retry circuit breakers, fan-out + aggregation; **now tool-calling aware** | Done |
| `capacity.rs` | Capacity model + `CapacityPool` + backend routing | Done |
| `config.rs` | `Config` (provider/capacity/retry/context) from `model/default_config` | Done |
| `backends.rs` | HTTP model backends (`NvidiaBackend`, `KiloBackend`) — feature-gated | Done |
| `tools.rs` | `Tool` trait (dyn-compatible) + `ToolRegistry` (register/lookup/`schemas_json`/`validate_input`/`dispatch`) + example tools | Done |
| `grammar.rs` | GBNF generation for tool calls (`tools_to_gbnf` / `_with_think` / `_response`), `tools_to_xml`, `validate_grammar` | Done |
| `sandbox.rs` | Timeout-bounded command runner + `GuardPolicy` (Permissive / AllowList / DenyList) gate | Done |
| `policy.rs` | Composable `Policy` gate over `Action`s (sandbox guard, tool allowlist, human-in-the-loop) | Done |
| `toolcall.rs` | Parse `<tool_call>` from model output → vet via policy → dispatch via registry (tools) / sandbox (shell) | Done |
| `builtins.rs` | Concrete agent tools: `read`/`write`/`list` (workspace-rooted) + `bash` (via sandbox) | Done |
| `infer.rs` | Sampling (greedy/temp/top-k/top-p) + autoregressive generation loop behind `GenerativeModel` | Done |
| `eval.rs` | Eval-suite runner (16 named evals) | Done |
| `main.rs` | Smoke test (mock backend; optional live backends) | Done |
| `infer.rs` / `rwkv.rs` / `train.rs` | Sampling/batching, linear attention, training loop | **stub (model required)** |

## How the pieces fit (tool-use path)

```
model output (constrained to <tool_call> GBNF)
        │
        ▼
   toolcall::parse_tool_calls()        ← grammar.rs defines the GBNF
        │  Vec<ToolCall>
        ▼
   for each call:
     call.action()  ──►  policy.evaluate(action)
        │                    ├─ Deny  → blocked, not executed
        │                    ├─ Review → human-in-the-loop, not executed
        │                    └─ Allow → execute
        ▼
     shell tool? ──► sandbox.run_shell()      (sandbox.rs guard)
     other tool? ──► registry.dispatch()        (tools.rs)
        │
        ▼
   ToolExecutionResult  ──►  aggregated into WorkerOutput.tool_results
```

The `Orchestrator` now accepts an optional tooling bundle
(`with_tooling(tools, sandbox, policy)`); every spawned `Worker` parses and
executes tool calls from its model response, so the whole safety/tooling layer
is live end-to-end (exercised without a real model via a tool-emitting mock).

## Test status

`cargo test` → **59 passing** (17 foundation + 6 tools + 7 grammar + 7 sandbox
+ 5 policy + 4 toolcall + 1 worker-integration + 4 builtins + 8 infer). `cargo build --features
http-backends` also compiles.

## Commits this session

- `a86df42` — foundation + `tools.rs` + `grammar.rs`
- `72a88ec` — `sandbox.rs`
- `8890487` — `policy.rs`
- `dd21890` — `toolcall.rs` (glue)
- `32e3159` — agent orchestrator tool-call integration + README update + this doc
- `1807cb9` — concrete builtins tools (read/write/list/bash) + progress-doc update
- (latest) — `infer.rs`: sampling + autoregressive generation loop

## Remaining work

- **Model-dependent stubs:** `infer.rs` (sampling/batching), `rwkv.rs`
  (linear attention), `train.rs`. Inspiration for a local RWKV backend lives in
  `~/dev/rwkv-harness/rust/crates/{engine,session,vectorstore,inference_daemon}`.
- **Real backends:** download a 3B model and implement a `ModelBackend` that
  emits grammar-constrained tool calls (wiring `grammar.rs` into the request).
- **Eval harness:** run the 16 named evals in `evals/` end-to-end once a model
  is available (currently they write `result.json` only when driven live).

## Next suggested steps

1. Hook `execute_tool_calls` into the orchestrator worker loop (done at the
   `Worker` level) — optionally surface tool results in the prompt for
   multi-step agentic loops.
2. Implement a local RWKV `ModelBackend` (mirroring `rust/crates/engine`),
   with `grammar.rs` GBNF passed as a constrained-decoding grammar.
3. Add concrete tools (file read/write, bash already sandboxable) to mirror
   `rwkv-harness/src/tools/`.
