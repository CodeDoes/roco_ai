# RoCo AI

An AI-assisted collaborative writing tool. You give it a story premise, it generates chapters with outlines, world wikis, and quality checks — all driven by a local LLM (RWKV-7 2.9B).

---

## Quick Start

```bash
./start.sh "A lighthouse keeper discovers a hidden message in the fog"
# or just
./start.sh
```

Desktop GUI:

```bash
./run_desktop.sh
```

Run tests:

```bash
./run_tests.sh
```

---

## How It Works

```
You type a premise
    ↓
[roco CLI] or [desktop GUI]
    ↓
AppContext (crates/app/) — wires everything together
    ↓
StoryEngine (crates/agent-core/) — outline → chapters → quality check
    ↓
MechaAgent (crates/agent-core/) — plan-first loop: classify intent → derive plan → dispatch actions
    ↓
ModelBackend (crates/engine/ + crates/inference/) — RWKV-7 inference with BNF grammar constraints
    ↓
Workspace (crates/workspace/) — saves chapters, outlines, wikis to .roco/workspaces/
```

---

## Code Layout

| Crate | What it does |
|---|---|
| `crates/cli/` | CLI binary (`roco`), entry point, subcommands |
| `crates/ui/` | Desktop GUI (egui widgets: chat, pacing, editor, file tree, wiki, link graph, sessions, timeline) |
| `crates/app/` | Single surface primitive (`AppContext`) — CLI and GUI both use this |
| `crates/agent-core/` | Core agent loop: `MechanisticAgent` (plan-first) + `CommonAgent` (ReAct) |
| `crates/agent-story/` | Story pipeline: outline generation, chapter writing, quality eval, revision |
| `crates/engine/` | Model backend interface, completion requests, grammar-constrained generation |
| `crates/inference/` | RWKV-7 inference runtime (actor thread, sampling, quantization) |
| `crates/grammar/` | BNF grammar definitions + pipeline state machine |
| `crates/bnf-engine/` | Isolated kbnf grammar engine (patched from vendor) |
| `crates/workspace/` | Sandbox file storage for story outputs |
| `crates/session/` | Conversation session pool (`LruSessionPool`, max 8) |
| `crates/message/` | Message types shared across the system |
| `crates/tools/` | Tool definitions the agent can call |
| `crates/chat-common/` | Shared chat types |
| `crates/server/` | HTTP server for plugins (VSCode, Zed, Obsidian) |
| `crates/gateway/` | Backend gateway |
| `crates/infer-client/` | Remote inference client |
| `crates/inferd/` | Standalone inference daemon |
| `crates/validation/` | Story quality validation |

---

## Two Agent Patterns

1. **Plan-first (MechanisticAgent):** Used for structured story generation. Classifies intent with a grammar → derives a plan → dispatches actions → commits results. Deterministic, code-driven.

2. **ReAct (CommonAgent):** Used for exploratory chat. Open-ended observe → think → act loop driven by the model. Model decides when to emit `final_answer`.

---

## Key Design Rules

- **BNF grammar on every LLM call.** Raw prompting on RWKV-7 produces `thinking` contamination. Grammar constrains output.
- **No `.await` in GUI.** Desktop uses `block_on()` for backend calls inside `egui::update()`.
- **Standalone-first widgets.** Each widget in `crates/ui/` has its own `#[cfg(test)]` — test before wiring into the desktop.
- **Never redirect test output.** Use `run_tests.sh` directly; fix failures instead of hiding them.
- **Run tests after every edit** before committing.

---

## Environment

- **Model:** RWKV-7 2.9B, auto-detected or set via `RWKV_MODEL`
- **Adapter:** `RWKV_ADAPTER=llvmpipe` if GPU inference hangs in debug mode
- **Config:** `.roco/config.toml` or `~/.config/roco/config.toml`
- **Rust:** Edition 2021, resolver 2

---

## Commands

| Command | What |
|---|---|
| `./start.sh ["premise"]` | Full story generation |
| `./run_desktop.sh` | Desktop GUI |
| `./run_tests.sh` | `cargo check` + `clippy` + test build |
| `cargo test --workspace --no-run` | Compile all tests |
| `roco eval` | Run eval + snapshot |
| `roco bless` | Update oracle snapshots |
| `roco server` | Start HTTP server for editor plugins |
| `roco gui` | Desktop GUI (same as `run_desktop.sh`) |
| `roco interact` | Interactive chat session |
