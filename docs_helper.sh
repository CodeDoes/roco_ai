#!/usr/bin/env bash
set -euo pipefail

# Documentation helper script for RoCo AI project
# Provides easy access to useful markdown documentation

DOCS_DIR="."
RFCDIR="./docs/rfc"

show_help() {
    cat << EOF
Documentation Helper for RoCo AI Project

Usage:
  ./docs_helper.sh [command] [options]

Commands:
  list                    List all documentation files with descriptions
  summary                 Show a summary of what documentation to read when
  search <term>           Search documentation for a specific term
  update                  Update this helper script (if needed)
  help                    Show this help message

Examples:
  ./docs_helper.sh list
  ./docs_helper.sh summary
  ./docs_helper.sh search "harness"
  ./docs_helper.sh search "RWKV"

This helps you navigate the project documentation efficiently.
EOF
}

list_docs() {
    echo -e "\nAvailable Documentation Files:\n"
    
    # Core documentation
    echo "Core Documentation:"
    echo "  AGENTS.md                       - Core project guide & developer reference"
    echo "  USE_CASES_AND_GAPS.md           - Crate analysis, tests, and coverage gaps"
    echo "  docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md - Architecture audit, DX & safety review"
    
    echo -e "\nRFC Documentation (docs/rfc/):"
    echo "  0001-local-ai-harness.md        - Local AI Harness Architecture"
    echo "  0004-harness-vs-fine-tuning-deep-dive.md - Harness engineering vs weight updates"
    echo "  0006-security-boundary-model.md - Sandbox & security boundaries"
    echo "  0007-rollback-detection-algorithm.md - Stuck-state detection & rollback algorithm"
    echo "  0008-offline-inference-protocol.md - Offline inference for secure deployment"
    echo "  0010-privacy-preserving-rag.md  - Privacy-preserving RAG"
    
    echo -e "\nOther Useful Files:"
    echo "  AGENTS.md                       - Main agent helper (updated with doc references)"
    
    echo -e "\nTIP: Use './docs_helper.sh summary' for reading recommendations"
}

show_summary() {
    echo -e "\nDocumentation Reading Guide for RoCo AI\n"
    
    echo "FOR NEWCOMERS (Start here):"
    echo "  1. AGENTS.md - Quick reference for architecture, commands, and crates"
    echo "  2. USE_CASES_AND_GAPS.md - Crate inventory, test suites & coverage gaps"
    echo "  3. docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md - DX, safety principles & audit"
    
    echo -e "\nFOR DEVELOPMENT/CONTRIBUTING:"
    echo "  • docs/rfc/0001-local-ai-harness.md - Harness execution architecture"
    echo "  • docs/rfc/0006-security-boundary-model.md - Security & path containment"
    echo "  • docs/rfc/0007-rollback-detection-algorithm.md - Retry & rollback algorithm"
    echo "  • docs/rfc/0008-offline-inference-protocol.md - RWKV offline inference protocol"
    echo "  • docs/rfc/0010-privacy-preserving-rag.md - Local context memory design"
    
    echo -e "\nQUICK TIPS:"
    echo "• Use './docs_helper.sh search <topic>' to find specific information"
    echo "• The docs/rfc/ directory contains architectural decision records"
    echo "• Most documentation is evergreen - check dates for relevance"
}

search_docs() {
    local query="${1:-}"
    if [[ -z "$query" ]]; then
        echo "Error: Please provide a search term"
        echo "Usage: ./docs_helper.sh search <topic>"
        return 1
    fi
    
    echo "Searching documentation for: '$query'"
    
    # Search in core docs
    local files=(
        "$DOCS_DIR/AGENTS.md"
        "$DOCS_DIR/USE_CASES_AND_GAPS.md" 
        "$DOCS_DIR/docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md"
        "$RFCDIR"/*.md
    )
    
    local found=false
    for file in "${files[@]}"; do
        if [[ -f "$file" ]]; then
            if grep -qi "$query" "$file"; then
                if [[ "$found" == false ]]; then
                    echo -e "\nSearch Results for '$query':"
                    found=true
                fi
                local filename=$(basename "$file")
                echo -e "\n[$filename]"
                grep -i "$query" "$file" | head -3 | sed 's/^/  /'
            fi
        fi
    done
    
    if [[ "$found" == false ]]; then
        echo "No matches found for '$query'"
    fi
}

update_self() {
    echo "This script is self-contained - no external updates needed"
    echo "To update documentation references, modify this script directly"
}

# Main execution
case "${1:-help}" in
    list)
        list_docs
        ;;
    summary)
        show_summary
        ;;
    search)
        search_docs "${2:-}"
        ;;
    update)
        update_self
        ;;
    help|*)
        show_help
        ;;
esac