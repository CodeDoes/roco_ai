#!/usr/bin/env python3
"""Augment plot_overview dataset by generating variations of existing outlines."""
import json
from pathlib import Path

SOURCE_DIR = Path("../rwkv-harness/src/agents/storyteller/examples")
OUTPUT_FILE = Path("datasets/tasks/plot_overview.jsonl")

def load_md(path: Path) -> str:
    """Load markdown file, strip frontmatter."""
    if not path.exists():
        return ""
    content = path.read_text(encoding="utf-8")
    if content.startswith("---"):
        end = content.find("---", 3)
        if end != -1:
            content = content[end+3:].strip()
    return content

def main():
    OUTPUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    examples = []
    
    for story_dir in sorted(SOURCE_DIR.iterdir()):
        if not story_dir.is_dir():
            continue
        
        plan_file = story_dir / "_plan.md"
        user_file = story_dir / "_user.md"
        
        if not plan_file.exists() or not user_file.exists():
            continue
        
        user_input = load_md(user_file)
        plan_output = load_md(plan_file)
        
        # Original example
        examples.append({
            "system": "You are a story planner. Create a detailed outline for the story based on the user's request. Include premise, chapter structure, and wiki entries to create.",
            "turns": [{"user": user_input, "assistant": plan_output}]
        })
        
        # Variation 1: Different workspace name
        user_var1 = user_input.replace("workspace/shadow-realm", "workspace/alternate-shadow")
        user_var1 = user_var1.replace("workspace/starfall", "workspace/alternate-starfall")
        user_var1 = user_var1.replace("workspace/dragon-realm", "workspace/alternate-dragon")
        
        # Extract premise and modify slightly
        plan_lines = plan_output.split('\n')
        premise_idx = next((i for i, line in enumerate(plan_lines) if '## Premise' in line), -1)
        if premise_idx != -1 and premise_idx + 1 < len(plan_lines):
            premise = plan_lines[premise_idx + 1]
            # Add variation to premise
            var_premise = premise + " (alternate version)"
            plan_var1 = '\n'.join(plan_lines[:premise_idx+1] + [var_premise] + plan_lines[premise_idx+2:])
            
            examples.append({
                "system": "You are a story planner. Create a detailed outline for the story based on the user's request. Include premise, chapter structure, and wiki entries to create.",
                "turns": [{"user": user_var1, "assistant": plan_var1}]
            })
        
        # Variation 2: Different request style
        user_var2 = f"Write an outline for: {user_input}"
        examples.append({
            "system": "You are a story planner. Create a detailed outline for the story based on the user's request. Include premise, chapter structure, and wiki entries to create.",
            "turns": [{"user": user_var2, "assistant": plan_output}]
        })
        
        # Variation 3: More detailed request
        user_var3 = f"{user_input}\n\nPlease include a detailed chapter breakdown and world-building wiki entries."
        examples.append({
            "system": "You are a story planner. Create a detailed outline for the story based on the user's request. Include premise, chapter structure, and wiki entries to create.",
            "turns": [{"user": user_var3, "assistant": plan_output}]
        })
    
    with open(OUTPUT_FILE, 'w', encoding='utf-8') as f:
        for ex in examples:
            f.write(json.dumps(ex, ensure_ascii=False) + '\n')
    
    print(f"Generated {len(examples)} plot_overview examples")

if __name__ == "__main__":
    main()
