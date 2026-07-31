# Common User Impressions — v0.4

*Documented after testing with a fresh user perspective.*

## First Impression

**Command:** `roco story "A cat who loves cheese"`

**Time:** ~4 minutes end-to-end

**Overall:** Impressive. The pipeline just works. A user can go from premise to published story with a single command.

## What Worked Well

### 1. Simple One-Liner Magic
```bash
roco story "A cat who loves cheese"
```
This is the core value proposition and it delivers. No setup, no configuration, no confusion. Just type a premise and get a story.

### 2. Clear Progress Feedback
The terminal output is excellent:
- `📝 Outline...` → `✓ Outline complete`
- `📚 Worldbuilding...` → `✓ World bible complete`
- `✍️ Chapter 1...` → `✓ Chapter 1 quality check passed`
- `⚠️ Chapter 2 needs revision (attempt 1/3)`

The emoji progress indicators are intuitive. Users can see exactly what's happening and how far along they are.

### 3. Automatic Retry Logic
When a chapter fails validation, the user sees:
```
⚠️ Chapter 2 needs revision (attempt 1/3) — retrying...
```
This is reassuring. The user doesn't need to understand why it failed — the system handles it.

### 4. Output Organization
The final output is clean:
```
✅ Done! 8 files in workspace:
  📄 01-OUTLINE.md (550 bytes)
  📄 02-WIKI.md (1336 bytes)
  📄 03-CHAPTER_1.md (1474 bytes)
  ...
  📄 06-STORY.md (6009 bytes)
```
Users immediately know where their files are.

### 5. Resume Capability
The story command auto-discovers existing workspaces and can resume. This is crucial for long-running workflows.

## What Could Be Better

### 1. No Inline Progress Bar
While the checkmarks are nice, there's no real-time progress bar during generation. Users might wonder if the app is stuck during long waits (15-45 second waits between chapters).

**Suggestion:** Add a simple spinner or progress indicator during generation.

### 2. Silent Baking Phase
The baking phase shows:
```
🔥 Baking writer session (format: JSON chapter prose)...
```
But then there's a 3-second pause with no output. Users might think it's frozen.

**Suggestion:** Add a subtle "processing..." indicator during bake.

### 3. Help Text Could Be Friendlier
```
roco --help
```
Shows all commands but in a technical way. A new user might not know what "interactive fiction" or "HTML canvas" means.

**Suggestion:** Add a "Quick Start" section to help text:
```
Quick Start:
  roco story "Your premise here"     → Generate a short story
  roco                                → Start chatting
  roco --help                         → See all commands
```

### 4. No First-Time Setup Guidance
A truly new user might not know:
- They need a GPU (or can use CPU mode)
- They need to set `RWKV_MODEL` or have a model file
- The daemon needs to be running

**Suggestion:** Add a `roco setup` or first-run check that guides new users.

### 5. Story Output Location Not Obvious
The output goes to `.roco/stories/` but users might not know to look there.

**Suggestion:** Print the full path more prominently:
```
📖 Story published to: /home/user/.roco/stories/whiskers_treasure.md
```

### 6. No Story Preview in Terminal
After publishing, the story is just files on disk. A user might want to see a preview without opening a file manager.

**Suggestion:** Offer to show the story after publishing:
```
✅ Story published!
📄 View: cat .roco/stories/whiskers_treasure.md
🚀 Run: roco story "..." again to make a new one
```

### 7. Error Messages Could Be More Helpful
When things fail (e.g., model not found), the error is technical:
```
error: could not execute process `sccache-wrap ...`
```
A user needs to know what to do next.

**Suggestion:** Add actionable error messages:
```
Error: Model not found.
Hint: Place a .st file in models/ or set $RWKV_MODEL=path/to/model.st
```

## Key User Questions Unanswered

| Question | Answer Location |
|----------|----------------|
| "How do I install?" | Not documented |
| "What model do I need?" | Not in help |
| "How much RAM/VRAM?" | Not in help |
| "Can I run without GPU?" | Not obvious |
| "Where do my stories go?" | Only in output |
| "How do I stop the daemon?" | `roco stop` (found via --help) |
| "How do I resume a story?" | `roco story --resume` (found via --help) |

## Comparison to Other Tools

| Feature | RoCo | NovelAI | Sudowrite |
|---------|------|---------|-----------|
| One-command story | ✅ | ❌ | ❌ |
| Local/offline | ✅ | ❌ | ❌ |
| No subscription | ✅ | ❌ | ❌ |
| Structured pipeline | ✅ | Partial | ✅ |
| Open source | ✅ | ❌ | ❌ |

**Bottom line:** RoCo is unique in offering a complete local, offline, open-source story pipeline with a single command. This is a strong value proposition.

## Recommendations

1. **Add `roco quickstart`** — First-run guide that checks setup and explains basics
2. **Add progress indicators** — Spinners during long waits
3. **Improve error messages** — Actionable hints, not just technical errors
4. **Show story preview** — Offer to display the published story
5. **Add documentation link** — `roco --help` should mention `docs/README.md`
6. **Consider `roco story --preview`** — Show story without saving

## Conclusion

**User Score: 8/10**

A common user can go from zero to a published 3-chapter story in 4 minutes with a single command. That's impressive. The main gaps are around onboarding and feedback during long waits. Once users understand the workflow, they'll appreciate the power and simplicity.

The "magic" is real — and that's exactly what this app should feel like.
