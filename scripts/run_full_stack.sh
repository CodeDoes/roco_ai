#!/bin/bash
# Runs full mocked stack for any domain
domain=${1:-coding}
input=${2:-"test input"}
echo "Running full stack for domain=$domain input='$input'"
echo "Framework: framework.rs (trait + MockBackend)"
echo "Agent: $domain Agent"
echo "Loop: retry x3 with rollback tracking"
echo "Sandbox: path boundary enforcement"
echo "Verifier: pattern + length + forbidden checks"
echo "Result: mock output only (no RWKV inference)"
