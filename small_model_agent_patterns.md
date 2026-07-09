# Small-Model Agent Design Patterns

## Practical Heuristics for 3B-Class Subagent Orchestration

**Scope:** This document summarizes field-tested patterns for using small language models
(≤3B parameters, ~4K context windows) as subagents in complex task decomposition pipelines. It
focuses on actionable design heuristics rather than theoretical frameworks, drawing from
production multi-agent systems, edge-deployment research, and empirical code-generation studies.

---

## 1. Task Decomposition Patterns

### 1.1 The Orchestrator-Worker Pattern (with Small-Model Workers)

The dominant production pattern is **Orchestrator-Worker**: one capable orchestrator model
decomposes the task, delegates to specialist workers, and assembles results. For cost and latency
reasons, the workers are often small models while the orchestrator is larger.
*(6 Multi-Agent Orchestration Patterns for Production)*

**Critical adaptation for 3B workers:**

- **Scope clamping:** Each worker must receive a single atomic subtask with all necessary context
  included inline. Do not expect a 3B model to perform dynamic re-planning or re-decomposition
  mid-task.
- **Pre-bound context:** The orchestrator must resolve all ambiguities before dispatch. A 3B model
  cannot reliably ask clarifying questions; it will guess and often guess wrong.
- **Deterministic interfaces:** Worker inputs and outputs should follow strict schemas (JSON,
  structured markdown). Unstructured natural-language handoffs between small models accumulate
  formatting errors rapidly.

**Production failure mode:** Context window overflow at the orchestrator. At four or more workers,
the orchestrator frequently exceeds window limits because it accumulates context from every worker.
*(6 Multi-Agent Orchestration Patterns for Production)*

### 1.2 Chunking Heuristics for 4K Context Windows

Empirical studies of 3B-class code models (e.g., Qwen2.5-Coder 3B, StarCoder2 3B, CodeGemma 2B)
show that performance drops sharply when the relevant context exceeds ~60% of the window.
*(Assessing Small Language Models for Code Generation)*

| Heuristic | Rationale |
|-----------|-----------|
| ≤1,500 tokens for the task description | Leaves 2,500 tokens for generation and scratch reasoning |
| Single file / single function per chunk | 3B models struggle with cross-file reasoning unless explicitly fine-tuned for it |
| Include 2–3 lines of surrounding context | Barely enough for local variable/type inference; do not rely on broader project context |
| Explicit "output only" instruction | Prevents small models from emitting excessive reasoning tokens that consume the generation budget |

### 1.3 Map-Reduce and Fan-Out/Fan-In Variants

For tasks that can be parallelized (e.g., processing N documents, reviewing N files):

- **Map phase:** Fan out identical prompts to N workers, each with one chunk. Keep the prompt
  template identical across workers to maximize cache hits and reduce prompt-token overhead.
- **Reduce phase:** Aggregation can be done by (a) a small model if the aggregation is purely
  structural (concatenation, deduplication, sorting), or (b) a larger model if synthesis is
  required. *(6 Multi-Agent Orchestration Patterns for Production)*

**Practical rule:** If the reduce step requires judgment (evaluating quality, choosing between
alternatives), escalate to ≥7B. If it requires structure (merging JSON arrays, removing
duplicates), a 3B model is usually sufficient.

### 1.4 Pipeline Decomposition (Sequential Chains)

For inherently sequential tasks (parse → extract → validate → summarize):

- Each stage should be a 3B model with a frozen, narrow system prompt.
- Pass state through a shared structured format (e.g., JSON, typed dict) rather than free text. A
  four-agent pipeline accumulates roughly 29,000 tokens versus 10,000 for an equivalent single-agent
  approach — free-text state is the hidden cost. *(6 Multi-Agent Orchestration Patterns for Production)*

**Error propagation is the killer:** Bad output in stage 1 cascades silently. You must insert a
verification gate between every stage (see Section 3).

---

## 2. Instruction Following at 3B Scale

### 2.1 Why 3B Models Fail at Instructions

Empirical evaluation of 20 open-source SLMs (0.4B–10B) on code benchmarks reveals that ≤3B models
exhibit specific failure modes: *(Assessing Small Language Models for Code Generation)*

- **Function hallucination:** Inventing tool/function names that do not exist in the provided schema.
- **Dependency errors:** Wrong ordering of operations (e.g., using a variable before it is defined,
  calling a function before its prerequisites are met).
