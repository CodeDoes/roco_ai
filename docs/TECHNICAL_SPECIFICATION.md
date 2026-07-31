# RoCo AI: Architectural & Technical Specification

This document provides a comprehensive, production-grade technical specification of the **RoCo AI** collaborative writing ecosystem. It details the design philosophy, architectural components, state-management models, and safety boundaries necessary for anyone to recreate the entire application from scratch.

To maintain a pure conceptual and design-focused blueprint, **no code snippets** are used. Instead, this specification explains the deep engineering reasoning, architectural decisions, and logic behind every component of the system.

---

## 1. Executive Summary & Design Philosophy

RoCo AI is a standalone-first local AI collaborative writing environment designed to support high-fidelity, hallucination-free generation of complex documents, stories, and structured data on resource-constrained consumer hardware. The system is built around several foundational guidelines:

### A. Standalone-First Local Loop
Most AI tools rely on cloud APIs, exposing user privacy, incurring network latency, and breaking offline workflows. RoCo AI is designed to run entirely locally. The primary model is an RNN-based architecture (specifically RWKV-7 2.9B), executing on consumer GPUs/CPUs. By prioritizing local execution, the system achieves instant responsiveness and zero data leaks.

### B. Grammar-Constrained Security ("The Grammar Shield")
LLMs are notoriously prone to formatting failures, leaking internal "thinking" blocks, or hallucinating invalid fields when asked to produce JSON. Rather than relying on fragile system prompts or post-generation regex parsing, RoCo AI restricts the model *at the token generation step*. Using Context-Free Grammars (BNF/GBNF), the model is physically prevented from emitting tokens that violate a schema.

### C. Recurrent State Tuning & Snapshotting
Transformers require re-evaluating the entire conversation context (prefix pre-filling) with each turn, which scales quadratically. By selecting a Recurrent Neural Network (RNN) architecture, RoCo AI leverages the fact that conversation memory is compressed into a fixed-size recurrent state. This state can be snapshotted, duplicated, blended, and hot-swapped instantly, enabling instant few-shot persona loading without re-evaluating long prompt prefixes.

### D. Multi-Turn Reversibility & Rollbacks
Agentic workflows often fail mid-execution or produce suboptimal subtasks. RoCo AI models its execution flow as a transaction. If a verification step fails or a model output is rejected by a reviewer agent, the system performs a deterministic rollback to a previous checkpoint state, correcting the generation pathway.

---

## 2. System Anatomy & Workspace Topology

To manage build times, cognitive load, and decouple user interfaces from raw execution, the application is divided into cleanly bounded domains. The ecosystem comprises a workspace of modular packages, detailed below by their conceptual responsibilities.

```
+-----------------------------------------------------------------------------------+
|                                 USER INTERFACES                                   |
|   +-----------------------+   +-----------------------+   +-------------------+   |
|   | Terminal UI (TUI)     |   | Desktop GUI (GUI)     |   | Command Line (CLI)|   |
|   +-----------+-----------+   +-----------+-----------+   +---------+---------+   |
+---------------|---------------------------|-------------------------|-------------+
                |                           |                         |
                +---------------------------+-------------------------+
                                            |
                                            v Connects To
+-----------------------------------------------------------------------------------+
|                             SERVER & GATEWAY LAYER                                |
|   +-----------------------+   +-----------------------+   +-------------------+   |
|   | Gateway Router        |   | OpenAI HTTP Server    |   | Client SDKs       |   |
|   +-----------+-----------+   +-----------+-----------+   +---------+---------+   |
+---------------|---------------------------|-------------------------|-------------+
                |                           |                         |
                +---------------------------+-------------------------+
                                            |
                                            v Resolves To
+-----------------------------------------------------------------------------------+
|                             CORE APPLICATION BOUNDARY                             |
|   +---------------------------------------------------------------------------+   |
|   | App Context (Workspace Config, Tracing, Session Manager)                 |   |
|   +---------------------------------------+-----------------------------------+   |
|                                           |                                       |
|                                           v Orchestrates                          |
|   +---------------------------------------------------------------------------+   |
|   | Agent Orchestration Framework (ReAct Agent, Outline Planner, Editor)      |   |
|   +---------------------------------------+-----------------------------------+   |
+-------------------------------------------|---------------------------------------+
                                            |
                                            v Interacts With
+-----------------------------------------------------------------------------------+
|                           INFERENCE & CONSTRAINT SHIELD                             |
|   +-------------------+   +-----------------------+   +-----------------------+   |
|   | Model Backend Seam|   | GBNF/KBNF Grammar     |   | State Cache & Slots   |   |
|   +---------+---------+   +-----------+-----------+   +-----------+-----------+   |
+-------------|-------------------------|---------------------------|---------------+
              v                         v                           v
+-----------------------------------------------------------------------------------+
|                         LOCAL SANDBOXED FILE SYSTEM                               |
|   +---------------------------------------------------------------------------+   |
|   | Sandbox Workspace (Strict Components, Traversal Defense, Extension Rules) |   |
|   +---------------------------------------------------------------------------+   |
+-----------------------------------------------------------------------------------+
```

