# State Tune Examples Browser

State Tuning in RWKV freezes all model weights and optimizes the **initial hidden state
vectors ($S_0$)** directly. The examples below are co-located with the code that uses them.
Each entry describes the session, system prompt, few-shot examples, and expected behavior.

---

## Table of Contents

1. [Engine: bake_into_session — Persona Baking](#1-engine-bake_into_session--persona-baking)
2. [Engine: bake_no_think_session — Clean Assistant Start](#2-engine-bake_no_think_session--clean-assistant-start)
3. [Eval: bake_fim_session — Fill-in-the-Middle](#3-eval-bake_fim_session--fill-in-the-middle)
4. [CLI (LSP): bake_fim_session — IDE Completion](#4-cli-lsp-bake_fim_session--ide-completion)
5. [Validation: state_tuned_json — Structured Extraction](#5-validation-state_tuned_json--structured-extraction)
6. [Eval: state_pirate_persona_baked — Multi-Turn Persona](#6-eval-state_pirate_persona_baked--multi-turn-persona)
7. [Eval: state_tune_custom_persona — Few-Shot Persona](#7-eval-state_tune_custom_persona--few-shot-persona)
8. [token0_probe — EOS-Padded State Tuning Tests](#8-token0_probe--eos-padded-state-tuning-tests)
9. [Engine: bake_persona — Raw State Bytes](#9-engine-bake_persona--raw-state-bytes)
10. [Grammar: StateTunedStrategy — Prompt Engineering Strategy](#10-grammar-statetunedstrategy--prompt-engineering-strategy)

---

## 1. Engine: bake_into_session — Persona Baking

**File:** `crates/engine/src/backend.rs::bake_into_session()` (line 272)

**Purpose:** The original state-tune pattern. Feeds few-shot (user, assistant) pairs into
a named session so the recurrent state absorbs the persona. *Deprecated in favor of
`bake_no_think_session`* — because it feeds assistant text through `prompt` (user role),
the baked state expects another user turn next, causing spurious `User:` emissions.

**Session:** caller-specified (e.g. `"pirate_persona"`)

**System prompt:** caller-specified (e.g. `"You are a terse pirate. Answer in exactly one short pirate sentence."`)

**Examples format:** `&[(&str, &str)]` — array of `(user_msg, assistant_msg)` pairs

**EOS padding:** `feed_eos` between examples (token 0 separator)

**Usage sites:**
- `crates/engine/src/cases.rs:375` — eval case `state_pirate_persona_baked`

**Expected behavior:** After baking, a plain user turn continues the persona without
repeating the system prompt. But due to the role mistake, the model may emit `User:`
preambles.

---

## 2. Engine: bake_no_think_session — Clean Assistant Start

**File:** `crates/engine/src/backend.rs::bake_no_think_session()` (line 339)

**Purpose:** Correctly-roled state-tune. Feeds assistant text as a **prefill**
(the correct assistant role), so the baked state learns that assistant responses
begin with content, never `  thinking`.

**Session:** caller-specified

**System prompt:** caller-specified (only first example)

**Examples format:** `&[(&str, &str)]` — array of `(user_msg, assistant_msg)` pairs

**Key difference vs bake_into_session:**
```
bake_into_session:      assistant text → prompt field (USER role) ❌
bake_no_think_session:  assistant text → prefill field (ASSISTANT role) ✅
```

**EOS padding:** `feed_eos` between examples

**Usage sites:**
- `crates/ui/tests/token0_probe.rs:294` — test `test_bake_no_think_with_eos`

**Expected behavior:** After baking with EOS padding, generation produces content
directly without `  thinking` blocks or `User:` preambles. This is the recommended
pattern for all new state tunes.

**Example test bake:**
```rust
// From token0_probe.rs
let mut examples: Vec<(&str, &str)> = Vec::new();
examples.push(("What is 2+2?", "The answer is 4."));
examples.push(("What is 3+5?", "The sum is 8."));
bake_no_think_session(&backend, "test_session", "You are a math tutor.", &examples).await;
```

---

## 3. Eval: bake_fim_session — Fill-in-the-Middle

**File:** `crates/engine/src/eval.rs::bake_fim_session()` (line 524)

**Purpose:** Bakes FIM few-shot bridge examples into a named session so the model
learns the BEFORE/AFTER → INSERT pattern for prose fill-in-the-middle.

**Session:** `FIM_SESSION` (`"fim_session"`)

**System prompt:**
```
You are RoCo, a collaborative story-writing assistant. Given the text BEFORE
the cursor and the text AFTER the cursor, write ONLY the short passage that
connects them. Never repeat the BEFORE or AFTER text, never use <fim> or
think tags, never add commentary.
```

**Few-shot examples (3 pairs):**

| User (BEFORE / AFTER) | Assistant (INSERT) |
|---|---|
| `BEFORE: He raised the blade, bracing for the clash.\nAFTER: the ground shook beneath their feet.\nINSERT:` | `He brought the sword down with all his might, and` |
| `BEFORE: She whispered a spell under her breath.\nAFTER: the ward flared to life around them.\nINSERT:` | `A shimmer of light curled from her fingertips as` |
| `BEFORE: The ship crested the wave.\nAFTER: the shore came into view.\nINSERT:` | `Through the salt spray,` |

**EOS padding:** `feed_eos` between examples

**Usage sites:**
- `crates/engine/src/cases.rs:528` — eval case `fim_basic_bridge` (session `fim_session`)
- `crates/engine/src/cases.rs:546` — eval case `fim_no_tag_leakage` (session `fim_session`)

**Expected behavior:** After bake, the model produces bridging text between a
BEFORE clause and an AFTER clause, without repeating either, without FIM sentinel tags.

---

## 4. CLI (LSP): bake_fim_session — IDE Completion

**File:** `crates/cli/src/lsp.rs::bake_fim_session()` (line 43)

**Purpose:** LSP-specific FIM bake for IDE completions (Zed, VSCode). Uses a
slightly different format with metadata tokens.

**Session:** `FIM_SESSION` (`"roco_fim"`)

**System prompt:** Same as eval FIM but with workspace metadata appended (username, timestamp)

**Examples:** Same 3 bridge examples as eval FIM, plus **open file context** from the
workspace (each file contents fed as additional turns to absorb project context).

**Usage sites:**
- `crates/cli/src/lsp.rs:127` — `ensure_fim_session()` — lazy one-time bake

**Expected behavior:** Model can fill in prose between BEFORE/AFTER cursor positions
in the user's actual project files, using absorbed project context for style consistency.

---

## 5. Validation: state_tuned_json — Structured Extraction

**File:** `crates/validation/src/agent.rs::state_tuned_json()` (line 1435)

**Purpose:** State-tuned JSON extraction without BNF grammar. Uses a prefill of
`{"\n` and trusts the state-tuned bias to produce valid JSON.

**Key parameters:**
- `grammar: None` — explicitly no grammar constraint (relies on state tuning)
- `prefill: Some("{\n")` — starts JSON structure
- Used with **high temperatures** (0.6–0.8) for creative tasks

**Usage sites (all in `crates/validation/src/agent.rs`):**

| Line | Task | Temperature | Max Tokens |
|---|---|---|---|
| 438 | Summary response | 0.3 | 200 |
| 702 | Edit response | 0.6 | 1024 |
| 760 | Revise response | 0.7 | 1024 |
| 895 | Style response | 0.7 | 2048 |
| 955 | POV response | 0.7 | 2048 |
| 1228 | Brainstorm response | 0.8 | 800 |
| 1289 | Expand response | 0.6 | 800 |

**Expected behavior:** Produces valid JSON (with `clean_json_output` post-processing)
for structured tasks like summarization, editing, style transfer. No grammar means
higher creative flexibility but risk of malformed output.

---

## 6. Eval: state_pirate_persona_baked — Multi-Turn Persona

**File:** `crates/engine/src/cases.rs:347` (line 375)

**Purpose:** Eval case that tests whether a persona baked via `bake_into_session`
persists across multiple turns without re-stating the system prompt.

**Session:** not explicitly named (eval framework handles internally)

**System prompt:**
```
You are a terse pirate. Answer in exactly one short pirate sentence.
```

**Expected hints:** `vec!["star"]` — model should mention stars for navigation

**Prompt at test time:**
```
What's the best way to navigate at night?
```

**Expected behavior:** After the persona is baked into the session's recurrent state,
the model should answer in pirate voice without the system prompt being re-fed.
*Note: uses deprecated `bake_into_session` pattern.*

---

## 7. Eval: state_tune_custom_persona — Few-Shot Persona

**File:** `crates/engine/src/cases.rs:` (line 387)

**Purpose:** Eval case for a persona baked from few-shot examples (not just system
prompt). Tests whether state tuning can absorb a custom interaction style from
example dialogue alone.

**Session:** not explicitly named

**System prompt:** `""` (empty — persona comes entirely from few-shot examples)

**Expected hints:** `vec!["weather"]`

**Prompt at test time:**
```
What do you think about the weather today?
```

**Expected behavior:** The model should adopt the persona's speaking style from the
baked few-shot examples without any system prompt describing the persona. This tests
pure state tuning without prompt engineering.

---

## 8. token0_probe — EOS-Padded State Tuning Tests

**File:** `crates/ui/tests/token0_probe.rs`

**Purpose:** Probe tests that verify the correctness of EOS-padded state tuning.
Demonstrates the recommended pattern for baking sessions.

**Test cases:**

| Test | What it verifies |
|---|---|
| `test_eos_padded_state_tuning_works` | EOS between examples produces clean state |
| `test_pure_state_tune_sufficient` | State tune alone (no prefill) works |
| `test_bake_no_think_with_eos` | bake_no_think_session + EOS padding end-to-end |

**Usage pattern demonstrated:**
```rust
// Step 1: Create session
let mut examples: Vec<(&str, &str)> = Vec::new();
examples.push(("User message 1", "Assistant response 1"));
examples.push(("User message 2", "Assistant response 2"));

// Step 2: Bake into session (feeds EOS between examples)
bake_no_think_session(&backend, "my_session", "System prompt", &examples).await?;

// Step 3: Generate using baked state
let resp = backend.complete(CompletionRequest {
    prompt: "New user message".into(),
    session: Some("my_session".into()),
    ..Default::default()
}).await?;
```

---

## Summary Table

| # | Pattern | Crate | Function | Role-Correct | Grammar | Prefill |
|---|---|---|---|---|---|---|
| 1 | Persona baking | engine | `bake_into_session` | ❌ (user role) | N/A | N/A |
| 2 | Clean assistant | engine | `bake_no_think_session` | ✅ (prefill role) | N/A | N/A |
| 3 | FIM bridge | engine/eval | `bake_fim_session` | ❌ (user role) | FillInMiddle BNF | `  response response` |
| 4 | IDE FIM | cli/lsp | `bake_fim_session` | ❌ (user role) | FillInMiddle BNF | `  response response` |
| 5 | Structured extract | validation | `state_tuned_json` | N/A | explicit None | `{\n` |
| 6 | Pirate persona | engine/eval | `bake_into_session` | ❌ (user role) | None | None |
| 7 | Custom persona | engine/eval | `bake_into_session` | ❌ (user role) | None | None |
| 8 | EOS padding tests | ui/tests | `bake_no_think_session` | ✅ (prefill role) | None | varies |

---

## Migration Path

Patterns 1, 3, 4, 6, 7 use the **deprecated `bake_into_session`** which feeds
assistant text through the `prompt` field (user role). This causes the baked state
to expect another user turn, producing `User:` preambles in generation.

The **recommended pattern** is `bake_no_think_session` (pattern 2) which feeds
assistant text as a **prefill** (assistant role). Combined with EOS padding between
examples (pattern 8), this produces clean generation without thinking blocks
or role confusion.

To migrate:
1. Replace `bake_into_session(backend, session, system, examples)` →
   `bake_no_think_session(backend, session, system, examples)`
2. No other changes needed — the function signature is identical
3. For grammar-constrained generation after bake, use the `FillInMiddle` BNF with
   the baked session

---

## 9. Engine: bake_persona — Raw State Bytes

**File:** `crates/engine/src/backend.rs::bake_persona()` (line 224)

**Purpose:** The original state-tune pattern that returns **raw state bytes**
instead of using named sessions. This is the precursor to `bake_into_session`.

**Key difference from session-based bakes:**
- Returns `Result<Vec<u8>, EngineError>` — caller saves/loads the byte blob
- Only meaningful for backends implementing `save_state()`/`load_state()`
- Uses `preserve_state: i > 0` (skips preserve on first turn)
- Also uses `max_tokens: 1024` (much higher than session bakes which use 1)

**Usage:**
```rust
let state_bytes = bake_persona(&backend, "You are a helpful assistant.", &examples).await?;
// Later, in a different process:
backend.load_state(&state_bytes).await?;
```

**Expected behavior:** Returns a serialized recurrent state blob that can be
loaded into any RWKV inference process with the same model.

---

## 10. Grammar: StateTunedStrategy — Prompt Engineering Strategy

**File:** `crates/grammar/src/strategies.rs::StateTunedStrategy` (line 348)

**Purpose:** A zero-grammar output strategy that relies entirely on prompt
engineering (few-shot examples in the system message) and post-processing
(strip markdown fences, trim whitespace, parse JSON).

**Why it exists:** Grammar-constrained strategies fail on small RWKV models
due to character-class incompatibility in `bnf_sampler` — the vocab contains
no standalone JSON-punctuation tokens (`"`, `{`, `}`, `:`). Unconstrained
generation with examples in the prompt produces perfect JSON in markdown
fences while all grammar strategies produce garbage.

**Strategy selector:** `StrategySelector::state_tuned()`

**Post-processing pipeline:**
1. Strip ` ```json ... ``` ` fences
2. Strip ` ``` ... ``` ` generic fences
3. Strip leading/trailing whitespace
4. Parse as JSON via `serde_json::from_str`

**Usage sites in CLI eval examples:**
| File | Task |
|---|---|
| `crates/cli/examples/wiki_eval.rs:38` | Wiki extraction |
| `crates/cli/examples/brainstorm_eval.rs:38` | Brainstorming |
| `crates/cli/examples/chapter_validate_eval.rs:45` | Chapter validation |
| `crates/cli/examples/synopsis_eval.rs:38` | Synopsis generation |
| `crates/cli/examples/outline_eval.rs:38` | Outline generation |

**Expected behavior:** Produces valid JSON from unconstrained model output
by relying on well-crafted few-shot examples in the prompt, then post-processing
the natural markdown fences the model wraps around JSON.
