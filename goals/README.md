# RoCo AI Goals

See [`index.md`](index.md) for the full roadmap.

## Core Principle: Grammar-First

**Every model call must go through a BNF grammar.** Undertrained RWKV models (1B–2.9B) produce systematic contamination (`<thinking>` tags, meta-commentary) that free-form prompting cannot prevent. Grammar-constrained decoding rejects non-conforming tokens at every sampling step, making contamination impossible. Every stage needs its own domain grammar — plan and tool grammars are necessary but not sufficient. See `goals/infer/thinking.md` and `goals/mechanistic-agent/task_grammars.md` for details.