### A. The Core Application Boundary
The application orchestrator acts as the glue. It loads configurations, configures the environment-aware logging tracing subscriber, maintains active sessions, and links the user interfaces to the agents and the model backend.

### B. The Dual-Agent Design Pattern
A key structural decision is the separation of the production system from the offline evaluation harness:
1. **Production Agents:** Designed for dynamic human-in-the-loop writing. These manage outline modifications, pacing adjustments, character profiles, and draft expansions. They interact directly with the live user interface.
2. **Offline Evaluation Harness:** A standalone package designed to run simulated scenarios (education, home automation, coding tasks) offline using a mocked backend. This separation ensures that developers can test and benchmark agent retry/rollback resilience without booting the GPU-heavy production pipeline.

---

## 3. Core Inference Engine & RNN Recurrent State Tuning

The inference layer isolates the rest of the application from specific hardware-accelerated drivers (Vulkan, OpenCL, Triton) by exposing a unified model abstraction.

### A. The Unified Model Seam
Any backend must implement a single interface that defines:
- **Generation Capabilities:** Accepting a prompt, temperature, max token limits, and an optional grammar constraint.
- **Recurrent State Management:** Saving, loading, and element-wise blending of model states.
- **Pre-Baking/Prefilling:** Processing prefix sequences without generating outputs to warm the model's memory.

### B. Recurrent State Architecture & "Baking"
In traditional Transformer architectures, providing few-shot examples or system prompts requires re-processing those prompt tokens in every single generation, costing significant processing time. In an RNN architecture (like RWKV), the past context is compressed into a numerical hidden state vector.
RoCo AI implements "State Baking":
1. The developer or user defines a system persona and a series of high-quality "User / Assistant" few-shot conversation turns.
2. The engine processes these prompt sequences *once* during initialization, setting the generation target length to zero.
3. The resulting hidden state is saved to a named file or "Session Slot."
4. When a new generation is requested, the engine instantly loads this state as the initial recurrent vector. The model immediately begins generating with the persona baked in, entirely bypassing prefix processing time.

### C. State Blending
Because RNN states are vector representations of context, they can be blended using a linear combination. By taking two separate baked states (e.g., a "Sci-Fi Suspense" state and a "Humorous Character Dialogue" state) and blending their weight tensors element-wise (e.g., using a 70/30 ratio), the system creates a brand-new hybrid state that exhibits traits of both personas without training a new model.

### D. Addressing "Think-Tag" Contamination
Modern models trained with reasoning processes frequently emit internal thoughts wrapped in `<think>` or `thinking` tags. While helpful for planning, these tags ruin the output layout of collaborative story text or JSON parameters.
RoCo AI addresses this through two techniques:
1. **No-Think Prefilling:** The system can force-feed a short string (e.g., ` thinking response`) immediately after the assistant prompt prefix. This tricks the model's recurrent state into believing it has already completed its internal thinking cycle, forcing it to generate direct content.
2. **No-Think Recurrent State Baking:** During the initialization phase, the assistant's few-shot examples are fed through the engine with no-think prefixes prefilled in the assistant role. This bakes a bias directly into the session's recurrent state, training the model's activation weights to avoid entering the planning state altogether.

---

## 4. Constrained Decoding & The Grammar Shield

