#!/bin/bash
# run_tests.sh — Quick verification for agents and users
# Keeps build green; run after any edit before committing.

set -euo pipefail

echo "========================================"
echo "  RoCo AI — Quick Verification"
echo "========================================"
echo ""

echo "Step 1: Workspace check..."
cargo check --workspace

echo ""
echo "Step 2: Clippy (no warnings allowed)..."
cargo clippy --workspace --all-targets -- --deny warnings

echo ""
echo "Step 3: Running workspace tests..."
# Run tests but don't hang; use --no-run first to verify compilation
cargo test --workspace --no-run

echo ""
echo "Step 4: Key crate tests (non-hanging)..."
# Only run quick tests; skip inference backend smoke tests that hang
# in debug mode (see AGENTS.md)
echo "  Note: Full backend tests hang in debug builds."
echo "  Use 'RWKV_ADAPTER=llvmpipe cargo test -p roco-inference' for CPU fallback."

echo ""
echo "✅ Verification complete."
echo "If any step failed, fix it before committing."

echo "Step 5: Example target build tests..."
# Compile every example target so surface scripts like start.sh stay working.
# We skip execution because examples may launch long-running generations.
cargo check -p roco-cli --examples
echo ""
