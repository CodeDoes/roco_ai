#!/usr/bin/env bash
# Build/run with dynamically linked libstd (smaller binary, same crate graph).
# Usage:
#   scripts/run_dyn.sh build -p roco-cli
#   scripts/run_dyn.sh run -p roco-cli -- interact
#   scripts/run_dyn.sh check
set -euo pipefail
SYSROOT="$(rustc --print sysroot)"
# Prefer the lib dir that contains libstd-*.so
if [ -d "$SYSROOT/lib" ]; then
  export LD_LIBRARY_PATH="$SYSROOT/lib:${LD_LIBRARY_PATH:-}"
fi
# Also cover the rustlib target lib (some layouts put .so there only)
TARGET_LIB="$SYSROOT/lib/rustlib/$(rustc -vV | awk '/host:/{print $2}')/lib"
if [ -d "$TARGET_LIB" ]; then
  export LD_LIBRARY_PATH="$TARGET_LIB:$LD_LIBRARY_PATH"
fi
export RUSTFLAGS="${RUSTFLAGS:-} -C prefer-dynamic"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-dyn}"
exec cargo "$@"
