#!/bin/bash
# sccache wrapper — disables CARGO_INCREMENTAL so sccache can cache rustc compilations
export CARGO_INCREMENTAL=0
export SCCACHE_DIR="$HOME/.cache/sccache"
mkdir -p "$SCCACHE_DIR"
exec sccache "$@"
