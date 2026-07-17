# Story Templates

Pre-made templates to help you get started.

## Available Templates

### 1. Short Story
A simple 3-chapter story with setup, conflict, and resolution.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template short-story
```

### 2. Hero's Journey
The classic hero's journey structure with 12 stages.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template heros-journey
```

### 3. Mystery
A mystery story with clues, suspects, and revelation.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template mystery
```

### 4. Romance
A romance story with meet-cute, conflict, and resolution.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template romance
```

### 5. Horror
A horror story with buildup, scares, and climax.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template horror
```

### 6. Sci-Fi
A science fiction story with world-building and conflict.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template scifi
```

### 7. Fantasy
A fantasy story with magic, quests, and adventure.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template fantasy
```

### 8. Xianxia
A Chinese fantasy story with cultivation and martial arts.

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template xianxia
```

## Template Structure

Each template includes:
- **Outline** - Pre-defined chapter structure
- **Direction** - Tone, style, themes
- **Characters** - Suggested characters
- **Setting** - Suggested setting

## Custom Templates

You can create your own templates by creating a JSON file:

```json
{
  "name": "my-template",
  "description": "My custom template",
  "outline": [
    {
      "title": "The Beginning",
      "summary": "Introduction of the protagonist"
    },
    {
      "title": "The Journey",
      "summary": "The protagonist sets out on a quest"
    },
    {
      "title": "The End",
      "summary": "Resolution of the conflict"
    }
  ],
  "direction": {
    "tone": "dark",
    "style": "literary",
    "themes": ["redemption", "loss"],
    "pacing": "slow"
  }
}
```

Save it to `templates/my-template.json` and use it:

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template my-template
```

## Examples

### Example 1: Short Story
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template short-story \
  "Write a short story about a lighthouse keeper who discovers a message in a bottle"
```

### Example 2: Hero's Journey
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template heros-journey \
  "Write a hero's journey about a young wizard who must save the kingdom"
```

### Example 3: Mystery
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template mystery \
  "Write a mystery about a detective solving a murder in a small town"
```

### Example 4: Romance
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template romance \
  "Write a romance about two people who meet at a coffee shop"
```

### Example 5: Horror
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template horror \
  "Write a horror about a family moving into a haunted house"
```

### Example 6: Sci-Fi
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template scifi \
  "Write a sci-fi about a colony ship discovering a new planet"
```

### Example 7: Fantasy
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template fantasy \
  "Write a fantasy about a thief who steals a magical artifact"
```

### Example 8: Xianxia
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  --template xianxia \
  "Write a xianxia about a lone cultivator who levels up alone"
```
