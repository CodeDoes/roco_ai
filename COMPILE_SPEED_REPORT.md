# How to read the compile problem (mental model)

Two different problems get confused:

| # | Problem | What fixes it | What does **not** |
|---|---|---|---|
| A | **Link** is slow (minutes on ld.bfd) | **mold** via `clang -fuse-ld=mold` | Shared libs, more CPU |
| B | **Compile / typecheck** is slow | **Smaller crate graph**, fewer features, smaller crates | mold, dylibs, prefer-dynamic |

**This repo after the split:**

- (A) is **solved**. mold is on every rustc line (`scripts/verify_mold.sh`). Pure relink of `roco` is **~0.2 s**. You are not “underusing mold.”
- (B) is **partially solved**. GPU/GUI/axum are off the default path. Remaining cost is still rustc on **tokio + reqwest/rustls + agent + big CLI sources**, on a 2-core/2 GB host.

**“Not set up to be compiler-optimized”** — partly fair for the *product* crates:

- `tokio = { features = ["full"] }` was workspace-wide (now slimmed).
- Multi-thousand-line modules (`roco.rs` ~1.5k, `mecha_agent` ~1k) force large monomorphization units.
- Default CLI still needs HTTP client stack for `RemoteBackend`.

That is **crate design**, not linker setup. mold cannot fix (B).

---


## Recompile times (the metric that matters for editing)

With `CARGO_INCREMENTAL=1` and **no** sccache (now the default):

| Edit | check target | Time |
|---|---|---|
| CLI source | `-p roco-cli` | ~0.35 s |
| Agent source | `-p roco-agent` | ~0.6 s |
| Agent source | `-p roco-cli` | ~0.75 s (agent+cli; app skipped) |
| Engine source | `-p roco-cli` | ~1.0 s (wide fan-out) |

Key structural win: **`roco-app` ↛ `roco-agent`**. Agent work no longer dirties app.

See `docs/COMPILE_MENTAL_MODEL.md` and `./scripts/bench_recompile.sh`.


# Compile Speed Report — RoCo AI

**Date:** 2026-07-22 (revised after structural split)  
**Host:** 2× Xeon @ 2.60 GHz, **1.9 GiB RAM**, Linux  
**Toolchain:** rustc 1.97.1

> **Why the first pass was not enough:** mold/sccache/thin-LTO made the *same*
> graph link faster. A **2+ minute** default `cargo check` is still a failed
> edit loop. The real fix is **not compiling the heavy graph at all** on the
> default path.

---

## Headline numbers (this host)

| Path | Cold `cargo check` | Notes |
|---|---|---|
| **Default (CLI only, no GPU, no GUI, no axum)** | **~43 s** | was broken → then ~2.5 min with full graph |
| Default + warm sccache / clean `target` | **~42 s** | sccache helps less once graph is small + RAM-starved |
| No-op check | **0.2 s** | |
| Touch `roco-agent` → `check -p roco-agent` | **0.6 s** | |
| Touch CLI source → `check -p roco-cli` | **~4 s** | |
| `cargo build -p roco-cli` (debug binary) | **~19 s** | binary **43 MB** (was ~108 MB with GPU) |
| `check -p roco-cli --features net` | **~10 s** incremental over default | pulls axum/server/gateway |
| `check -p roco-inferd` (GPU stack) | **~108 s** | **expected** — wgpu/web-rwkv only here |
| Full workspace + desktop (old shape) | minutes | avoid for day-to-day |

On a normal 8–16 core / 32 GB laptop these default times should drop
substantially; the **structure** is what keeps GPU work off your critical path.

---

## Architecture change (the actual fix)

