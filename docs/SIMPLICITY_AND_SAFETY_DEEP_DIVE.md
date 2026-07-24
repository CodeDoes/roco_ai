# RoCo AI: Simplicity and Safety Deep-Dive Architectural Review

This document contains a comprehensive, production-grade conceptual audit of the **RoCo AI** codebase. It explores developer experience (DX), safety guarantees, system complexity, and structural clarity, directly addressing how developers interact with, comprehend, and modify this local AI collaborative writing environment.

---

## 1. System Anatomy: How Things Are Getting Used

### Primary Execution Flow (Production)

The core application runs in a **standalone-first local loop** connecting user-facing inputs to local inference (RWKV-7 2.9B). The data flow from user interaction to execution behaves as follows:

```
+------------------------------------------+
|  User Interfaces                         |
|  - roco CLI (crates/cli)                 |
|  - desktop GUI (crates/ui)               |
|  - Editor Plugin / Server (crates/server)|
+---------------------+--------------------+
                      |
                      v Constructs
+------------------------------------------+
|  AppContext (crates/app)                 |
|  - Binds Workspace, Engine, Sessions     |
|  - Blocks or streams sync/async calls    |
+---------------------+--------------------+
                      |
                      v Orchestrates
+------------------------------------------+
|  Agent Framework (crates/agent)          |
|  - Plan-First (MechanisticAgent)         |
|  - Chat ReAct (CommonAgent)              |
|  - Story Pipelines / Outline / Quality   |
+---------------------+--------------------+
                      |
                      v Constrained By
+------------------------------------------+
|  Grammar & Validation                    |
|  - roco-bnf-engine (crates/bnf-engine)   |
|  - kbnf Regex state (crates/grammar)     |
+---------------------+--------------------+
                      |
                      v Executes
+------------------------------------------+
|  Inference Backend                       |
|  - web-rwkv GPU thread (crates/inference)|
|  - MockBackend Fallback (crates/app/la)  |
+---------------------+--------------------+
                      |
                      v Persists
+------------------------------------------+
|  Workspace (crates/workspace)            |
|  - Confined directory sandboxing         |
+------------------------------------------+
```

### The Dual-Agent Paradox: Production vs. Mock

A major conceptual friction point in this workspace is the existence of **two entirely separate agent systems**:
1. **Production System (`crates/agent`)**: Implements rich collaborative writing, outline revision, chapter generation, pacing, and human-in-the-loop controls. This is what the CLI and GUI actually use.
2. **Mock Execution Framework (`crates/app/src/local_agent/`)**: An offline, fully mocked harness designed to simulate 70 use cases (privacy, home automation, niche edge, education). This utilizes a custom `DomainHarness` trait, standard `Sandbox`, and a sequential `ExecutionLoop`.

**Conceptual Friction:** While the Mock Framework is a fantastic, self-contained harness for testing local retry-and-rollback logic on a simulated CPU-only pipeline, a newcomer might get extremely confused about which files contain the actual production agent logic.

---

## 2. Developer Onboarding: Cognitive Load & Understandability

### Crate Fragmentation (19 Workspace Crates)
The cargo workspace comprises **19 separate crates** under `crates/`. This represents a high degree of fragmentation:

| Crate Category | Crates | Proposed Simplification (Consolidation) |
|---|---|---|
| **Messaging & Transport** | `message`, `chat-common`, `protocol` | Merge into a single `roco-protocol` crate. |
| **Agent & Oracles** | `agent`, `validation`, `tools` | Merge into `roco-agent` as modules. |
| **Model & Engine** | `engine`, `inference`, `grammar`, `bnf-engine` | Consolidate core logic into `roco-engine` (keeping `bnf-engine` as a vendor dependency). |

* **Why consolidation makes it simpler:** A developer making a change to a basic chat/message format (e.g., adding metadata or structural fields) currently has to modify 3–5 crates simultaneously, causing build graph rebuilds across almost the entire workspace. Combining them reduces compilation overhead and makes search paths trivial.

### Redundant Code & "Framework Clones"
* **The Problem:** The directory `crates/app/src/local_agent/test_clones/` contained five duplicate files (`framework_clone_1.rs` through `framework_clone_5.rs`). These were exact copies of `framework.rs` and added immense cognitive noise, leaving developers searching for their usage or purpose (only to discover they were unused).
* **Our Cleanup Action:** We have completely eliminated this directory and its declarations, removing 6 redundant files and immediately simplifying the file tree under `crates/app/src/local_agent/`.

---

## 3. Structural Clarity: Filepath Mapping & Search Paths

### Filepath Layout Analysis
Are filepaths conceptually understandable? Let's trace how folders map to concepts:

