#!/usr/bin/env bash
# Measure edit→check latency for common touch points.
set -euo pipefail
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=1
unset RUSTC_WRAPPER || true
export CARGO_TERM_COLOR=never

cargo check -p roco-cli -q

bench() {
  local label=$1 file=$2 target=$3
  touch "$file"
  local start end ms
  start=$(date +%s%N)
  cargo check -p "$target" -q
  end=$(date +%s%N)
  ms=$(( (end - start) / 1000000 ))
  printf '%-20s  touch %-40s  check -p %-16s  %5d ms\n' "$label" "$file" "$target" "$ms"
}

echo "=== recompile benchmark (incremental=1, no sccache) ==="
bench cli_leaf     crates/cli/src/rich_output.rs   roco-cli
bench cli_interact crates/cli/src/interact.rs      roco-cli
bench agent_via_cli crates/agent/src/util.rs       roco-cli
bench agent_leaf   crates/agent/src/util.rs        roco-agent
bench engine_cli   crates/engine/src/lib.rs        roco-cli
bench engine_leaf  crates/engine/src/lib.rs        roco-engine
bench app_cli      crates/app/src/lib.rs           roco-cli
