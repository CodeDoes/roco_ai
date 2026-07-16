#!/usr/bin/env python3
"""Extract task-specific datasets from storyteller examples.

Source: ../rwkv-harness/src/agents/storyteller/examples/
Output: datasets/tasks/{task_name}.jsonl

Tasks:
- story_writing: (plan + wiki) -> chapter prose
- plot_overview: (user request) -> outline/plan
- wiki_generation: (plan + context) -> wiki entry
- project_planning: (user request) -> full workflow
- summarization: (chapters) -> plan summary
"""
import json
import os
import re
from pathlib import Path

# Source directory (relative to roco_ai root)
SOURCE_DIR = Path("../rwkv-harness/src/agents/storyteller/examples")
OUTPUT_DIR = Path("datasets/tasks")

def extract_frontmatter(content: str) -> tuple[str, str]:
    """Extract think block and content from markdown with frontmatter."""
    think_match = re.search(r"^---\nthink: \|\n(.*?)\n---\n", content, re.DOTALL | re.MULTILINE)
    think = think_match.group(1).strip() if think_match else ""
    body = content[think_match.end():].strip() if think_match else content.strip()
    return think, body

def load_md(path: Path) -> tuple[str, str]:
    """Load markdown file, return (think, body)."""
    if not path.exists():
        return "", ""
    content = path.read_text(encoding="utf-8")
    return extract_frontmatter(content)

def extract_story_writing(story_dir: Path) -> list[dict]:
    """Extract story writing examples: (plan + wiki) -> chapter."""
    examples = []
    plan_think, plan_body = load_md(story_dir / "_plan.md")
    
    # Load all wiki entries
    wiki_dir = story_dir / "wiki"
    wiki_context = ""
    if wiki_dir.exists():
        for wiki_file in sorted(wiki_dir.rglob("*.md")):
            _, wiki_body = load_md(wiki_file)
            wiki_context += f"\n\n## {wiki_file.stem}\n{wiki_body}"
    
    # Extract each chapter
    for chapter_file in sorted(story_dir.glob("chapter-*.md")):
        chapter_think, chapter_body = load_md(chapter_file)
        
        # Input: plan + wiki context + chapter number
        chapter_num = chapter_file.stem.split("-")[1]
        user_input = f"# Story Plan\n{plan_body}\n\n# World Wiki\n{wiki_context}\n\n# Task\nWrite Chapter {chapter_num}."
        
        examples.append({
            "system": "You are a fiction writer. Write the next chapter based on the story plan and world wiki. Maintain consistency with established characters, locations, and plot.",
            "turns": [{"user": user_input, "assistant": chapter_body}]
        })
    
    return examples

def extract_plot_overview(story_dir: Path) -> list[dict]:
    """Extract plot overview examples: (user request) -> plan."""
    examples = []
    user_think, user_body = load_md(story_dir / "_user.md")
    plan_think, plan_body = load_md(story_dir / "_plan.md")
    
    examples.append({
        "system": "You are a story planner. Create a detailed outline for the story based on the user's request. Include premise, chapter structure, and wiki entries to create.",
        "turns": [{"user": user_body, "assistant": plan_body}]
    })
    
    return examples

def extract_wiki_generation(story_dir: Path) -> list[dict]:
    """Extract wiki generation examples: (plan + context) -> wiki entry."""
    examples = []
    plan_think, plan_body = load_md(story_dir / "_plan.md")
    
    wiki_dir = story_dir / "wiki"
    if not wiki_dir.exists():
        return examples
    
    for wiki_file in sorted(wiki_dir.rglob("*.md")):
        wiki_think, wiki_body = load_md(wiki_file)
        entry_type = wiki_file.parent.name  # character, location, faction
        entry_name = wiki_file.stem
        
        # Input: plan + other wiki entries (excluding current)
        other_wiki = ""
        for other_file in sorted(wiki_dir.rglob("*.md")):
            if other_file != wiki_file:
                _, other_body = load_md(other_file)
                other_wiki += f"\n\n## {other_file.stem}\n{other_body}"
        
        user_input = f"# Story Plan\n{plan_body}\n\n# Existing Wiki\n{other_wiki}\n\n# Task\nCreate a {entry_type} wiki entry for {entry_name}."
        
        examples.append({
            "system": f"You are a world-builder. Create a detailed {entry_type} wiki entry that fits the story plan and is consistent with other wiki entries.",
            "turns": [{"user": user_input, "assistant": wiki_body}]
        })
    
    return examples

def extract_project_planning(story_dir: Path) -> list[dict]:
    """Extract project planning examples: (user request) -> full workflow."""
    examples = []
    user_think, user_body = load_md(story_dir / "_user.md")
    plan_think, plan_body = load_md(story_dir / "_plan.md")
    
    # Build full workflow output
    workflow = f"# Story Plan\n{plan_body}\n\n"
    
    wiki_dir = story_dir / "wiki"
    if wiki_dir.exists():
        workflow += "# Wiki Entries\n"
        for wiki_file in sorted(wiki_dir.rglob("*.md")):
            _, wiki_body = load_md(wiki_file)
            workflow += f"\n## {wiki_file.stem}\n{wiki_body}\n"
    
    examples.append({
        "system": "You are a creative project manager. Plan the full story workflow: outline the story, then create wiki entries for characters, locations, and factions.",
        "turns": [{"user": user_body, "assistant": workflow}]
    })
    
    return examples

def extract_summarization(story_dir: Path) -> list[dict]:
    """Extract summarization examples: (chapters) -> plan summary."""
    examples = []
    plan_think, plan_body = load_md(story_dir / "_plan.md")
    
    # Concatenate all chapters
    chapters = ""
    for chapter_file in sorted(story_dir.glob("chapter-*.md")):
        _, chapter_body = load_md(chapter_file)
        chapters += f"\n\n{chapter_body}"
    
    # Use plan premise as summary target
    # Extract just the premise from plan
    premise_match = re.search(r"## Premise\n(.*?)(?=\n##|\Z)", plan_body, re.DOTALL)
    if premise_match:
        summary = premise_match.group(1).strip()
        examples.append({
            "system": "You are a summarizer. Create a concise summary of the story.",
            "turns": [{"user": chapters.strip(), "assistant": summary}]
        })
    
    return examples

def main():
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    
    # Find all story directories
    story_dirs = sorted([d for d in SOURCE_DIR.iterdir() if d.is_dir()])
    print(f"Found {len(story_dirs)} story directories: {[d.name for d in story_dirs]}")
    
    # Extract each task
    tasks = {
        "story_writing": extract_story_writing,
        "plot_overview": extract_plot_overview,
        "wiki_generation": extract_wiki_generation,
        "project_planning": extract_project_planning,
        "summarization": extract_summarization,
    }
    
    for task_name, extractor in tasks.items():
        all_examples = []
        for story_dir in story_dirs:
            examples = extractor(story_dir)
            all_examples.extend(examples)
        
        output_file = OUTPUT_DIR / f"{task_name}.jsonl"
        with open(output_file, "w", encoding="utf-8") as f:
            for example in all_examples:
                f.write(json.dumps(example, ensure_ascii=False) + "\n")
        
        print(f"{task_name}: {len(all_examples)} examples -> {output_file}")

if __name__ == "__main__":
    main()
