#!/bin/bash
# run_tests.sh — Quick verification for agents and users
# Keeps build green; run after any edit before committing.
#
# All cargo invocations are routed through `rustup run stable` so they
# always use the same rustc + cargo + cargo-clippy version, regardless
# of where Nix / system installs have shadowed the toolchain on PATH.
# See `rust-toolchain.toml` for the project pinning.

set -euo pipefail
shopt -s inherit_errexit 2>/dev/null || true

# ── Toolchain dispatch ────────────────────────────────────────────────────
# Force PATH to prefer rustup's toolchain bin before any Nix-installed
# rust override. Without this, cargo resolves companion binaries
# (cargo-clippy, clippy-driver, rustfmt) to the Nix-installed 1.95.0
# build, which mixes versions with rustup's 1.96+ and produces E0514
# "compiled by an incompatible rustc" errors when sccache picks up
# rmeta files from whichever rustc ran first.
RUSTUP_BIN="$(rustup which rustc 2>/dev/null | xargs -I{} dirname {})"
if [ -n "$RUSTUP_BIN" ] && [ -d "$RUSTUP_BIN" ]; then
    PATH="$RUSTUP_BIN:$PATH"
    export PATH
fi
unset CARGO_HOME_PREFER_STABLE 2>/dev/null || true
# rust-toolchain.toml pins the channel; explicit override here so the
# script behaves identically even when run outside the dev shell.
export RUSTUP_TOOLCHAIN="stable"

echo "========================================"
echo "  RoCo AI — Quick Verification"
echo "========================================"
echo ""
echo "Toolchain:  $(rustup run stable rustc --version)"
echo "Clippy:     $(rustup run stable cargo-clippy --version 2>/dev/null || echo 'n/a')"
echo ""

# ── Step 1: workspace check ──────────────────────────────────────────────
echo "Step 1: Workspace check..."
cargo check --workspace

# ── Step 2: clippy surfaced as warnings, not gate ───────────────────────
# The v1 roadmap marks `--deny warnings` as ⚠️ "non-blocking — 44 unused
# fn warnings" today. We run clippy but don't fail on its findings; the
# output below is the canonical signal so the writer can audit at a glance
# without it blocking day-to-day iteration.
echo ""
echo "Step 2: Clippy (informational)..."
set +e   # don't exit on clippy's findings; we *do* want to keep going
cargo clippy --workspace --all-targets >/tmp/clippy.log 2>&1
clippy_status=$?
set -e
if [ "$clippy_status" -eq 0 ]; then
    echo "  ✅ Clippy clean."
else
    echo "⚠ Clippy reported findings. See /tmp/clippy.log."
    # Use awk so SIGPIPE from head doesn't fail the script.
    echo "  Counts by kind:"
    awk '
        /^error\[/ {errs++}
        /^warning:/ {warns++}
    END {
        printf "    errors=%d  warnings=%d\n", errs+0, warns+0
    }' /tmp/clippy.log
    echo "  First 5 unique lints:"
    { grep '^warning: ' /tmp/clippy.log; grep '^error: ' /tmp/clippy.log; } | sort -u | head -5 || true
fi

# ── Step 3: compile tests, don't run hung backends ───────────────────────
echo ""
echo "Step 3: Workspace test compilation (no-run)..."
cargo test --workspace --no-run

# ── Step 4: example targets (used by start.sh + surfaces) ────────────────
echo ""
echo "Step 4: Example target build (start.sh, story_*, grammar_smoke)..."
cargo check -p roco-inference --examples
cargo check -p roco-cli

# ── Step 5: rustfmt check ────────────────────────────────────────────────
# Drift in formatting is a low-cost quality signal; catch it before the
# user commits a wall of `cargo fmt` noise.
if command -v rustfmt >/dev/null 2>&1; then
    echo ""
    echo "Step 5: Format check..."
    if ! cargo fmt --all -- --check 2>/tmp/fmt.log; then
        echo "❌ Formatting drift detected. Run \`cargo fmt --all\` and rerun."
        echo "First few diffs:"
        head -20 /tmp/fmt.log
        exit 1
    fi
else
    echo ""
    echo "Step 5: Format check — skipped (rustfmt not installed)"
fi

echo ""
echo "✅ Verification complete."
echo "If any step failed, fix it before committing."
echo ""
echo "Note: full backend tests hang in debug builds (see AGENTS.md §I)."
echo "      Use 'RWKV_ADAPTER=llvmpipe cargo test -p roco-inference' for CPU fallback."
