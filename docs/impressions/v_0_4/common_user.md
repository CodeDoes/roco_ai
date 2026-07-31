# Common User Impression Report: RoCo AI v0.4

This document captures the simulated user experience of a non-technical writer trying RoCo AI for the first time via the CLI with a basic one-shot call:

```bash
roco -p "Write a sci-fi story about a lost signal"
```

---

## 1. Initial Setup and First Contact

### The Start-Up Latency Gap
- **Action**: User types the command and hits Enter.
- **Experience**: The screen hangs without any visual feedback for roughly 20-30 seconds.
- **Under the Hood**: The system is dynamically starting the inference daemon (`roco-inferd`), initializing Vulkan shaders, and loading the 2.9B model into GPU VRAM.
- **Friction**: A common user is highly likely to think the app crashed, prompting them to cancel the operation with `Ctrl+C`.
- **Recommendation**: Show a clear loading progress indicator with a message such as `[RoCo] Spawning local inference server & loading 2.9B model to GPU VRAM (may take up to 30s)...`.

---

## 2. Pacing Controls and Flow

### Careful Mode Interaction
- **Experience**: Once loaded, RoCo begins streaming the story beautifully, but then pauses and prints:
  ```text
    [a]ccept  [s]kip  [q]uit
  ```
  And waits for input.
- **Friction**: The user wanted to read a completed story but is suddenly forced to participate in a "human-in-the-loop" pacing check. This is fantastic for collaborative writing but highly unexpected for a simple prompt/one-shot generation.
- **Recommendation**: Set the default pacing mode for direct CLI prompt commands (`roco -p "..."`) to `auto-accept` or `planning` (which run to completion without pausing), while retaining `careful` as the default for the interactive `:repl` session.

---

## 3. Formatting and Readability

### Beautiful Markdown Outputs
- **Experience**: The final story output features perfectly formatted Markdown headers, clean bullet points, and appropriate double-newlines separating paragraphs.
- **Wins**:
  - Zero raw JSON bracket leaks.
  - Zero reasoning/thinking blocks contaminating the final prose.
  - Front matter matches standard Hugo/Jekyll metadata.

---

## 4. Stability and Robustness

### Format Failures
- **Experience**: If the local GPU drops a packet or experiences throttling, the JSON parsing fails, and the pipeline halts with a loud raw traceback.
- **Friction**: This is highly intimidating to non-developers.
- **Recommendation**: Wrap all JSON and structural parsers in the application with robust fallback handling. If parsing fails, explain the situation gently to the user and attempt an automated self-healing retry without exiting the CLI process.
