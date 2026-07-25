#!/usr/bin/env bash
set -uo pipefail

# ─── scout.sh ────────────────────────────────────────────────────────────────
# Live project introspection — run any time for the *actual* state.
# No stale docs. No guessing.
# Usage:
#   ./scout.sh                    # full report
#   ./scout.sh crates             # crate list + sizes + dep graph
#   ./scout.sh tests              # test modules per crate
#   ./scout.sh types <pattern>    # grep for types/structs/traits
#   ./scout.sh deps <crate>       # dep tree for one crate
#   ./scout.sh api                # public API surface per crate
#   ./scout.sh summary            # brief overview
# ──────────────────────────────────────────────────────────────────────────────

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

MODE="${1:-full}"

# ── helpers ──────────────────────────────────────────────────────────────────
bold()  { printf "\033[1m%s\033[0m" "$*"; }
dim()   { printf "\033[2m%s\033[0m" "$*"; }
red()   { printf "\033[31m%s\033[0m" "$*"; }
green() { printf "\033[32m%s\033[0m" "$*"; }

header() { echo ""; bold "$1"; echo ""; dim "──────────────────────────────────────────────"; echo ""; }

list_crates() {
  for d in crates/*/; do
    name=$(basename "$d")
    pkg_name=$(grep -m1 '^name' "$d/Cargo.toml" 2>/dev/null | sed 's/name = "\(.*\)"/\1/')
    lines=$(find "$d" -name '*.rs' -exec cat {} + 2>/dev/null | wc -l)
    printf "  %-20s %-25s %6d lines\n" "$name" "${pkg_name:-}" "$lines"
  done
}

list_tests() {
  for d in crates/*/; do
    name=$(basename "$d")
    tests=$(grep -rn '#\[cfg(test)\]' "$d/src" 2>/dev/null | wc -l || true)
    integ=$(ls "$d/tests/"*.rs 2>/dev/null | wc -l || true)
    if [ "$tests" -gt 0 ] || [ "$integ" -gt 0 ]; then
      echo "  $name: $tests unit tests, $integ integration tests"
    fi
  done
}

dep_graph() {
  local target="${1:-all}"
  for d in crates/*/; do
    name=$(basename "$d")
    pkg=$(grep -m1 '^name' "$d/Cargo.toml" 2>/dev/null | sed 's/name = "\(.*\)"/\1/')
    deps=$(grep -E '^(roco-|roco_)' "$d/Cargo.toml" 2>/dev/null | sed 's/ = .*//' | tr '\n' ' ')
    if [ "$target" = "all" ] || echo "$name" | grep -qi "$target"; then
      echo "  $(bold "$pkg") ($name)"
      [ -n "$deps" ] && echo "        depends on: $deps" || echo "        (no workspace deps)"
    fi
  done
}

public_api() {
  for d in crates/*/; do
    name=$(basename "$d")
    pkg=$(grep -m1 '^name' "$d/Cargo.toml" 2>/dev/null | sed 's/name = "\(.*\)"/\1/')
    pubs=$(grep -rn '^pub' "$d/src" 2>/dev/null | grep -v '#\[' | grep -v '//!' | head -5 || true)
    if [ -n "$pubs" ]; then
      echo "  $(bold "$pkg"):"
      echo "$pubs" | while IFS= read -r line; do
        echo "    $line"
      done
      echo ""
    fi
  done
}

types_grep() {
  local pattern="${1:-pub (struct|enum|trait|type|fn|mod)}"
  for d in crates/*/; do
    name=$(basename "$d")
    matches=$(grep -rn "$pattern" "$d/src" 2>/dev/null | head -20)
    if [ -n "$matches" ]; then
      echo "  $(bold "$name"):"
      echo "$matches" | while IFS= read -r line; do
        echo "    $line"
      done
      echo ""
    fi
  done
}

