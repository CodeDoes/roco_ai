# Why I Need BNF

## Problem: Undertrained Models Contaminate Output

Undertrained RWKV models (1B–2.9B g1h) **systematically leak non-content tokens** into every generation — `<think>` planning text, meta-commentary like "Okay let me write...", and structural artifacts that no prompt engineering can suppress.

Live story pipeline runs (outline → wiki → chapters ×3 → validate → synopsis) proved:

| Attempt | Result |
|---------|--------|
| System prompt "output JSON only" | Contaminated within 3 tokens |
| Temperature sweep (0.1 → 0.9) | All produce contamination |
| Pre-fill `<thinking>plan</thinking>` | Works partially but is fragile |
| Post-processing regex strip | Broken for unclosed tags |
| **BNF grammar constraint** | **Contamination cannot occur** |

## How BNF Fixes It

The sampler rejects every token that would violate the grammar — at every position, on every step. The model literally cannot emit `<think>` if the grammar doesn't allow it.

```text
# Without BNF: model generates freely, produces garbage
User: Output {"name": "Alice", "age": 30}
Assistant: Okay let me think... I need to output JSON...
           {"name": "Alice", "age": 30}

# With BNF: sampler rejects "Okay", "let", "me", "think"...
# Only tokens matching the grammar survive
Assistant: {"name": "Alice", "age": 30}
```

This isn't a preference — it's a hard requirement. The 2.9B RWKV model is too undertrained to generate structured output without token-level enforcement.

## What Counts as "BNF"

Three layers, from weakest to strongest:

1. **Prompt engineering** (not BNF) — examples in system prompt, post-processing cleanup. Bandaid, proven insufficient.
2. **Character-level grammar** (schoolmarm) — parses GBNF, constrains at character level. Works for small grammars but bnf_sampler fails on character classes/quantifiers, and the fallback architecture was error-prone.
3. **Token-level grammar (kbnf)** — parses full GBNF, constrains at token level using the model's actual vocabulary. Single correct engine, no fallback needed.

## Current Status

- **kbnf** is the chosen engine — crate-isolated (`roco-bnf-engine`) to avoid `string-interner` + `web-rwkv` type overflow
- `BnfEngine` wraps kbnf behind a minimal `BnfMask` trait (`roco-engine`), so kbnf types never enter the inference compilation unit
- App layer creates `Box<dyn BnfMask>` from grammar + vocab bytes, passes through `CompletionRequest`
- Ready to be wired into CLI examples and the story pipeline

## When to Add Grammars

Every pipeline stage that calls the model needs its own BNF grammar:

| Stage | Grammar needed | Status |
|-------|---------------|--------|
| Outline generation | `outline.bnf` | 🔜 Schema→GBNF (kbnf) |
| Wiki/worldbuilding | `wiki.bnf` | 🔜 Schema→GBNF (kbnf) |
| Chapter prose | `chapter_prose.bnf` | 🔜 Schema→GBNF (kbnf) |
| Validation | `validation_report.bnf` | 🔜 Schema→GBNF (kbnf) |
| Synopsis | `synopsis.bnf` | 🔜 Schema→GBNF (kbnf) |
| Tool calls | `tool_call.bnf` | ✅ Built into message layer |
| Planning | `plan.bnf` | ✅ Planner module |

Signal that a grammar is missing: any use of `strip_think_blocks()`, pre-fill workarounds, or regex cleanup in the pipeline.

## Key Insight

The grammar doesn't need to be complex — it just needs to exist. A 10-rule grammar that matches `{"key": "value"}` is infinitely better than unrestricted generation. Schema→GBNF produces verbose but correct grammars; optimization for size is premature.