To guarantee 100% reliable system boundaries, RoCo AI features a strict token-level grammar constraint system.

### A. Context-Free Grammar Compilation
Grammars are written in standardized GBNF (Gnu Backus-Naur Form). When an agent initiates a query requiring structured output (such as a JSON envelope containing specific keys), the grammar compiles into a state machine representation (using a regex and pushdown-automaton parser like KBNF).

### B. Token-Level Masking During Decoding
The grammar engine executes in step with the model's token selection loop:
1. At each token step, the model generates logit values (raw probabilities) for its entire vocabulary (e.g., 65,000 tokens).
2. Before the sampler selects a token, the grammar engine computes the set of allowed next characters based on the current state of the schema.
3. It converts this allowed-character set into a binary bitset mask corresponding to the vocabulary's token strings.
4. Any token in the vocabulary that does not match the schema's expectation has its logit overwritten with negative infinity.
5. The sampler then selects from the remaining allowed tokens.

This architectural decision ensures that the model can *never* produce syntactically invalid JSON, miss a required field, or leak formatting markdown.

---

## 5. Agent Orchestration & Collaboration Framework

Collaborative writing is more complex than standard chat; it requires tracking plots, adjusting pacing, and structuring drafts.

### A. Production Agent Archetypes
The production agent framework is built on two primary structural patterns:
- **The Plan-First Agent (Mechanistic):** Used for analytical tasks. It divides a high-level task into distinct sub-plans, resolves them sequentially, evaluates the result, and rewrites the sub-plans if any step is rejected.
- **The Chat ReAct Agent (Common):** Used for natural conversation. It executes a loop of **Thought** (determining what to do), **Action** (invoking a local tool or querying state), and **Observation** (integrating the tool result) to collaborate with the user.

### B. The Collaborative Writing Pipeline
The production pipeline structures story generation across dedicated agent roles:
1. **The Outliner Agent:** Responsible for establishing the meta-structure, themes, characters, and chapter-by-chapter summaries.
2. **The Pacing Agent:** Analyzes the transition between chapters to ensure suspense, action, and character-driven scenes are balanced.
3. **The Expansion Agent (Writer):** Takes a single chapter's summary and expands it into rich, descriptive prose, adhering to the structural tone set by the outliner.
4. **The Quality Reviewer Agent:** Reviews generated prose against thematic and structural criteria, outputting a clear evaluation status.

### C. The State-Tracking Session System
Writing sessions represent a state machine. The system maintains a shared document journal containing the full edit history. Every change (such as adding a paragraph, modifying a character description, or revising an outline chapter) is stored as an atomic operation. This ensures that the human collaborator can undo or inspect any agent-driven action.

---

## 6. Offline Evaluation & Sandboxed Execution Harness

For security and developer productivity, the offline evaluation harness is fully decoupled and highly constrained.

### A. Sequential Execution Loop & Transactional Integrity
The evaluation harness runs a series of tasks (e.g., simulated home automation scripts, document formatting) through an agent loop. To prevent failure cascade, the execution loop treats runs as transactions:
1. The harness captures a complete state checkpoint before executing an agent.
2. The agent runs, emitting files or updating databases.
3. A separate validator engine parses and evaluates the results.
4. **If execution succeeds but verification fails:** The harness immediately invokes a rollback, reverting the files and virtual states to the checkpoint and requesting a retry with the verification failure report injected as feedback. This guarantees that corrupt files never persist in the workspace.

### B. Lexical Path Traversal Security
Allowing an LLM or local agent to read and write files poses a massive security risk. A malicious prompt could command the agent to read system files or overwrite system configurations.
Standard path checks (e.g., verifying if a path starts with the workspace root string) are highly vulnerable to relative traversal (such as `/workspace/root/../../etc/passwd`).

RoCo AI implements an absolute sandboxing safety pattern:
1. Every file read/write request must go through a custom **Sandbox Guard**.
2. The Sandbox Guard dissects the target path into its individual lexical components.
3. It strictly rejects the path if it contains any parent directory components (`..`), root component prefixes (`/`), or empty components (`.`).
4. It resolves the clean relative path directly against the fully canonicalized workspace root.
5. **Extension Whitelisting:** The guard maintains a strict list of allowed file extensions (such as `.txt`, `.json`, `.md`). Any write request attempting to create a executable or configuration file (such as `.sh`, `.exe`, `.conf`) is instantly blocked, preventing agents from altering system behaviors.

