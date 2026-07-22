# Optimize for recompile time (edit → check)

## Mental model

```
you edit a .rs file
    → rustc re-typechecks that crate (and anything that `use`s its public API)
    → mold re-links only if you `cargo build` a bin
```

**mold** only helps the last step. **Recompile time** is almost entirely:
1. Which crates are dirtied (dependency fan-out)
2. Whether **incremental** is on
3. How big the dirty crate’s codegen unit is

## Measured on this machine (incremental=1, no sccache)

| You edit | `cargo check -p …` | What rebuilds | Typical |
|---|---|---|---|
| `crates/cli/src/*` | `roco-cli` | cli only | **~0.3–0.4 s** |
| `crates/agent/src/*` | `roco-agent` | agent only | **~0.6 s** |
| `crates/agent/src/*` | `roco-cli` | agent → cli (**not app**) | **~0.75 s** |
| `crates/app/src/*` | `roco-cli` | app → cli | **~0.45 s** |
| `crates/engine/src/*` | `roco-cli` | engine → … → everyone | **~1.0 s** |

Before decoupling: agent edit also rebuilt **app**. That edge is gone.

## Defaults (edit-loop)

```bash
source scripts/compile_env.sh     # incremental=1, no sccache
cargo check -p roco-cli           # or: make check
cargo check -p roco-agent         # leaf while editing agent
make watch CRATE=roco-agent       # if cargo-watch installed
./scripts/bench_recompile.sh      # measure your machine
```

**Do not** export `RUSTC_WRAPPER=sccache` during edit loops — sccache
forces `CARGO_INCREMENTAL=0` and makes every touch pay near-cold cost.

Cold/CI only:

```bash
source scripts/compile_env.sh cold
make build-cold
```

## What we changed for recompile

| Change | Why |
|---|---|
| **Default incremental=1**, sccache opt-in | sccache was the silent recompile killer |
| **`roco-app` no longer depends on `roco-agent`** | agent edits don’t rebuild app |
| **`VersionControl` moved to `roco-workspace`** | app only needed VC, not the whole agent |
| **`codegen-units = 256` in dev** | more parallel dirty-unit codegen |
| **mold** | bin link after check stays ~0.2–1 s |
| **GPU/GUI/net out of default graph** | less code in the cli unit when it *does* rebuild |

## Habits that keep recompiles fast

1. **Check the crate you edit**, not always the bin:
   `cargo check -p roco-agent` while in agent code.
2. **Don’t touch `engine` / `grammar` lightly** — they fan out to the whole tree.
3. Prefer new code in **new modules** over growing 1k-line files (smaller dirty CGUs).
4. Avoid changing **public signatures** in leaf crates when a private fn will do
   (signature changes force downstream re-typecheck).

## mold vs recompile

| | mold | incremental + graph |
|---|---|---|
| `cargo check` | unused (no link) | **this is the whole game** |
| `cargo build` after check | ~0.2 s link | crate rebuilds dominate |

You were right to want **recompile** optimization. That is graph + incremental,
not “more mold.”
