#!/usr/bin/env python3
"""Generate a dataset_generation task dataset for self-bootstrapping.

This creates training examples where the model learns to generate more
examples for a given task. The meta-task: "given a task description and
existing examples, generate new synthetic examples."
"""
import json
from pathlib import Path

SOURCE_DIR = Path("datasets/tasks")
OUTPUT_FILE = Path("datasets/tasks/dataset_generation.jsonl")

def load_dataset(path: Path) -> list:
    """Load JSONL dataset."""
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]

def main():
    examples = []
    
    # For each task dataset, create "generate more examples" training pairs
    for dataset_file in SOURCE_DIR.glob("*.jsonl"):
        if dataset_file.name == "dataset_generation.jsonl":
            continue  # Skip self-reference
        
        task_name = dataset_file.stem
        dataset = load_dataset(dataset_file)
        
        if not dataset:
            continue
        
        # Extract system prompt and sample examples
        system_prompt = dataset[0]["system"]
        sample_examples = dataset[:3]  # Use first 3 as few-shot
        
        # Create the generation prompt
        user_input = f"""Task: Generate 3 new training examples for the following task.

Task description: {system_prompt}

Here are existing examples:
"""
        for i, ex in enumerate(sample_examples, 1):
            user_input += f"\nExample {i}:\n"
            user_input += f"Input: {ex['turns'][0]['user'][:200]}...\n"
            user_input += f"Output: {ex['turns'][0]['assistant'][:200]}...\n"
        
        user_input += "\nGenerate 3 new examples in the same format:\n"
        
        # Generate synthetic outputs (for now, use variations of existing)
        # In practice, this would be filled by running the model
        assistant_output = ""
        for i in range(3):
            # Create variation by modifying existing examples
            base_example = sample_examples[i % len(sample_examples)]
            assistant_output += f"\nExample {i+1}:\n"
            assistant_output += f"Input: {base_example['turns'][0]['user'][:200]} (variation {i+1})...\n"
            assistant_output += f"Output: {base_example['turns'][0]['assistant'][:200]} (synthetic)...\n"
        
        examples.append({
            "system": "You are a dataset generator. Create new training examples for the given task.",
            "turns": [{
                "user": user_input,
                "assistant": assistant_output
            }]
        })
    
    # Write output
    OUTPUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_FILE, 'w') as f:
        for ex in examples:
            f.write(json.dumps(ex) + '\n')
    
    print(f"Generated {len(examples)} dataset_generation examples")

if __name__ == "__main__":
    main()
