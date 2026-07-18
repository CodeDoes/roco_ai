# Imagined Usecases, Agent Monitoring, and an AI-Friendly Interface

Notes from the 2026-07-18 prompt/state-tune experiments and a thought
exercise: *if I were a human supervising this agent, how would I monitor
it without babysitting it?* See `crates/cli/examples/prompt_format_probe_eval.rs`
and `prompt_probe_eval.rs` for the raw data.

## 1. Imagined usecases

- **Collaborative novel**: human sets a premise + tone; the agent drafts an
  outline, then chapters, pausing at human-set checkpoints (chapter steering).
  The human edits inline; the agent revises with a diff. The final artifact is
  a markdown/PDF/epub export.
- **World bible**: as chapters are written, the agent maintains a wiki of
  characters/locations/lore, extracted from prose, never contradicting
  established facts.
- **Plot-state continuity**: a structured `PlotState` (characters, conflicts,
  foreshadowing, arc stage) is tracked per chapter so a 30-chapter story stays
  coherent without re-reading everything.
- **Quality judge**: the agent scores its own chapters on 7 dimensions and
  self-revises below threshold — but only on a *baked no-think session* so the
  judge doesn't leak `<think>` into the critique.
- **Agentic task runner**: given "produce a 3-chapter outline", the agent emits
  `<action>plan_story_outline</action>` and the router dispatches. Crucially,
  this only works when the think channel is **closed first** (see §2).

## 2. Monitoring the agent (as a human, hands-off)

The agent must be **observable, not watchable**. Concretely:

- **Structured event log**: every model call, action, and quality score is
  already recorded by `ObservabilitySystem`. Surface it as JSON lines a script
  can tail, not a scrolling terminal.
- **Think-tag rate as a health signal**: `prompt_probe_eval` showed the model
  *defaults* to `<think>` on most starts. A rising think-tag rate in production
  means the no-think state-tune drifted — alert on it.
- **Min-decay state-vector telemetry**: `RwkvBackend::save_state()` now
  serializes the recurrent vector (incl. the per-head min-decay channels — the
  last two of `head_size+2`). The min-decay norm (~145–157) and 256-bin entropy
  (~0.6–0.8 bits) are cheap, computable signals. Track them per turn: a sudden
  entropy jump often coincides with a format/system change (the experiment saw
  `contradictory` system → 0.80 bits, highest). Use it as a "is the model
  confused / in a different regime?" gauge.
- **Checkpoint, don't stream-watch**: the agent should surface *decisions*
  ("I'm about to revise chapter 3 — accept / modify / skip") and wait for a
  non-blocking ack. The human reviews asynchronously.
- **Reversibility**: every action is versioned (`Reversibility`/`VersionControl`),
  so a bad turn is an `undo`, not a crisis.

## 3. AI-friendly CLI / API (machine-first)

The agent should be drivable by *another* program (or a human via script),
not only by an interactive REPL:

- **Structured I/O**: every command emits stable JSON (or a `--json` flag),
  so a wrapper can parse it without scraping ANSI.
- **Event stream**: a `--events` mode that emits one JSON object per event
  (`gen_start`, `gen_token`, `action`, `quality`, `checkpoint`) over stdout or
  a socket.
- **Gateway as a background task**: `roco gateway` / `roco server` must be
  startable detached — `nohup`/systemd/`devenv` background, or a `--daemon`
  flag — so the agent runs without a tied-up terminal. The CLI then talks to
  it over a local HTTP/gRPC socket. This is the "should not require constant
  monitoring" requirement made concrete.
- **Resumable sessions**: a run is a named session on disk (`story_human
  --resume`); a supervising script polls status and injects feedback as JSON
  POSTs, never by reading a live TTY.

## 4. Open questions from the experiments

- **Line-prefix newline masking fails via prefill**: `prompt_format_probe_eval`
  showed a `▸ `/`> ` prefill is dropped after the first token (0/3, 0/2 lines
  kept it). To force per-line structure (e.g. for diff-friendly or
  token-efficient output) we need a **grammar** that mandates a line-prefix
  nonterminal, not a prefill. Tracked as a grammar task.
- **System instructions are inert for think suppression**: none / neutral /
  "no think" / "think step by step" / contradictory all emitted `<think>`.
  Don't rely on system prompts to control think; use the `NO_THINK_PREFILL`
  state-tune. This is a hard limit of the undertrained base model.
- **Format lock-in**: only the native `System/User/Assistant` format is
  followed; alt formats degrade and *trigger* think. The no-think prefill
  still suppresses think across formats (token-level, format-independent), so
  the state-tune transfers even if you wrap prompts in another format.
