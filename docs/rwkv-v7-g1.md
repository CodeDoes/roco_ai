# RWKV-7 and G1: Capabilities, Architectures, and System Integration

This document outlines the core mathematical and structural capabilities of the **RWKV-7 State Space Model (SSM)** and the **G1/g1h reasoning architectures**, specifically as they pertain to the 2.9B parameter offline model integrated into RoCo AI.

---

## 1. Mathematical Foundation: Why Recurrent State Space Models?

Traditional generative models rely on the Transformer architecture, which uses Softmax self-attention. The physical constraints of self-attention are:
1. **Quadratic Time Complexity**: Computing attention scores between all pairs of tokens scales at $\mathcal{O}(N^2)$ where $N$ is the sequence length.
2. **Quadratic Memory Scaling**: Storing the Key-Value (KV) cache for $N$ tokens scales at $\mathcal{O}(N^2)$ or $\mathcal{O}(N)$ depending on architecture optimizations, causing rapid memory exhaustion at long context windows.

### The RWKV SSM Paradigm
RWKV-7 replaces quadratic Softmax attention with a linear-time Recurrent State Space formulation. Mathematically, the update step can be represented as:

$$W_{k+1} = \text{diag}(\alpha_k) W_k + \beta_k \otimes \gamma_k$$

Where the current state tensor $W$ acts as a linear memory pool. This recurrence affords two major physical advantages:
- **$\mathcal{O}(1)$ Inference VRAM**: Regardless of whether the conversation history is 10 tokens or 10,000 tokens, the recurrent state tensor size is completely **constant**. The model does not suffer from "out of memory" (OOM) crashes as sequence length increases.
- **$\mathcal{O}(N)$ Training and Prefill Complexity**: Processing prompts is linear in time complexity, enabling highly efficient prompt prefilling on standard consumer GPUs.

---

## 2. State-Tuning (The "Bake" Pattern)

Because the entire context history is physically represented as a vector of floats (the model's recurrent state tensor), RoCo AI does not need to send the entire system prompt and few-shot templates over the wire on every single HTTP generation request.

Instead, we use a technique called **State-Tuning** (or the **Bake Pattern**):
1. The system prompt and few-shot examples are fed through the model exactly once.
2. The generated output token count is restricted to 0 (`max_tokens = 0`), meaning the model only computes the transition state without executing a sampling loop.
3. The resulting recurrent state tensor is cached on the GPU VRAM or saved to a `.state` binary sidecar.
4. When a user sends a message, we initialize the model using this cached state tensor, append the single user message, and generate.

### Why this is a game-changer for 2.9B models
On smaller consumer GPUs, re-processing a 2,000-token system prompt/few-shot example on every turn introduces a ~3-second latency penalty. State-Tuning bypasses this entirely, offering **instant sub-100ms response latencies** on consecutive chat turns.

---

## 3. Grammar Masking and Constrained Decoding

RoCo AI relies on **GBNF (Gramer-Based Normal Form)** grammar masks to force the model's outputs to strictly comply with structured layouts (such as JSON schemas for outlines, character bibles, and story quality validations).

### Execution Flow
1. During sampling, instead of picking a token directly from the raw model logits, the logits are fed to a grammar-based token mask.
2. The token mask (compiled from GBNF via `roco-bnf-engine` and `kbnf`) sets the probability of all syntactically illegal tokens to $-\infty$.
3. The model is forced to choose only from the remaining valid tokens, guaranteeing 100% syntactically valid JSON structures.

### Structural Nuances of RWKV-7 Under Constraints
- **GBNF `|` Precedence Trap**: The GBNF operator `|` has low precedence. Inlining an alternation (e.g. `"pass" | "fail"`) inside an object rule can cause the engine to bypass surrounding braces. Therefore, RoCo AI compiles all enums as independent named rules.
- **In-String Quality Degradation**: While the JSON *structure* is guaranteed to be 100% correct, the actual content inside JSON string values can sometimes degrade under severe constraints, producing repetitive or degenerate tokens. Safe revision/retry loops are implemented to detect and heal these.

---

## 4. In-Context Learning and G1/g1h Reasoning

The G1 and g1h architectures are specialized prompting paradigms designed to unlock deep reasoning in recurrent networks.

- **Dynamic Pacing & CoT (Chain of Thought)**: Rather than forcing the model to generate a final answer immediately, G1 prompts the model to emit collapsible `<think>` blocks. This gives the model "compute time" to formulate its path before committing to the final text.
- **Hallucinated Turn Cutting**: When generating multi-turn dialogs or stories, models often hallucinate the user's responses. RoCo AI tracks generated token sequences and terminates early if the model attempts to write `User:` or `RoCo:`.
