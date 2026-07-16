#!/usr/bin/env python3
"""Augment project_planning dataset by generating variations of existing workflows."""
import json
from pathlib import Path

SOURCE_DIR = Path("../rwkv-harness/src/agents/storyteller/examples")
OUTPUT_FILE = Path("datasets/tasks/project_planning.jsonl")

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
        
        # Wrap plan in project planning format
        project_plan = f"# Story Plan\n{plan_output}"
        
        # Original example
        examples.append({
            "system": "You are a creative project manager. Plan the full story workflow: outline the story, then create wiki entries for characters, locations, and factions.",
            "turns": [{"user": user_input, "assistant": project_plan}]
        })
        
        # Variation 1: Different workspace name
        user_var1 = user_input.replace("workspace/shadow-realm", "workspace/alternate-shadow")
        user_var1 = user_var1.replace("workspace/starfall", "workspace/alternate-starfall")
        user_var1 = user_var1.replace("workspace/dragon-realm", "workspace/alternate-dragon")
        
        examples.append({
            "system": "You are a creative project manager. Plan the full story workflow: outline the story, then create wiki entries for characters, locations, and factions.",
            "turns": [{"user": user_var1, "assistant": project_plan}]
        })
        
        # Variation 2: More detailed request
        user_var2 = f"{user_input}\n\nPlease include a detailed workflow with outline and wiki entries."
        examples.append({
            "system": "You are a creative project manager. Plan the full story workflow: outline the story, then create wiki entries for characters, locations, and factions.",
            "turns": [{"user": user_var2, "assistant": project_plan}]
        })
        
        # Variation 3: Different phrasing
        user_var3 = f"Plan a complete story project: {user_input}"
        examples.append({
            "system": "You are a creative project manager. Plan the full story workflow: outline the story, then create wiki entries for characters, locations, and factions.",
            "turns": [{"user": user_var3, "assistant": project_plan}]
        })
    
    with open(OUTPUT_FILE, 'w', encoding='utf-8') as f:
        for ex in examples:
            f.write(json.dumps(ex, ensure_ascii=False) + '\n')
    
    print(f"Generated {len(examples)} project_planning examples")

if __name__ == "__main__":
    main()
