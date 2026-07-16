#!/usr/bin/env python3
"""Simple bake test: load normalized data, bake, probe."""
import json
import sys
from pathlib import Path

# Add the crates path to import the engine
sys.path.insert(0, str(Path(__file__).parent.parent / "crates" / "engine" / "src"))

# This is a placeholder - we can't actually call Rust from Python
# So let's just verify the data loads correctly and print what we'd pass to bake_persona

def load_normalized(path: Path):
    """Load normalized JSONL and return list of (system, turns)."""
    conversations = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            row = json.loads(line)
            conversations.append((row["system"], row["turns"]))
    return conversations


def main():
    roleplay_data = load_normalized(Path("datasets/normalized/roleplay_normalized.jsonl"))
    pippa_data = load_normalized(Path("datasets/normalized/pippa_normalized.jsonl"))
    
    print(f"Loaded {len(roleplay_data)} roleplay conversations")
    print(f"Loaded {len(pippa_data)} PIPPA conversations")
    
    # Pick first 5 roleplay conversations as a test subset
    subset = roleplay_data[:5]
    
    print("\n=== Test subset (first 5 roleplay) ===")
    for i, (system, turns) in enumerate(subset):
        print(f"\n[{i}] System: {system[:80]}...")
        print(f"    Turns: {len(turns)}")
        if turns:
            print(f"    First: {turns[0]['user'][:60]} -> {turns[0]['assistant'][:60]}")
    
    # What we'd pass to bake_persona:
    # system = subset[0][0]  # Use first conversation's system
    # examples = []
    # for _, turns in subset:
    #     for t in turns:
    #         examples.append((t["user"], t["assistant"]))
    
    print(f"\n=== Would pass to bake_persona ===")
    print(f"System: {subset[0][0][:100]}...")
    total_turns = sum(len(turns) for _, turns in subset)
    print(f"Total (user, assistant) pairs: {total_turns}")
    print("\nThis is the minimal test: bake with 5 conversations (20 turns), probe with held-out prompt.")
    print("If that works, scale to 10, 20, 50 conversations and compare output quality.")


if __name__ == "__main__":
    main()
