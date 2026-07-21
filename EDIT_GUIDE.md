# Edit Guide — Safe Edit Boundaries

> Before editing any source file, check its status here.

## Frozen Files (Edit Only to Fix Blockers)

These files contain the correct, tested engine. Touch them only if a frontend feature is blocked by a bug in the engine.

| File / Module | Reason Frozen | What to Avoid |
|---|---|---|
| `crates/inference/src/backend.rs` | RWKV backend; GPU/shader logic | Any structural change |
| `crates/inference/src/actor.rs` | Actor thread; state management | Changing message passing |
| `crates/engine/src/backend.rs` | `ModelBackend` trait definition | Changing trait signatures without updating all implementors |
| `crates/grammar/src/*.rs` | `BnfConstraint` + JSON→GBNF | Changing public API without updating `agent/` and `message/` |
| `crates/bnf-engine/src/lib.rs` | `kbnf` isolation crate | Any edit — this avoids `E0275` recursion |
| `crates/session/src/*.rs` | Session pools | Changing `LruSessionPool` behavior |
| `crates/message/src/*.rs` | Prompt formatting | Changing role prefixes or GBNF output |
| `crates/tools/src/*.rs` | Tool trait + builtins | Changing `Tool` trait |
| `crates/workspace/src/workspace.rs` | Sandbox boundary | Changing workspace paths or isolation logic |
| `crates/agent/src/story_engine.rs` | Core story pipeline (outline, plot state, persistence) | Removing or renaming core methods (`generate_chapter`, `expand_outline`, etc.) |

## Editable Files (Experience Layer)

These files are safe to edit for user-facing improvements. Always add tests when changing behavior.

| File / Module | What It Controls | Common Edits |
|---|---|---|
| `crates/cli/src/bin/roco.rs` | CLI binary wiring | Add subcommands, change output formatting |
| `crates/cli/src/interact.rs` | Interactive prompt loop | Improve prompts, add shortcuts |
| `crates/cli/examples/story_human.rs` | **Canonical user entry point** | Improve feedback parsing, add clearer messages |
| `crates/cli/examples/story_collaborative.rs` | Conversational variant | Adjust interaction flow |
| `crates/cli/examples/story_engine.rs` | Auto-mode demo | Adjust automation logic |
| `crates/ui/src/*.rs` | Desktop widgets | Add buttons, change colors, improve text rendering |
| `crates/app/src/*.rs` | Surface wiring | Add new capabilities (with caution — see below) |
| `crates/agent/src/interaction.rs` | Human-action mapping (`Interactive`/`Automatic`) | Add new interaction modes |
| `crates/agent/src/natural_feedback.rs` | NL feedback parsing | Improve `FeedbackParser` |
| `crates/agent/src/outline_editing.rs` | Outline add/remove/move commands | Improve error messages |
| `crates/agent/src/commentary.rs` | Bidirectional commentary | Add new commentary types |
| `crates/agent/src/chapter_steering.rs` | Mid-generation steering | Improve checkpoint logic |
| `crates/agent/src/quality.rs` | Quality metrics | Adjust scoring thresholds |

## Caution Zone (Edit With Tests)

These files are editable, but they have hidden dependencies. Any change must pass full workspace tests.

| File / Module | Hidden Dependency | Test Before Committing |
|---|---|---|
| `crates/app/src/lib.rs` | Used by `cli/`, `ui/`, `tui/`, `server/` | `cargo test -p roco-app` |
| `crates/app/src/context.rs` | Creates `AppContext` used everywhere | `cargo test -p roco-app` |
| `crates/app/src/daemon.rs` | Background daemon for desktop | Manual: `run_desktop.sh` |
| `crates/agent/src/lib.rs` | Re-exports 25 modules; any missing export breaks `cli/` | `cargo test --workspace` |
| `crates/agent/src/mecha_agent.rs` | Mechanistic controller; links to `plan.rs`, `context.rs`, `scheduler.rs` | `cargo test -p roco-agent` |
| `crates/agent/src/common_agent.rs` | ReAct loop; links to `mecha_agent.rs` | `cargo test -p roco-agent` |

## Never Edit These (They Break Everything)

- `Cargo.toml` workspace members (reordering is fine; removing is not)
- `.envrc` (environment loading)
- `vendor/web-rwkv/` (patched dependency — edit only through `Cargo.toml` patch)
- `devenv.nix` / `flake.nix` (development environment — edit only if you know Nix)
- `node_modules/` inside `apps/` (use `npm install`, not manual edits)

## Quick Edit Workflow

```bash
# 1. Check file status
cat EDIT_GUIDE.md | grep -A 2 "File / Module"

# 2. Make your change

# 3. Verify the workspace builds
run_tests.sh

# 4. Verify no clippy warnings
cargo clippy --workspace --all-targets -- --deny warnings

# 5. Write progress
# Append one line to roadmap/progress.md
```

## Example Error Pattern — do not change without tests
`crates/cli/examples/*.rs` call `Result<_, String>` APIs. Do NOT use `anyhow::Result<()>` in `main()`: `?` cannot coerce `String` in this repo's anyhow version.
Use `fn main() -> Result<(), Box<dyn std::error::Error>>` in example binaries, and verify with `cargo check -p roco-cli --examples`.

