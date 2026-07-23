# RFC 0001: Local AI Harness Architecture
Status: Draft
Author: roco_ai framework

## Summary
A harness-first architecture separates execution environment from model weights. The harness manages context, tool sandbox, deterministic verifiers, rollback loops, and state tracking. Fine-tuning is reserved for sub-8B models requiring strict DSL output.

## Motivation
Remote APIs are expensive and unreliable. A local harness running RWKV via roco-inferd enables offline autonomous agents for coding, writing, research, automation, and creative work.

## Design
- DomainHarness trait: name, init, run, verify, rollback
- MockBackend: format string generator (swap for RWKV backend)
- ExecutionLoop: retry with rollback tracking
- Sandbox: file boundary enforcement
- Verifier: deterministic output checks
