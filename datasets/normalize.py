#!/usr/bin/env python3
"""Normalize roleplay datasets into bake_persona format.

Output: JSONL with {system: str, turns: [{user: str, assistant: str}, ...]}
"""
import json
import re
import sys
from pathlib import Path

SNIFF_DIR = Path("datasets/sniff")
OUT_DIR = Path("datasets/normalized")
OUT_DIR.mkdir(exist_ok=True)


def normalize_roleplay(src: Path, dst: Path):
    """Parse <|system|>...<|user|>...<|assistant|>...</s> format."""
    pattern = re.compile(
        r"<\|system\|>(.*?)</s>\s*"
        r"(?:<\|user\|>(.*?)</s>\s*<\|assistant\|>(.*?)</s>\s*)+",
        re.DOTALL,
    )
    with open(src, encoding="utf-8") as fin, open(dst, "w", encoding="utf-8") as fout:
        for line in fin:
            row = json.loads(line)
            text = row["text"]
            matches = pattern.findall(text)
            if not matches:
                continue
            # matches is [(system, user, assistant), ...] but regex only captures first group
            # Re-parse manually
            parts = re.split(r"<\|(system|user|assistant)\|>", text)
            system = ""
            turns = []
            current_user = ""
            current_asst = ""
            for i, part in enumerate(parts):
                if part == "system":
                    system = parts[i + 1].split("</s>")[0].strip()
                elif part == "user":
                    current_user = parts[i + 1].split("</s>")[0].strip()
                elif part == "assistant":
                    current_asst = parts[i + 1].split("</s>")[0].strip()
                    if current_user and current_asst:
                        turns.append({"user": current_user, "assistant": current_asst})
            if turns:
                fout.write(json.dumps({"system": system, "turns": turns}, ensure_ascii=False) + "\n")


def normalize_pippa(src: Path, dst: Path):
    """Parse <|system|>...<|user|>...<|model|>... format."""
    with open(src, encoding="utf-8") as fin, open(dst, "w", encoding="utf-8") as fout:
        for line in fin:
            row = json.loads(line)
            prompt = row["prompt"]
            generation = row.get("generation", "")
            parts = re.split(r"<\|(system|user|model)\|>", prompt)
            system = ""
            turns = []
            current_user = ""
            for i, part in enumerate(parts):
                if part == "system":
                    # System text goes until next <|user|> or end
                    sys_text = parts[i + 1]
                    if "<|user|>" in sys_text:
                        sys_text = sys_text.split("<|user|>")[0]
                    system = sys_text.strip()
                elif part == "user":
                    user_text = parts[i + 1]
                    if "<|model|>" in user_text:
                        user_text = user_text.split("<|model|>")[0]
                    current_user = user_text.strip()
                elif part == "model":
                    model_text = parts[i + 1]
                    if "<|user|>" in model_text:
                        model_text = model_text.split("<|user|>")[0]
                    if current_user:
                        turns.append({"user": current_user, "assistant": model_text.strip()})
            # Append the generation as the final assistant response
            if current_user and generation:
                turns.append({"user": current_user, "assistant": generation.strip()})
            if turns:
                fout.write(json.dumps({"system": system, "turns": turns}, ensure_ascii=False) + "\n")


def main():
    datasets = [
        ("roleplay.jsonl", "roleplay_normalized.jsonl", normalize_roleplay),
        ("pippa.jsonl", "pippa_normalized.jsonl", normalize_pippa),
    ]
    for src_name, dst_name, normalizer in datasets:
        src = SNIFF_DIR / src_name
        dst = OUT_DIR / dst_name
        if not src.exists():
            print(f"Skipping {src_name} (not found)")
            continue
        print(f"Normalizing {src_name} -> {dst_name}")
        normalizer(src, dst)
        # Count output rows
        with open(dst) as f:
            count = sum(1 for _ in f)
        print(f"  {count} conversations written")


if __name__ == "__main__":
    main()