* **Clear Paths**:
  - `crates/ui/src/` is beautifully divided into separate standalone widget modules: `markdown_editor.rs`, `link_graph.rs`, `file_tree.rs`, `pacing.rs`. This design makes it incredibly easy to work on single frontend widgets.
  - `apps/node-local-agent/` cleanly encapsulates the Node.js / TypeScript port of the agent harness.
* **Confusing Paths**:
  - `crates/app/src/local_agent/` is a mock/scaffold, but because it sits directly inside `crates/app` (the core interface primitive), it suggests it is an integral part of the live desktop/CLI application.
  - *Recommendation:* Move the mock execution harness (`local_agent`) out of the core application library crate (`crates/app`) into an isolated integration test package or a dedicated evaluation tool crate, such as `crates/roco-harness`.

### Do You Have to Search for Things?
Yes, navigation is currently search-heavy due to tight coupling across many tiny crates. For example:
- `CompletionRequest` is defined in `crates/engine/src/lib.rs`.
- `structured_complete` (grammar decoding helper) is in `crates/agent/src/util.rs`.
- `SessionAgent` is defined in `crates/app/src/session.rs`.
- This requires the developer to constantly jump around the crate graph rather than following a single, local flow.

---

## 4. Setup, Environment, and Build Complexity

### Developer Setup Analysis
* **The Good:** End-user setup is highly streamlined. Running `./start.sh` compiles and executes the CLI, with configuration auto-detected via local directories or standard system paths.
* **The Friction:**
  - **Toolchain Conflicts:** Due to deep system interactions with GPU libraries (web-rwkv/WGPU), a mismatch between Nix-supplied Rust versions and rustup can break compilation with compilation environment errors (e.g., `E0514` compiler mismatches). `run_tests.sh` successfully mitigates this by manually injecting rustup paths into `PATH` before executing.
  - **Linker Configuration:** The Cargo setup defaults to the `mold` linker in `.cargo/config.toml`. If the developer's system does not have `mold` installed, cargo builds will fail. Developers must bypass this using custom flags (`RUSTFLAGS=""`) or install `mold` manually.

---

## 5. Production Simplicity: Elegant Usage with a Neat Backend

The system achieves a wonderful design balance: **The user interface remains exceptionally simple while grammar enforcement operates strictly in the background.**

### The Grammar Shield: Absolute Safety from Hallucinations
A classic local LLM challenge is *prompt contamination* and formatting failures. `RoCo AI` achieves a brilliant safety architecture:
* **Production Decoders:** Every LLM call uses kbnf-based BNF grammars to constrain structure.
* **Why it's safe:** The model is physically incapable of emitting raw `thinking` tokens or unstructured garbage, avoiding standard parser breakages.
* **Why it's neat:** This is transparent to the end-user. They interact with clean UI panels while the backend strictly shapes raw tokens through context-free grammars.

---

## 6. Realized Safety & Consistency Upgrades

As part of this conceptual audit, we implemented concrete security and consistency changes to make the codebase safer and simpler:

### A. Preventing Lexical Path Traversal in Sandbox
We discovered a serious security issue in `Sandbox::read` and `Sandbox::write` inside the local agent execution module:
* **Vulnerability:** They used lexical `starts_with` checks (`full.starts_with(&self.root)`) to prevent sandboxed path escape. Since `starts_with` does not resolve relative directories, a path containing `../` traversal could successfully escape the root.
* **The Fix:** We implemented `is_safe_relative_path()`, which inspects path components and strictly rejects any absolute paths, prefixes, or relative traversals (`..` or `.`).
* **Enforced Containment:** We added `allowed(...)` verification directly into `read` and `write` methods, preventing agents from writing disallowed extensions (e.g., executing arbitrary scripts).

### B. Making Rollbacks Consistent and Deterministic
* **The Bug:** The execution loop was only rolling back when an agent returned an explicit runner error. If the run succeeded but the output failed verification, the loop continued without invoking a rollback, leading to inconsistent state.
* **The Fix:** We aligned the loop so that both runner errors and verification failures trigger a clean state rollback, ensuring full state integrity.

---

## Summary of Refactoring Recommendations for a Simpler Future

To make RoCo AI even simpler and more robust, we propose the following evolutionary steps:
1. **Crate Consolidation:** Reduce the 19 workspace crates to 6-8 comprehensive crates.
2. **Harness Isolation:** Move the simulated `local_agent` package from `crates/app` into its own crate (`crates/harness`) or an `integration_test` folder to separate live app logic from mock evaluation scaffolds.
3. **Linker Portability:** Provide a fallback check in `.cargo/config.toml` or setup scripts so builds don't fail immediately on machines without `mold`.