```
BEFORE (every `cargo build` / `roco`):
  roco-cli ──► roco-inference ──► web-rwkv ──► wgpu/naga   (huge)
           └──► eframe/egui                               (huge)
           └──► axum/server/gateway                       (large)

AFTER:
  default `cargo check` / `cargo build`:
    roco-cli ──► agent/app/engine/grammar/… ──► reqwest/tokio
                 ✗ no wgpu  ✗ no egui  ✗ no axum

  make build-inferd / cargo build -p roco-inferd:
    roco-inferd ──► roco-inference ──► web-rwkv ──► wgpu

  make build-desktop:
    roco-cli --features desktop ──► egui/eframe

  make build-net:
    roco-cli --features net ──► axum + roco-server + roco-gateway
```

### New / moved pieces

| Piece | Role |
|---|---|
| `crates/inferd` (`roco-inferd` bin) | **Only** default place that links wgpu/web-rwkv |
| `roco-cli` feature `desktop` | egui GUI |
| `roco-cli` feature `net` | HTTP server/gateway/story façade + LSP |
| `default-members = ["crates/cli"]` | `cargo build` does not walk the whole workspace |
| Story/grammar examples | Moved to `crates/inference/examples/` |
| Dead deps removed | `roco-inference` dropped from app/server/gateway/cli |

`roco interact` / `roco story` already used `RemoteBackend` + daemon spawn;
the daemon now starts **`roco-inferd`** instead of loading a model inside `roco`.

---

## Also kept from pass 1

| Knob | Effect |
|---|---|
| `.cargo/config.toml` → **clang + mold** | Final link: seconds vs ld.bfd multi-minute timeout |
| sccache via Makefile / `scripts/compile_env.sh` | Cross-clean cache (set `CARGO_INCREMENTAL=0` with it) |
| Profiles: dev line-tables, release **thin LTO**, `dist` = old fat LTO | Release iterates without CGU=1 tax |
| `handle_complete` fix in `actor.rs` | Workspace actually compiles |

---

## How to work day-to-day

```bash
# Fast path (default) — no GPU, no GUI
cargo check                 # or: make build
cargo build -p roco-cli
cargo check -p roco-agent   # leaf crate edit loop

# Local model server (slow compile, once)
make build-inferd
cargo run -p roco-inferd -- --port 8080

# HTTP surface
make build-net
cargo run -p roco-cli --features net -- gateway

# Desktop GUI
make build-desktop
cargo run -p roco-cli --features desktop -- gui

# Story examples (need inference crate)
cargo run -p roco-inference --example story_human --release

# Everything (CI)
cargo check --workspace
cargo check -p roco-cli --features net,desktop
```

Host packages: `clang mold` (+ optional `sccache`).

---

## Dependency graph size

| Target | Unique packages (`cargo tree`) |
|---|---|
| `roco-cli` default (now) | **~156** |
| `roco-cli` before split (with GPU dev-dep leak) | **~279** |
| `roco-cli --features desktop` (earlier) | **~401** |
| `roco-inferd` | **~240** (includes GPU) |
| `roco-agent` alone | **~57** |

---

## Why ~43 s cold still happens on this box

1. **2 cores / 1.9 GB RAM** — rustc + linker thrash; jobs capped at 2.
2. Default CLI still needs **tokio + reqwest + rustls** (daemon health checks,
   `RemoteBackend`). That is much smaller than wgpu but not free.
3. sccache cannot use rustc incremental; with a tiny machine the disk cache
   read path does not beat a warm `target/` no-op (0.2 s).

**Further cuts if you want sub-15 s cold on weak hardware:**
- Split a `roco-core` lib without reqwest; put daemon HTTP in `net` only.
- Make `interact` talk to a Unix socket served by inferd (drop rustls from default).
- Cranelift codegen backend for dev checks (nightly).
- More RAM/cores.

---

## Verification

```text
cargo check                         # OK ~43s cold / 0.2s noop
cargo check -p roco-cli --features net
cargo check -p roco-inferd          # OK ~108s cold (GPU)
cargo build -p roco-cli             # target/debug/roco ~43MB
```

---

## Files touched (this revision)

