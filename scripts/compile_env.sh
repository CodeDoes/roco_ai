# shellcheck shell=bash
# Source this for the fastest *edit → recompile* loop:
#   source scripts/compile_env.sh
#
# Default: rustc incremental ON, sccache OFF.
# sccache cannot combine with incremental and hurts tight edit loops.
# For cold/CI cleans use: source scripts/compile_env.sh cold

export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
unset RUSTC_WRAPPER 2>/dev/null || true

if [ "${1:-}" = "cold" ]; then
  # Optional: sccache for clean builds across machines
  export CARGO_INCREMENTAL=0
  export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-10G}"
  if [ -x "${HOME}/.cargo/bin/sccache" ]; then
    export RUSTC_WRAPPER="${HOME}/.cargo/bin/sccache"
  elif command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER="$(command -v sccache)"
  fi
  echo "compile_env: COLD mode (sccache=${RUSTC_WRAPPER:-none}, incremental=0)"
else
  echo "compile_env: EDIT mode (incremental=1, no sccache)"
fi

# mold is selected via .cargo/config.toml on Linux.
