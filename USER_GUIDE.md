# User Guide — RoCo AI

> For writers, not developers. You don't need to know Rust to write a story.

## What This Is

RoCo AI is a collaborative story-writing tool. You set the direction, and the AI writes the chapters. You can edit, give feedback, skip, or stop at any time. The AI is a tool; you are the author.

## Three Ways to Use It

### 1. CLI — The Simplest Way (Recommended for Beginners)

Open a terminal and run:

```bash
./start.sh
```

Or with your own idea:

```bash
./start.sh "Write a dark fantasy about a fallen knight"
```

The tool will ask you simple questions (tone, style, themes), show you an outline, and write chapters one at a time. At each chapter you can:

- Press **Enter** to continue
- Type **`f`** and then your feedback (e.g., "add more dialogue")
- Type **`r`** to check quality
- Type **`s`** to skip to next chapter
- Type **`q`** to stop and publish

Your finished story is saved in `.roco/workspaces/` as:
- `01-OUTLINE.md` — your outline
- `03-CHAPTER_*.md` — individual chapters
- `06-STORY.md` — the complete story

### 2. Web Editor — For Visual Editing (`apps/editor/`)

If you prefer a browser interface:

```bash
# In one terminal, start the server
RWKV_MODEL=... cargo run --release -p roco-server

# In another terminal, start the editor
cd apps/editor
npm install
npm run dev
```

Open `http://localhost:5173`. You can edit the outline and chapters in a rich text editor.

### 3. Desktop GUI — The Future Way (`crates/ui/`)

A desktop app (built with `egui`) is in development. It will offer:
- A markdown editor with inline AI suggestions
- File tree for navigating your workspace
- Chat panel for giving feedback
- Pacing controls (plan first, roll continuously, or auto-accept)

To explore the current desktop code:

```bash
run_desktop.sh
# or
open crates/ui/src/lib.rs
```

## Getting Started in 5 Minutes

1. Clone or open this folder.
2. Make sure you have a `.st` model file in `models/` (see `INSTALL.md`).
3. Run `./start.sh`.
4. Answer the setup questions (or press Enter to skip).
5. See your outline.
6. Type `done` to accept the outline and start writing.
7. Watch the first chapter appear.
8. Give feedback or continue.
9. When done, find your story at `.roco/workspaces/story_*/06-STORY.md`.

## Giving Good Feedback

The AI understands plain English. Be specific:

- ✅ "Make the knight hesitate before drawing his sword."
- ✅ "Add more dialogue between the characters."
- ✅ "The pacing is too fast; slow down the fight scene."
- ❌ "Make it better." (Too vague — the AI doesn't know what "better" means to you.)

## Resuming a Story

If you stopped earlier, resume with:

```bash
./start.sh --resume .roco/workspaces/story_1234567890
```

You can also load an existing text file:

```bash
./start.sh --from-file my_existing_story.md
```

## Common Questions

**Q: Do I need a GPU?**  
A: No, but it's faster. The tool falls back to CPU (`RWKV_ADAPTER=llvmpipe`).

**Q: Where is my model file?**  
A: It should be in `models/`. See `INSTALL.md` for download links. The CLI looks for any `.st` file there automatically.

**Q: My story is weird / off-track.**  
A: Edit the outline (`add`, `remove`, `move`) before writing chapters. The outline guides the AI.

**Q: I want to write with a friend.**  
A: Use `story_collaborative` (`RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli`) for a back-and-forth conversation mode.

**Q: How do I export my story?**  
A: The completed story is `06-STORY.md` in your workspace folder. Copy it anywhere. You can also read individual chapters (`03-CHAPTER_*.md`).
