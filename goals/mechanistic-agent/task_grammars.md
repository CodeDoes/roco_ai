# Task Grammars

## Intent
BNF grammars that constrain model output per task domain (plan, chapter, wiki, synopsis, etc.).
Derive uses the plan grammar; per-type handlers use their own domain grammar. 
Output that doesn't parse is caught by the repair loop.

## Why This Matters: Learnings from Live Generation
### The contamination problem proved grammars are non-negotiable
During multi-step story pipeline runs on a 2.9B RWKV model, we observed:
- **System prompts alone cannot prevent** `thinking>` tag leakage or meta-commentary
- **Temperature adjustment has minimal effect** — the model leaks planning text at any temperature
- **Free-form prompting produces structurally unreliable output** — outline/wiki/validation/synopsis all contaminated
- Post-processing stripping is fragile and incomplete when models don't close their think tags

**Grammar-constrained decoding is the only reliable solution.** When output must satisfy a BNF grammar:
- The sampler rejects non-conforming tokens at every step
- No meta-commentary can slip through — it's not part of the grammar
- No post-processing needed — output is structurally guaranteed
- `serde_json::from_str()` always succeeds on constrained output
- Error recovery reduces to timeout/retry logic only

### Grammar design principles
- Each stage handler should have its OWN domain grammar (outline grammar ≠ wiki grammar ≠ chapter grammar)
- Grammars should be small but precise — enough structure to prevent ambiguity, not so much they block valid content
- JSON-compatible: derive plans → JSON parser, chapters → prose templates, wiki → structured fields
- Use `json_schema_to_gbnf` (`crates/grammar/src/json_schema.rs`) to auto-generate from schema objects

## Sub-goals
- **Plan grammar**: `<task-list>` with typed tasks — DONE in message GBNF, needs integration into MechanisticAgent
- **Per-domain grammars**: chapter prose, wiki entry, synopsis, validation report, event log
- **Grammar per mode**: each mode declares its task set with corresponding grammars
- **Auto-generation**: `json_schema_to_gbnf()` converts Rust schema types to GBNF at compile time

## Integration Status
- ✅ BNF infrastructure: `BnfConstraint` (`crates/grammar/src/bnf.rs`) + `bnf_sampler` v0.3.8
- ✅ GBNF→BNF converter wraps nonterminal names in angle brackets
- ✅ JSON-Schema → GBNF converter (`crates/grammar/src/json_schema.rs`) handles objects, arrays, enums
- ✅ Story-domain per-handler grammars (`GBNF/*.bnf`: outline, wiki, chapter_prose, validation_report, synopsis) — embedded + validated via `roco_grammar::grammar_library` against `roco-bnf-engine`
- ✅ Think-tag **state-tuning** primitives in `crates/engine/src/backend.rs`: `NO_THINK_PREFILL` (`<think></think>`) and `bake_no_think_session`. Validated by `crates/cli/examples/prompt_probe_eval.rs` (probe of the training-prompt prefixes as `prefill` after `Assistant:`).
- ⬜ Story pipeline stages wired to BNF constraints + no-think prefill (current workaround: pre-fill think blocks)
- ⬜ Per-handler grammars in mechanistic agent — this is the next critical gap

## Think-tag state-tuning (experiment, 2026-07-18)
Probed `System:`, `User:`, `Assistant:`, `Assistant: <think`, `Assistant: <think></think>`,
`Assistant: <reason>…</reason>`, etc. by feeding them as `prefill` and observing the
continuation. Findings:
- A **bare `Assistant:` start defaults to an open `<think>` block** — the contamination source.
- `Assistant: <think></think>` → content, no re-open. **Reliable suppression.**
- `Assistant: <reason>…</reason>` → a `<plan>` outline instead of `<think>` — alternate
  planning markers are the "certain areas" where thinking is acceptable.
- A system prompt "never use `<think>` tags" **backfires** (primes `<think>`). Don't use it.
- A baked no-think session is a *soft* bias (noisier than the prefill; occasional `User:`
turn leakage). Prefer the closed-think **prefill** for deterministic suppression.
- **Format lock-in** (`prompt_format_probe_eval.rs`): only the native
  `System:/User:/Assistant:` format is followed; ChatML/Alpaca/`Human:` are
  out-of-distribution and *trigger* `<think>`. `NO_THINK_PREFILL` still
  suppresses `<think>` across **all** formats (token-level, format-independent).
- **System instructions are inert** for think suppression: none / neutral /
  "no think" / "think step by step" / contradictory all emitted `<think>`.
- **Agentic induction works only with the no-think prefill**: the agentic
  system prompt + `NO_THINK_PREFILL` emitted `<action>plan_story_outline</action>`;
  without the prefill the model only thought and never emitted the action.
- **Line-prefix newline masking fails via prefill** (`▸ `/`> ` dropped after the
  first token). Force per-line structure with a **grammar** that mandates a
  line-prefix nonterminal, not a prefill.
- **Min-decay state-vector monitoring works**: `RwkvBackend::save_state()`
  serializes the recurrent vector; the per-head min-decay channels (last two of
  `head_size+2`) yield a norm (~145–157) and entropy (~0.6–0.8 bits) that varies
  by prompt — a cheap monitorable signal.

**Design:** suppress `<think>` by prefilling `NO_THINK_PREFILL` at every assistant-turn
start, and generate free prose **outside** the JSON envelope via the `GBNF/*.bnf`
grammars (which structurally exclude `<`/`>`, so `<think>` cannot appear); for
the JSON-envelope stages that must permit `<`, strip a leading `<think>…</think>`
span before parsing. For reasoning stages, intentionally prefill `<think>` to
capture the trace, then strip it before parsing JSON. This replaces the
grammar-level `<`/`>` ban and the pre-fill + strip-think workaround.

## Reference
- `mindful/spec/agent.md` — plan grammar BNF definition with `<task>`, `<type>`, `<domain>`, `<spec>` productions
- `ksr/spec.md` — typed tasks for plan/chapter/wiki
- `crates/grammar/src/bnf.rs` — `BnfConstraint` implementation with vocab trie
