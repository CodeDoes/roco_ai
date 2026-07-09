# RoCo AI — GUI (`roco-gui`)

A **native Rust** UI for the RoCo AI architecture, written with
[Dioxus](https://dioxuslabs.com/) (RSX = Rust's JSX, plus a plain CSS file).

This is one of two crates in the workspace:

| Crate | Dependency group | Role |
|-------|------------------|------|
| `roco_ai` (library) | **lib** | core orchestration, tracer, types |
| `roco-gui` (this) | **gui** | Dioxus front-end that renders it |

The GUI depends on the `roco_ai` library and **runs the real (mock-backed)
orchestration directly** — it builds an `Orchestrator` + `CollectingTracer`,
fans a task out, and renders the recorded `TraceEvent`s. There is no JSON
file and no separate server: the core crate *is* the data source.

## Scenes
1. **Stateful Core** — O(1) hidden state vs. a growing KV-cache.
2. **Fan-out** — orchestrator → parallel workers → verification gate → aggregate.
3. **ContextBudget** — the hard 4K split + 3000-token prompt cap.
4. **CapacityPool** — backend routing by capacity (GPU + CPU concurrent).

## Run it (web / WASM)

```bash
cargo install dioxus-cli          # one-time
dx serve                          # http://localhost:5173
```

`dx build` produces a static WASM bundle; `dx build --platform desktop`
(with the system webview installed) produces a native window.

## Verify it compiles without a browser

```bash
rustup target add wasm32-unknown-unknown
cargo check --target wasm32-unknown-unknown
```

The `roco_ai` library is compiled into the WASM build, so this also checks
that the core compiles for the web target.
