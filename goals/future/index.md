# Goals: Future (Archived)

> Ideas and features that are valuable but not on the critical path.
> These amplify a working core — build them after the story engine works.

## Archive Date: 2026-07-17

These goals are parked here until the local story generation engine is solid.
They represent the "nice to have" and "scale" layers.

---

## Memory & Retrieval

### FAISS Graph Vector Embeddings
- Hybrid vector + graph storage with petgraph
- <50ms retrieval latency for 100K document corpus
- Enables semantic search across sessions, notes, dreams
- **Dependency:** Requires working story sessions to index

### Session Search Enhancement
- Vector-based semantic search (beyond current Jaccard overlap)
- Entity/concept extraction from sessions
- Temporal + semantic + graph-based retrieval
- **Dependency:** FAISS integration

### Notes Research System
- Markdown-based note storage in `~/.roco/notes/`
- Auto-tagging with LLM (grammar-constrained JSON output)
- Bidirectional linking (`[[concept]]` syntax)
- **Dependency:** FAISS for semantic search

---

## Agent Intelligence

### Dreaming Pipeline (OpenClaw Dreaming)
- Offline memory consolidation during idle
- Pattern extraction from past sessions
- Insight synthesis and novel connections
- **Dependency:** Multiple completed story sessions to analyze

### Self-Training
- Preference learning from user feedback (DPO/RLHF)
- Collected (prompt, response, feedback) triples
- Fine-tuning smaller models on preferences
- **Dependency:** 100+ feedback data points

### Self-Prompt Adjustment
- Meta-learning for prompt optimization
- Version-controlled prompt templates
- A/B testing old vs new prompts
- **Dependency:** Working story pipeline with quality metrics

---

## User Interfaces

### TUI App (Terminal User Interface)
- Multi-pane layout with ratatui (sessions, chat, context, memory, agent status)
- Session management, context viewer, memory browser
- Vim-like keybindings
- **Priority:** Medium — CLI works fine for story generation

### Web App
- Next.js 15 + Assistant UI
- Streaming chat, session management, memory explorer
- Agent playground with visual feedback
- **Priority:** Medium — local-first philosophy

### Dashboard
- Real-time metrics collection
- Usage, performance, quality tracking
- Anomaly detection and alerts
- **Priority:** Low — premature optimization

### Agent Manager
- Multi-agent orchestration
- Agent templates (coder, writer, researcher)
- Lifecycle management, resource limits
- **Priority:** Low — single agent works for stories

---

## Developer Tools

### Project Stats
- Codebase analytics, complexity scores
- Test coverage trends, build time analysis
- Grammar coverage audit
- **Priority:** Low

### ORPC-RS, NAPI-RS, ZOD-RS Integration
- Auto-generated API clients from Rust types
- Node.js bindings via NAPI-RS
- Type-safe cross-language schemas
- **Priority:** Low — no external consumers yet

---

## Infrastructure

### Inference Optimization
- Batching, speculative decoding, KV cache optimization
- Multi-GPU support, model routing
- **Priority:** Medium — 2.9B performance is acceptable

### Gateway Hardening
- Rate limiting, authentication, load balancing
- Response caching, observability
- WebSocket support
- **Priority:** Low — local-only is fine

### Browser Use
- Driving a real browser via agent tools
- Page understanding, stealth, agent-controlled navigation
- **Priority:** Low — defer until agent loop is robust

---

## When to Unarchive

These goals move back to active when:
1. ✅ Story engine generates indefinitely with coherent plot
2. ✅ Interactive mode works (human feedback loop)
3. ✅ Per-handler grammars eliminate think-block leakage
4. ✅ Quality metrics are defined and measured
5. ✅ 10+ successful story runs completed

Then: pick the highest-impact item from each category and schedule it.

---

*Archived: 2026-07-17*
