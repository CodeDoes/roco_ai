# Project Structure — RoCo AI

> Quick orientation for anyone (human or agent) working on this repo.

## The Three "App" Concepts

| Path | Type | What it is | Safe to edit? |
|---|---|---|---|
| `crates/app/` | Rust library (`roco_app`) | Core primitive that wires sessions, workspace, and model backends. Every surface uses it. | Only for feature work; see `EDIT_GUIDE.md` |
| `crates/ui/` | Rust library (`roco_ui`) | Desktop widgets (egui) — pacing, chat, markdown editor, file tree, etc. | Yes — standalone-first rule applies |
| `apps/plugins/` | Editor plugins | VSCode, Zed, Obsidian plugins (talk to `crates/server` HTTP API) | Yes |

## Main Entry Points for End Users

| What the user wants | Command / Script |
|---|---|
| Write a story (CLI, interactive) | `start.sh` or `cargo run --release --example story_human -p roco-cli` |
| Run desktop GUI (egui) | `run_desktop.sh` or `cargo run --release -p roco-ui` (if binary exists) |
| Run tests quickly | `run_tests.sh` |
| Check agent-edit safety | Read `AGENT_GUIDE.md` and `EDIT_GUIDE.md` first |

## Directory Map

```
roco_ai/
├── Cargo.toml                 # Workspace: 14 crates
├── start.sh                    # Quick CLI entry
├── run_desktop.sh              # Desktop GUI entry
├── run_tests.sh                # Quick test run
├── PROJECT_STRUCTURE.md        # This file
├── AGENT_GUIDE.md              # Short agent behavior guide
├── EDIT_GUIDE.md               # Which files are frozen / editable
├── USER_GUIDE.md               # End-user orientation
├── README.md                   # Public landing page
├── QUICKSTART.md               # 5-minute start
├── INSTALL.md                  # Detailed setup
├── COMMANDS.md                 # CLI reference
├── PLUGINS.md                  # Plugin setup
├── API.md                      # Server API reference
│
├── crates/                     # Rust libraries (14 crates)
│   ├── agent/                  # Story engine, interaction, quality, outline editing
│   ├── app/                    # Core surface primitive (`AppContext`)
│   ├── cli/                    # `roco` binary + examples (`story_human.rs` is canonical)
│   ├── ui/                     # Desktop widgets (egui)
│   ├── engine/                 # Model backend trait + evaluation
│   ├── inference/              # RWKV backend (WGPU / Vulkan)
│   ├── grammar/                # BNF-constrained decoding
│   ├── workspace/              # Sandbox workspace (`Workspace`)
│   ├── session/                # Session stores
│   ├── message/                # Prompt formatting
│   ├── tools/                  # Tool trait + builtins
│   ├── bnf-engine/             # `kbnf` isolation crate
│   ├── infer-client/           # HTTP client for inference API
│   ├── server/                 # HTTP server (story API + LSP)
│   ├── gateway/                # API gateway (rate limit, auth)
│   └── chat-common/            # Shared chat types
│
├── apps/plugins/               # Editor plugins
│   ├── vscode/
│   ├── zed/
│   └── obsidian/
│
├── roadmap/                    # Living plan — READ THIS FIRST for any feature work
│   ├── v1.md                   # v1 milestone checklist
│   ├── ux.md                   # Human experience spec
│   ├── progress.md             # Append-only change log
│   └── blocked.md              # Parking lot for open questions
│
├── docs/                       # Long-form docs (currently empty)
│
├── GBNF/                       # Grammar files for constrained decoding
├── templates/                  # Prompt templates
├── datasets/                   # In-tree eval / training data
├── scripts/                    # Model conversion scripts
├── evals/                      # Benchmark results
└── assets/vocab/               # RWKV vocab JSON
```

## Frozen vs Editable (High-Level)

**Frozen (build on, don't modify unless blocking a feature):**
- `crates/inference/` — RWKV backend
- `crates/engine/` — `ModelBackend` trait, eval harness
- `crates/grammar/` — `BnfConstraint`, schema conversion
- `crates/bnf-engine/` — `kbnf` isolation crate
- `crates/session/`, `message/`, `tools/`, `workspace/`
- `crates/agent/src/story_engine.rs` — core story pipeline (but interaction surfaces are editable)

**Editable (experience layer):**
- `crates/cli/src/bin/roco.rs` — CLI wiring
- `crates/cli/examples/*.rs` — Example entry points
- `crates/ui/src/*.rs` — Desktop widgets
- `crates/app/src/*.rs` — Surface wiring (with caution)
- `apps/plugins/*` — Editor plugins
- `roadmap/`, docs, guides, READMEs