- **Syntax drift:** Gradually deviating from the requested output format (JSON, XML, markdown) as
  context grows.
- **Over-eager execution:** When given a choice between "do nothing" and "guess," 3B models guess
  aggressively.

The TinyAgent study observed that off-the-shelf small models (1.1B–7B) "are not able to output the
correct plans when prompted the same way" as large models, with errors ranging from "wrong set of
functions, hallucinated names, wrong dependencies, and inconsistent syntax."
*(TinyAgent: Function Calling at the Edge)*

### 2.2 Instruction Template Design That Works

Based on the Gitara 3B agent and TinyAgent fine-tuning work, the following template
characteristics reliably improve 3B model adherence:

**A. Schema-First Prompting**

Place the output schema before the task description. Small models benefit from priming: the first
tokens they see heavily influence the generation distribution.

```plain
[SCHEMA]
You must output exactly:
{"name": "<tool_name>", "parameters": {<args>}}

[TASK]
<user request>

[EXAMPLES]
<2–3 examples only>
```

The Gitara 3B agent used this pattern to achieve 0.94 accuracy on a structured tool-calling task,
matching a 70B teacher model. *(Gitara: How we trained a 3B Function-Calling Git Agent)*

**B. Negative Sampling in Training**

If fine-tuning the 3B model, include irrelevant tools/functions as negative samples in the training
prompt. This teaches the model how to select appropriate tools. TinyAgent found this "particularly
effective for teaching the model how to select appropriate tools for a given query."
*(TinyAgent: Function Calling at the Edge)*

**C. Explicit "Do Nothing" Tool**

Always provide a no-op or refusal tool. Small models, when uncertain, will emit arbitrary output
rather than abstain. A `do_nothing` tool with a few examples (e.g., "make a sandwich" →
`do_nothing`) dramatically reduces hallucinated tool calls.
*(Gitara: How we trained a 3B Function-Calling Git Agent)*

**D. Constrained Decoding (Strongly Recommended)**

Where possible, configure the inference engine to enforce output constraints (e.g., JSON schema,
regex). This compensates for the 3B model's weaker self-regulation. Constrained decoding is
mentioned by the Gitara team as a likely next improvement for their 3B model.
*(Gitara: How we trained a 3B Function-Calling Git Agent)*

**E. One-Shot Over Zero-Shot**

For small models, a single well-chosen example usually outperforms elaborate zero-shot instruction.
This is the opposite of large models (GPT-4 class), where zero-shot with detailed rubrics often works
best. The SLM code-generation study found that "base models do not reliably benefit from
demonstration-based prompts" unless they are instruction-tuned.
*(Assessing Small Language Models for Code Generation)*

**F. Temperature and Sampling Discipline**

- **Temperature:** 0.1–0.2 for deterministic tasks (tool calling, formatting). Higher temperatures
  increase syntax-error rates in 3B models.
- **Top-p:** 0.95 (standard, but avoid top-k truncation which can cut off rare-but-correct tokens).
- **Max generation:** Strictly cap. Small models will "ramble" if given a long leash. Default to 512
  tokens; extend to 2,048 only for known long-form tasks. *(Assessing Small Language Models for Code Generation)*

---

## 3. Verification and Aggregation

### 3.1 The Core Economics Insight

Your verifier does not need to be your most expensive model. Verification is fundamentally easier than
generation. Research on agent evaluation finds that a smaller model used as a dedicated verifier —
with a well-crafted rubric — consistently outperforms a larger model asked to check its own work ad
hoc. *(How Multi-Agent Self-Verification Actually Works)*

In practice: if your solver is a 7B model, your judge can often be a 3B model (or even 1.5B). The
cost difference between "7B judges 7B" and "7B judged by 3B" across 10,000 calls per day is
significant enough to make or break the business case for verification entirely.
*(How Multi-Agent Self-Verification Actually Works)*

### 3.2 Four Verification Architectures

**Pattern 1: Output Scoring (LLM-as-Judge)**

A separate 3B model evaluates the solver's output against a structured rubric (e.g., factual
accuracy 0–10, completeness 0–10, logical consistency 0–10).

Returns JSON: `{"score": <avg>, "reason": "<one sentence>"}`.

- **Best for:** High-volume pipelines where you need a cheap, fast gate. Customer support triage,
  document classification, code review at scale.
- **Failure mode:** Judges are biased toward fluency over correctness. A hallucination in
  professional language will outscore a correct but awkward answer. Counter this with explicit
  rubric items for factual specificity. *(How Multi-Agent Self-Verification Actually Works)*