summary() {
  echo "  $(bold "RoCo AI") — $(date '+%Y-%m-%d %H:%M')"
  echo ""
  echo "  Crates: $(ls -d crates/*/ 2>/dev/null | wc -l)"
  echo "  Total Rust lines: $(find crates -name '*.rs' -exec cat {} + 2>/dev/null | wc -l)"
  echo "  Test modules: $(grep -rn '#\[cfg(test)\]' crates --include='*.rs' 2>/dev/null | wc -l)"
  echo "  Integration tests: $(find crates -path '*/tests/*.rs' 2>/dev/null | wc -l)"
  echo ""
  echo "  Features:"
  grep -rn '^name *= *"' crates/*/Cargo.toml 2>/dev/null | sed 's|.*/Cargo.toml:name = "\(.*\)"|    \1|'
  echo ""
  echo "  Binary targets:"
  grep -rl '\[\[bin\]\]' crates/*/Cargo.toml 2>/dev/null | while read f; do
    crate=$(basename $(dirname "$f"))
    name=$(grep -A1 '\[\[bin\]\]' "$f" 2>/dev/null | grep 'name' | sed 's/name = "\(.*\)"/\1/')
    echo "    $crate → $name"
  done
}

# ── run ──────────────────────────────────────────────────────────────────────
dead_refs() {
  local pattern="${1:-agent-core|agent-story|roco_full_stack|roco_agent_core|roco_agent_story}"
  local found=0
  shopt -s nullglob 2>/dev/null || true
  for f in *.md docs/*.md; do
    [ -f "$f" ] || continue
    matches=$(grep -nE "$pattern" "$f" 2>/dev/null || true)
    if [ -n "$matches" ]; then
      echo "  $(bold "$f"):"
      echo "$matches" | while IFS=: read -r line rest; do
        echo "    line $line: $rest"
      done
      found=$((found+1))
    fi
  done
  [ "$found" -eq 0 ] && green "  ✓ No stale references found"
}

case "$MODE" in
  crates|list)
    header "Crates (name → package → lines)"
    list_crates
    ;;
  tests)
    header "Test modules per crate"
    list_tests
    ;;
  deps|dep|depends)
    header "Dependency graph"
    dep_graph "${2:-all}"
    ;;
  api|public)
    header "Public API surface (top 5 per crate)"
    public_api
    ;;
  types|type)
    header "Types matching: $2"
    types_grep "${2:-pub (struct|enum|trait|type)}"
    ;;
  summary|brief)
    summary
    ;;
  full|"")
    summary
    header "Crates"
    list_crates
    echo ""
    header "Dependencies"
    dep_graph all
    echo ""
    header "Tests"
    list_tests
    echo ""

    # check for dead doc refs
    header "Dead doc checks"
    dead=0
    for ref in "agent-core" "agent-story"; do
      if grep -q "$ref" *.md 2>/dev/null; then
        red "  ⚠  '$ref' still referenced in markdown but not a crate"
        dead=$((dead+1))
      fi
    done
    [ "$dead" -eq 0 ] && green "  ✓ No stale crate refs found" || echo "  (run ./scout.sh dead for details)"
    echo ""

    header "All doc references to non-existent crates"
    grep -n "agent-core\|agent-story\|agent_tools\|roco-agent-core\|roco-agent-story\|roco_full_stack" *.md 2>/dev/null | head -30 || echo "  (none)"
    echo ""

    header "Quick commands"
    echo "  ./scout.sh crates       — list all crates with line counts"
    echo "  ./scout.sh tests        — test modules per crate"
    echo "  ./scout.sh deps <name>  — dep graph for one crate"
    echo "  ./scout.sh api          — public API per crate"
    echo "  ./scout.sh types <pat>  — grep types/structs/traits"
    echo "  ./scout.sh dead         — find stale crate refs in docs"
    echo "  ./scout.sh summary      — this overview"
    ;;
  dead|stale)
    header "Stale references in docs"
    dead_refs "${2:-agent-core|agent-story|roco_full_stack|roco_agent_core|roco_agent_story|roco_full_stack}"
    ;;
esac