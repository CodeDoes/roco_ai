#!/usr/bin/env bash
# Confirm rustc invocations actually pass -fuse-ld=mold.
set -euo pipefail
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "$(dirname "$0")/.."
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
rm -f target/debug/roco target/debug/deps/roco-*
cargo build -p roco-cli -v >"$tmp" 2>&1 || { tail -20 "$tmp"; exit 1; }
if grep -q 'fuse-ld=mold' "$tmp"; then
  echo "OK: mold is on the rustc link line"
  grep -oE 'linker=[^ ]+|link-arg=-fuse-ld=[a-z]+' "$tmp" | sort -u
  exit 0
fi
echo "FAIL: mold not found on link line"
grep 'crate-name roco ' "$tmp" | tail -1 | tr ' ' '\n' | grep -E 'linker|fuse|mold|link' || true
exit 1