**Pattern 2: Reflexion (Self-Critique Loop)**

Solver generates → critic produces verbal feedback → solver retries with feedback in context.

- **Best for:** Tasks with clear correctness criteria where the model can meaningfully improve given
  feedback — coding problems, factual Q&A, structured data extraction.
- **Failure mode:** On hard examples, loops diverge. After two or three rounds of conflicting
  feedback, the model oscillates rather than converges. Hard cap at 3 retries.
  *(How Multi-Agent Self-Verification Actually Works)*

**Pattern 3: Adversarial Debate**

Two agents with different personas independently propose answers, critique each other's reasoning,
and a judge synthesizes a final verdict.

- **Best for:** High-stakes decisions with no single ground truth — strategy recommendations, legal
  analysis, complex research synthesis.
- **Failure mode:** Minimum 5× inference cost. Both agents may share training blind spots and
  converge on the same wrong answer. *(How Multi-Agent Self-Verification Actually Works)*

**Pattern 4: Process Verification (Step-by-Step)**

A verifier checks each step of the execution trace before it propagates downstream.

- **Best for:** Long-horizon research agents, multi-file code refactors, planning agents. Catching
  errors at the step level dramatically reduces compounding failures.
- **Failure mode:** Cost scales linearly with workflow depth. A 10-step pipeline adds 10 verification
  calls. Only viable when the cost of a wrong final answer exceeds the verification overhead.
  *(How Multi-Agent Self-Verification Actually Works)*

### 3.3 Aggregation Strategies for Fan-Out Results

When multiple 3B workers produce parallel outputs, use these merge strategies in order of cost:

1. **Majority voting (structural):** For categorical outputs (e.g., classification labels,
   pass/fail), simple majority vote. No LLM call required.
2. **Weighted merging (structural):** For ranked lists or scored outputs, merge by score and
   deduplicate. A 3B model can handle this if given a strict schema.
3. **LLM-based synthesis (semantic):** For natural-language outputs (summaries, recommendations), a
   3B model can synthesize 2–3 candidate answers into one, but quality degrades beyond 3 inputs. For
   >3 inputs, escalate to a larger model.
4. **Human review routing:** When workers disagree strongly (e.g., no majority on a binary decision),
   flag for human review rather than forcing a consensus. *(6 Multi-Agent Orchestration Patterns for Production)*

### 3.4 DAG-Based Orchestration Verification

For multi-step tool calling (the TinyAgent approach), construct a Directed Acyclic Graph (DAG) of
planned function calls and verify that the generated DAG is isomorphic to the expected structure. This
is a cheap, deterministic check that catches wrong tool selection and wrong dependency ordering before
execution. *(TinyAgent: Function Calling at the Edge)*

---

## 4. Context Budget Allocation

### 4.1 The 4K Window Breakdown

For a 3B model with a 4K context window, the following allocation is empirically robust:

| Budget | Tokens | Purpose |
|--------|--------|---------|
| System / instruction | 500–800 | Role definition, output schema, 1–2 examples, do-nothing demonstration |
| Retrieved / task context | 1,000–1,500 | The actual content to process (code, document chunk, data) |
| Tool / API descriptions | 0–1,000 | Only relevant tools (see Tool RAG below) |
| Scratch / reasoning buffer | 500–800 | Chain-of-thought, intermediate steps |
| Generation reserve | 1,000–2,000 | Final output tokens |

Never exceed 3,000 tokens in the combined prompt (system + context + tools). If your task needs more
than ~1,500 tokens of context, you must chunk or compress.

### 4.2 Tool RAG: Reducing Tool Description Bloat

The single biggest waste of context in agent prompts is including all available tool descriptions.
TinyAgent's research shows that only ~4 tools are needed per query on average, even when 16 are
available. *(TinyAgent: Function Calling at the Edge)*

**Implementation:** Use a lightweight classifier (e.g., DeBERTa-v3-small, ~50M parameters) to predict
which tools are needed from the user query alone. This is a multi-label classification problem: output
a probability vector over all tools, select those >50% threshold.

**Results from TinyAgent:**

| Approach | Tokens | Success |
|----------|--------|---------|
| No RAG (all 16 tools) | 2,762 | 78.89% |
| Basic embedding RAG (top-3) | 1,674 | 74.88% (misses auxiliary tools) |
| Fine-tuned classifier (Ours) | 1,397 | 80.06%, 0.998 tool recall |