- `Cargo.toml` — members + **cli-only** `default-members`; profiles
- `crates/inferd/**` — **new** GPU daemon binary
- `crates/cli/Cargo.toml` — features `desktop`, `net`; no inference
- `crates/cli/src/bin/roco.rs` — cfg gates; server → inferd
- `crates/app/src/daemon.rs` — `ensure_inference_daemon` / `find_inferd`
- `crates/{app,server,gateway}/Cargo.toml` — drop unused `roco-inference`
- `crates/inference/examples/*` — story/grammar examples moved here
- `Makefile`, `devenv.nix`, `run_tests.sh`, `COMMANDS.md` — new targets/paths
- `.cargo/config.toml`, `scripts/compile_env.sh` — mold/sccache
- `crates/inference/src/actor.rs` — compile fix (reply/`?` control flow)

---

## Shared / linked libraries — what Rust can and cannot do

Short answer: **not like C++ `.so` intermediate libs**, and on this repo after
the graph split **you do not need them for link speed** (mold already relinks
`roco` in ~0.2 s).

### What people usually mean

| Model | C/C++ | Rust default |
|---|---|---|
| Intermediate crates as `.so` | common (`libfoo.so`) | **rare / painful** |
| Final binary links system libs | yes | yes (`libc`, etc.) |
| Dynamic standard library | glibc always dynamic | **opt-in** via `-C prefer-dynamic` → `libstd-*.so` |
| Plugin ABI | `.so` + stable C ABI | `cdylib` + `#[no_mangle]` / `abi_stable` |

### 1. `-C prefer-dynamic` (supported, limited win)

Links the rustup-provided **`libstd-*.so`** instead of stuffing libstd into
every binary.

```bash
make build-dyn
# or
./scripts/run_dyn.sh build -p roco-cli
./scripts/run_dyn.sh run -p roco-cli -- interact
```

Measured here:

| | Static (default) | `prefer-dynamic` |
|---|---|---|
| `target/debug/roco` size | ~44 MB | ~32 MB |
| Pure relink (mold) | ~0.2 s | ~0.2 s |
| Runtime | works | needs `LD_LIBRARY_PATH=$sysroot/lib` (script sets it) |

**It does not** turn `roco-agent`, `tokio`, `reqwest`, etc. into shared
objects. Those stay **rlib → statically linked into the bin**. Compile time
of the crate graph is unchanged.

### 2. `crate-type = ["dylib"]` on workspace crates (usually a bad idea)

You *can* set:

```toml
[lib]
crate-type = ["rlib", "dylib"]
```

Reality check:

- **No stable Rust ABI** — every rustc bump invalidates dylibs; all crates must
  be built with the same flags/features.
- **Diamond dependency hell** — `tokio` as both dylib and rlib = duplicate
  symbols / worse compile.
- **Often slower overall** — more linker work, worse optimization, fiddly
  `LD_LIBRARY_PATH` for every test.
- Cargo’s unit graph still **typechecks and monomorphizes** generics per
  crate; dylibs do not skip that.

Use dylibs for **true plugins** (load at runtime), not for “make my monorepo
compile faster.”

### 3. `cdylib` (good for C/FFI plugins, not for this edit loop)

```toml
[lib]
crate-type = ["cdylib"]
```

Right tool for: editor plugins, Python/Node native modules, WASM. Wrong tool
for: shaving seconds off `cargo check` of `roco-cli`.

### 4. What actually fixed *link* time here

**mold** (`-fuse-ld=mold`) + smaller binary after splitting GPU out.
Static relink of `roco` is already **~0.2 s**. Shared libs cannot beat that
enough to matter.

### 5. What actually fixed *compile* time here

**Not linking the heavy graph**: `roco-inferd` owns wgpu; `desktop` / `net`
features own egui/axum; default-members = CLI only.

### Recommendation for this repo

| Goal | Do this |
|---|---|
| Fast edit/check | default split path (`cargo check`) |
| Fast link | mold (already on) |
| Slightly smaller dev bins | optional `make build-dyn` |
| Ship plugins later | `cdylib` + C ABI at a boundary |
| Do **not** | convert workspace crates to `dylib` for speed |

