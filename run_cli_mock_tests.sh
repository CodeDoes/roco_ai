#!/usr/bin/env bash
set -euo pipefail

echo "================================================================="
echo "  RoCo AI — CLI & Interactive TUI Mock Test Suite"
echo "================================================================="

export ROCO_USE_MOCK_BACKEND=1
export RWKV_MODEL=mock-model

cargo test -p roco-cli --test mock_cli_subcommands --test mock_tui_interactive -- --nocapture

echo "================================================================="
echo "  All CLI and Interactive TUI Mock Tests Passed Successfully!"
echo "================================================================="
