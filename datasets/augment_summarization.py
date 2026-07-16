#!/usr/bin/env python3
"""Augment summarization dataset with chapter-level and pair-level summaries."""
import json
from pathlib import Path

SOURCE_DIR = Path("../rwkv-harness/src/agents/storyteller/examples")
OUTPUT_FILE = Path("datasets/tasks/summarization.jsonl")

def load_md(path: Path) -> str:
    """Load markdown file, strip frontmatter."""
    if not path.exists():
        return ""
    content = path.read_text(encoding="utf-8")
    # Strip frontmatter
    if content.startswith("---"):
        end = content.find("---", 3)
        if end != -1:
            content = content[end+3:].strip()
    return content

def generate_summary(text: str) -> str:
    """Generate a one-sentence summary from text."""
    # Extract key elements: characters, actions, outcomes
    lines = text.split('\n')
    # Find the first substantial paragraph
    for line in lines:
        line = line.strip()
        if len(line) > 50 and not line.startswith('#'):
            # Extract first sentence or key action
            sentences = line.split('. ')
            if sentences:
                return sentences[0].rstrip('.') + '.'
    return "A story unfolds."

def main():
    OUTPUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    examples = []
    
    for story_dir in sorted(SOURCE_DIR.iterdir()):
        if not story_dir.is_dir():
            continue
        
        # Load all chapters
        chapters = []
        for i in range(1, 4):
            chapter_file = story_dir / f"chapter-00{i}.md"
            if chapter_file.exists():
                chapters.append(load_md(chapter_file))
        
        if not chapters:
            continue
        
        # Generate summaries at different granularities
        # Single chapters
        for i, chapter in enumerate(chapters, 1):
            summary = generate_summary(chapter)
            examples.append({
                "system": "You are a summarizer. Create a concise summary of the story chapter.",
                "turns": [{
                    "user": chapter,
                    "assistant": summary
                }]
            })
        
        # Chapter pairs
        for i in range(len(chapters) - 1):
            combined = chapters[i] + "\n\n" + chapters[i+1]
            summary = generate_summary(combined)
            examples.append({
                "system": "You are a summarizer. Create a concise summary of the story chapters.",
                "turns": [{
                    "user": combined,
                    "assistant": summary
                }]
            })
        
        # All chapters
        all_text = "\n\n".join(chapters)
        summary = generate_summary(all_text)
        examples.append({
            "system": "You are a summarizer. Create a concise summary of the story.",
            "turns": [{
                "user": all_text,
                "assistant": summary
            }]
        })
    
    # Write output
    with open(OUTPUT_FILE, 'w', encoding='utf-8') as f:
        for example in examples:
            f.write(json.dumps(example, ensure_ascii=False) + '\n')
    
    print(f"Generated {len(examples)} summarization examples")

if __name__ == "__main__":
    main()
