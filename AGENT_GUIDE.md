# Agent Guide — RoCo AI (Short)

> Read `PROJECT_STRUCTURE.md` first. Read `AGENTS.md` for full philosophy.

## One-Page Rules

1. **Engine is frozen.** Do not modify `crates/inference/`, `engine/`, `grammar/`, `bnf-engine/`, `session/`, `message/`, `tools/`, `workspace/` unless a frontend feature is blocked. See `EDIT_GUIDE.md`.
2. **Build experience, not engine.** A feature is done only when a human can reach it through the real UI (CLI, desktop, or web) and drive it. See `roadmap/README.md`.
3. **No hidden control.** Every AI output is a suggestion until accepted. Always expose `accept / modify / skip / stop` visibly.
4. **Tests are part of the feature.** If you add a surface, add a test. No test = not done.
5. **Small steps.** One focused change per commit. Keep build green (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- --deny warnings`).
6. **Write progress.** After each meaningful change, append one line to `roadmap/progress.md`.
7. **Don't gold-plate.** Grammar tidy-ups, extra example binaries, or new crate scaffolding are not progress unless they change the human experience.

## Before You Edit Any File

- Check `EDIT_GUIDE.md` — is the file frozen or editable?
- Check `PROJECT_STRUCTURE.md` — does this file serve the user or the engine?
- Check `roadmap/README.md` — is this feature in the current focus?

## Quick Commands

```bash
# Test everything
run_tests.sh

# Check if your edit breaks anything
cargo check --workspace
cargo clippy --workspace --all-targets -- --deny warnings

# See what's frozen
cat EDIT_GUIDE.md | head -n 40
```

## Common Agent Traps (Avoid These)

- **Don't split `crates/agent/src/mecha_agent.rs`** without updating all imports in `lib.rs` and `cli/`.
- **Don't rename `crates/app/` or `apps/`** — the names are confusing but changing them breaks 20+ import paths.
- **Don't add new crate dependencies** to the workspace unless `Cargo.toml` is updated; the workspace resolver is strict.
- **Don't edit `.envrc`** — it sets `PATH` for the devenv shell; wrong edits break the build environment.
- **Don't modify `vendor/web-rwkv/`** directly; changes must go through `Cargo.toml` `[patch.crates-io]` if needed.
- **Don't assume `node_modules` is fresh.** Web apps (`apps/chat`, `apps/studio`) have their own `package-lock.json` and build steps.

## End-User Priority

The user is a writer, not a developer. Every change should make one of these easier:
- Starting a story (`start.sh`)
- Giving feedback (`f` in interactive mode)
- Editing the outline (`add`, `remove`, `move`)
- Seeing the result (clear output paths, readable files)
- Resuming a story (`--resume`)