---

## 7. The Ecosystem Interfaces: Clients, Servers, and Gateways

To fit into the modern developer and writer workflow, RoCo AI is accessible through three distinct interface layers, all connecting to the same unified app core.

### A. Client Interfaces
- **The Terminal UI (TUI):** A low-latency interface utilizing keyboard-driven navigation. It splits the screen into a file manager, active chat stream, and document preview, allowing developers to manage writing projects entirely from the terminal.
- **The Desktop GUI:** A visual application constructed around widgets:
  - **The Markdown Editor:** Combines side-by-side editing with live spelling and agent writing assists.
  - **The Link Graph:** Visualizes relationships between story characters, locations, and events as an interactive network of nodes.
  - **The Pacing Monitor:** Displays a structural chart showing tension levels per chapter as designed by the pacing agent.

### B. OpenAI-Compatible Daemon Server
To allow developers to integrate standard web frontend frameworks or alternative client tools (such as Vercel AI SDK or Assistant UI) without modifications, the system runs an HTTP daemon. This server implements the official OpenAI completion and streaming protocol, serving endpoints like `/v1/completions` and `/v1/chat/completions`.
Under the hood, it translates OpenAI request parameters into RoCo's internal `CompletionRequest` formats and converts token buffers into server-sent events (SSE) for seamless text streaming.

### C. Proxy Gateway and Rate Limiter
When deployed in multi-user local environments or collaborative LAN networks, a gateway layer sits in front of the daemon server. The gateway provides:
- **Rate Limiting:** Protects the single-threaded GPU inference engine from being overwhelmed, using a token bucket queue to buffer incoming requests.
- **Error Resilience:** Intercepts engine crashes or driver hangs, responding to clients with clean error states while executing auto-recovery protocols on the local daemon.

---

## 8. Logging, Configuration, and Diagnostic Infrastructure

High observability is critical to making a complex, multi-crate system maintainable and easy to debug.

### A. Priority-Based Configuration Loading
Configuration behaves deterministically, ensuring that local project settings override global rules while allowing CLI flags to force overrides. Loading follows a strict, sequential hierarchy:
1. **Environment Variables:** Direct overrides (such as model path configurations) defined in the active terminal environment take absolute priority.
2. **Local Workspace Configuration:** Lookups search for a `.roco/config.toml` directory relative to the active folder, allowing individual writing projects to target different models or grammars.
3. **Global User Settings:** Falls back to XDG standard directories (e.g., `~/.config/roco/config.toml`).
4. **Auto-detection:** If no files are located, the configuration manager performs file-system discovery in local paths (`models/`) to auto-configure appropriate default model weights.

### B. Structured Tracing and Routing
To prevent debug statements from overlapping with natural language outputs on terminal stdout (which would break TUI layouts or stream parsing):
- The system routes all trace logs through a centralized logging framework.
- Live CLI/TUI modes configure custom filters that suppress standard out logs, writing structured trace telemetry directly into `.roco/roco.log`.
- Daemon modes serialize execution span logs to disk, creating comprehensive diagnostic trails that capture active session IDs, parent spans, and execution latencies for post-mortem analysis.

---

## Summary of Replication Blueprint

To recreate RoCo AI from scratch, a developer should build the following layers in order:
1. **The Sandbox Guard:** Establish secure, traversal-proof file system operations.
2. **The Inference Model Seam:** Bind your local hardware acceleration to a state-savable RNN model executor.
3. **The Grammar Decryptor:** Implement token-level logit masking using schema state engines to guarantee structured outputs.
4. **The Recurrent State Baker:** Create the state saving, pre-baking, and no-think prefill logic to bypass prefix calculation.
5. **The Transactional Harness:** Program the rollback-enabled execution loop to manage agent tasks safely.
6. **The Agent Roles:** Construct the Plan-First and ReAct structures, defining the cooperative Outliner, Writer, and Reviewer behaviors.
7. **The Server & Interfaces:** Expose the logic through a local OpenAI-compatible API and wrap it inside TUI, GUI, or CLI clients.
