#!/usr/bin/env bash
# Per-crate test runner. Compile only the chosen package, then run.
#
# Usage:
#   scripts/run_quick_tests.sh                  # default: touch current package from $PWD
#   scripts/run_quick_tests.sh roco-agent       # explicit
#   scripts/run_quick_tests.sh roco-agent core  # just core subcrate
#
# Why this exists:  `run_tests.sh` builds the whole workspace (~2-4min cold).
# During an agent-engine edit loop, we want <10s incremental on the touched
# crate. This script enforces that.
#
# Pair with: NEXTEST_PROFILE=quick scripts/run_quick_tests.sh roco-agent
#
# See .config/nextest.toml for profile details.

set -euo pipefail

# Default to current directory's crate name.
crate="${1:-}"

if [[ -z "$crate" ]]; then
    crate="$(basename "$(pwd)")"
    echo "==> Detected crate from cwd: ${crate}"
fi

workspace_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "${workspace_root}"

# 1. Verify the crate exists in the workspace.
if ! grep -q "\"${crate}\"" Cargo.toml; then
    echo "error: crate '${crate}' not found in workspace Cargo.toml" >&2
    exit 1
fi

# 2. Compile tests only (don't run them). This is the gate.
echo "==> cargo test --no-run -p ${crate}"
cargo test --no-run -p "${crate}"

# 3. Run via nextest if available, otherwise fall back to cargo test.
if command -v cargo-nextest >/dev/null 2>&1; then
    echo "==> cargo nextest run -p ${crate}"
    cargo nextest run -p "${crate}" "$@"
else
    echo "==> cargo test -p ${crate} (install cargo-nextest for faster runs)"
    cargo test -p "${crate}"
fi