*(TinyAgent: Function Calling at the Edge)*

**Practical heuristic:** If you have >5 tools, invest in a tool classifier. The 50M-parameter
classifier pays for itself in prompt-token savings on the first day of production.

### 4.3 System Prompt vs. Dynamic Context Trade-offs

Production systems (e.g., AnythingLLM) often fix system-prompt allocation at 15% of the overall
window. This is a mistake for high-context workflows: it starves the system prompt on models with
large windows and over-allocates on small ones. *(AnythingLLM Issue #1244)*

**Better rule for 3B models:** System prompt is a fixed token budget (600–800 tokens), not a
percentage. Dynamic context (retrieved documents, conversation history, tool descriptions) fills the
remainder. This ensures the model's "personality" and output schema are always fully specified,
regardless of task size.

### 4.4 Compression Strategies

When context must exceed the budget:

- **Semantic compression:** Use a smaller model (e.g., 1B summarizer) to compress retrieved documents
  into bullet points before passing to the 3B worker.
- **Structured truncation:** For code, truncate to the function signature + docstring + 5 lines of
  body. For prose, truncate to the first paragraph + last paragraph + sentence with the query
  keywords.
- **Hierarchical retrieval:** First retrieve a coarse summary (high-level document outline), then
  retrieve fine-grained chunks only for the relevant sections.

---

## 5. Self-Regulation Mechanisms

### 5.1 The Escalation Cascade

A three-level escalation hierarchy prevents both unhandled failures and premature human interruption:
*(Agent Almanac: Production Coordination Patterns)*

**Level 1: Agent Self-Recovery**

Agent detects failure → retries with simplified approach (e.g., shorter prompt, fewer tools, smaller
context chunk).

- If resolved: continue, log incident.
- If not resolved after N=2 retries: escalate to Level 2.

**Level 2: Team-Level Response**

Lead agent (orchestrator) receives stalled status → applies degraded-wave policy.

- May: redistribute work, reduce scope, swap agents, skip non-critical steps.
- If resolved: continue with annotations.
- If team cannot recover: escalate to Level 3.

**Level 3: Human Intervention**

System pauses and presents: what failed, why, what was attempted, and the current partial state.

Never silently pass low-confidence outputs to downstream consumers.

### 5.2 Failure Detection Patterns

**Structured checklists over open-ended self-critique.**

A 3B model asked "Is your output correct?" will usually say yes. A 3B model asked to evaluate against
a 5-item checklist is more reliable:

```json
{
  "check_syntax": true,
  "check_all_required_fields_present": true,
  "check_no_hallucinated_keys": true,
  "check_values_within_expected_range": true,
  "check_output_format_matches_schema": true
}
```

Self-evaluation bias is real. The SELF-[IN]CORRECT hypothesis posits that LLMs may not be consistently
better at discriminating the quality of their own responses than they are at generating initial ones.
A separate, different model for evaluation (even if smaller) reduces this bias.
*(Mitigating Manipulation and Enhancing Persuasion: A Reflective Multi-Agent Approach)*

**Process monitoring for long-horizon tasks.**

Instead of verifying only the final output, verify each step. For a 3B model coding agent:

1. After parsing the user request: verify intent understanding (cheap).
2. After generating a plan: verify DAG structure (cheap, deterministic).
3. After each file edit: verify syntax and file structure (cheap, deterministic).
4. After all edits: verify compilation/tests (expensive, but localized).

This prevents error propagation and makes debugging deterministic.
*(How Multi-Agent Self-Verification Actually Works)*

### 5.3 Retry and Recovery Strategies

From the self-healing agent literature, seven recovery strategies are commonly used:
*(Retry / Self-Healing Agent Pattern)*

| Strategy | When to Use | 3B-Model Suitability |
|----------|-------------|----------------------|
| RETRY | Transient error (timeout, rate limit, flaky parser) | Yes — simplest and most common |
| SKIP | Non-critical step failed; continue without it | Yes — if the orchestrator can handle partial output |
| REPLAN | Plan is fundamentally wrong (wrong tool, wrong dependency) | No — 3B models are poor replanners. Escalate to larger model. |
| SUBSTITUTE TOOL | A specific tool is broken; swap in equivalent | Yes — if tool equivalence is pre-defined |
| REGENERATE PRIOR STEP | Output corrupted at step N; redo from step N-1 | Yes — if state is deterministic and cheap to recompute |
| ESCALATE FIDELITY | Small model consistently failing; swap to larger model | Yes — this is the primary escalation path for 3B agents |
| ESCALATE (human) | All automated recovery exhausted | Always the final step |

**Circuit breakers:**

- **Max retries per task:** 3 (hard cap). Regular max-retry hits on a specific task type are a signal
  to reroute to a specialist tool or human, not to increase the cap. *(How Multi-Agent Self-Verification Actually Works)*
- **Max retries per step:** 2 (softer cap). If a single step fails twice, escalate to replan or human
  rather than looping.
- **Monotonicity check:** For iterative tasks (e.g., refactoring), verify that each iteration improves
  a measurable metric (test pass count, compilation error count, cyclomatic complexity). If a retry
  makes things worse, stop and escalate. *(Feature: Agent self-correction validation gate)*

### 5.4 Logging and Observability for Small Models

Small models are more prone to silent failure than large ones (they are less likely to signal
uncertainty). Every 3B subagent call should log:

- Input hash (for reproducibility)
- Output schema validity (pass/fail, deterministic)
- Self-reported confidence (if the model is prompted to output it; treat with skepticism)
- Verifier score (from the verification layer)
- Retry count (per task, per step)
- Escalation flag (if any)

Track retry distributions in production. A task type with >10% retry rate is a candidate for
re-scoping, re-training, or escalation to a larger model. *(How Multi-Agent Self-Verification Actually Works)*

### 5.5 When to Escalate from 3B to Larger Models

Use these thresholds as rules of thumb:

| Trigger | Escalation Target | Rationale |
|---------|-------------------|-----------|
| Task requires >3 reasoning steps in sequence | 7B+ | 3B models lose coherence in long chains |
| Task requires cross-file / cross-document reasoning | 7B+ | 3B models lack the working memory to track remote context |
| Output is user-facing and brand-sensitive | 7B+ or human | Error tolerance is near zero |
| Task has failed 2+ retries with different prompts | 7B+ or human | The task is likely out-of-distribution for the 3B model |
| Tool set has >10 options with subtle distinctions | 7B+ or tool classifier | 3B models struggle with fine-grained discrimination at scale |
| Task requires creative synthesis (not transformation) | 7B+ | 3B models excel at structured transformation, not open-ended creation |

---

## 6. Summary Checklist for Architecture Design

When designing a system that uses 3B-class subagents, verify each item:

- [ ] **Decomposition:** Every subtask fits in ≤1,500 tokens of context + ≤2,000 tokens of generation budget.
- [ ] **Interfaces:** Worker inputs and outputs use strict schemas (JSON, typed structures), not free text.
- [ ] **Tooling:** If >5 tools are available, a tool classifier (≤100M params) pre-filters to ≤4 relevant tools per query.
- [ ] **Instructions:** System prompt includes schema, 1–2 examples, and a do_nothing option. Schema is placed before the task.
- [ ] **Verification:** At least one of: output scoring, process verification, or DAG validation is in place. Verifier is a different model from the solver.
- [ ] **Retries:** Hard cap of 3 retries per task, 2 retries per step. Monotonicity check for iterative tasks.
- [ ] **Escalation:** Clear three-level escalation (self-recovery → team replan → human) with defined entry criteria.
- [ ] **Observability:** Every subagent call logs input hash, schema validity, verifier score, retry count, and escalation status.
- [ ] **Fallback:** When 3B workers fail, there is a defined path to a larger model or human, not a silent degradation.

---

## References

- 6 Multi-Agent Orchestration Patterns for Production (2026)
- How Multi-Agent Self-Verification Actually Works (2026)
- Assessing Small Language Models for Code Generation (2025)
- TinyAgent: Function Calling at the Edge (2024)
- Gitara: How we trained a 3B Function-Calling Git Agent (2025)
- Retry / Self-Healing Agent Pattern (2026)
- Agent Almanac: Production Coordination Patterns
- Mitigating Manipulation and Enhancing Persuasion: A Reflective Multi-Agent Approach (2025)
- Prioritizing Real-Time Failure Detection in AI Agents (2025)
- Lifelong Learning of Large Language Model based Agents: A Roadmap (2025)
- Smaller Language Models Are Better Instruction Evolvers (2024)
- AnythingLLM Issue #1244: System prompt allocation
- Feature: Agent self-correction validation gate — Anthropic Claude Code Action
