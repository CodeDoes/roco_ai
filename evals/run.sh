#!/usr/bin/env bash
# Convenience wrappers for the RoCo eval harness.
#
# Usage:
#   ./evals/run.sh full            # full suite, local RWKV model (release)
#   ./evals/run.sh live            # full suite, stream every case to stdout
#   ./evals/run.sh one <name>      # ONE case, streamed live, immediate verdict
#   ./evals/run.sh one-story       # == one coherence_story
#   ./evals/run.sh one-fim         # == one fim_basic_bridge
#   ./evals/run.sh remote <name>   # ONE case against the running `roco server`
#   ./evals/run.sh filter <substr> # run the subset of cases matching <substr>
#   ./evals/run.sh bless           # promote latest snapshot outputs to oracles
#
# The model is auto-detected from models/*.st (or $RWKV_MODEL). Builds in
# --release because debug builds hang on most consumer GPUs.
set -euo pipefail

cd "$(dirname "$0")/.."

run() { cargo run -p roco-cli --example eval_suite --release -- "$@"; }

case "${1:-full}" in
  full)   run --backend rwkv ;;
  live)   run --backend rwkv --live ;;
  one)    shift; run --backend rwkv --one "${1:?usage: run.sh one <case-name>}" ;;
  one-story) run --backend rwkv --one coherence_story ;;
  one-fim)    run --backend rwkv --one fim_basic_bridge ;;
  remote) shift; run --backend remote --one "${1:?usage: run.sh remote <case-name>}" ;;
  filter) shift; run --backend rwkv --filter "${1:?usage: run.sh filter <substring>}" ;;
  bless)  cargo run --bin roco -- bless ;;
  *) echo "unknown target: $1" >&2; exit 1 ;;
esac
