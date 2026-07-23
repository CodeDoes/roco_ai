# All Local AI Use Cases + Harness Strategy (from Gemini synthesis)

## 1. Software Engineering & Development
- Code completion / refactoring (local, no leak)
- Local terminal agents (CLI execution)
- Synthetic test data generation

## 2. Document Processing & RAG
- Air-gapped document Q&A (local vector DB)
- Local OCR / parsing (multimodal vision)
- Private PII redaction

## 3. Audio, Speech, Vision
- Offline STT (Whisper variants)
- On-device TTS / voice assistants / VAD
- Local CV / NVR (object detection, facial recognition, LPR)

## 4. Systems Automation & Edge
- Smart home integration (local automation routing)
- Autonomous edge / robotics / drones
- System log / event monitoring (local security flags)

## 5. Regulated Industries
- Medical / clinical data analysis (HIPAA/SADP compliant)
- Financial / legal document audit (regulatory compliance)

## 6. Creative & Media
- Local image / vector generation (diffusion, upscaling, editing)
- Local narrative / world-building / interactive fiction / procedural dialogue

## 7. Expanded Domains (from user vision)
- Coding (mecha_agent + workspace)
- HTML generation / preview
- Chat (multi-turn session persistence)
- Organization (workspace timeline + session pool)
- Desktop pet (ui widget + local inference)
- Debugging (tool execution + deterministic validation)
- Emails (structured message / GBNF templates)
- Research (memory + recall + external fallback)
- Aggregating (snapshot comparison + eval aggregation)
- Browser use (local server gateway for web surface)

## Harness vs Fine-Tuning Verdict (from benchmarks)
- Harness quality gap: 15-25%+ improvement
- Model generation gap (same harness): 5-12%
- Fine-tuning: only useful for sub-8B local models needing strict DSL/JSON format offloading
- Harness wins for: stuck-state detection, strict schema/MCP enforcement, context compaction, deterministic verification (compiler/linter loops), sub-agent isolation

## Implementation Priority for roco_ai
1. Strengthen harness: session persistence, workspace sandbox, validation pipeline, grammar strategies (tests/evals filled above)
2. Bind local inference (`inferd`) to unified `local_agent` scaffold
3. Use fine-tuning ONLY if deploying sub-8B quantized models requiring exact output grammar; otherwise rely on harness + grammar engine (`bnf-engine`)
