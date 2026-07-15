# Error Recovery

## Intent
Detect and recover from generation failures, timeouts, or parse errors (retry, fallback, or graceful abort).

## What Changed After Grammar-First Architecture
### The Key Insight: Grammar Eliminates Most Error Cases
When model output is **GBNF-constrained**, the following error categories effectively disappear:
- **Malformed JSON / invalid structure** — can't happen; sampler rejects non-conforming tokens
- **Missing fields** — grammar only produces valid structures
- **Type mismatches** — syntax guarantees correct types at decode time
- **Unparseable output** — `serde_json::from_str()` always succeeds on grammar-constrained output

**Rule:** If a stage has a proper BNF grammar, its "error recovery" should be limited to:
1. Model timeout / OOM → retry with smaller max_tokens
2. Grammar engine failure (bnf_sampler crash) → fall back to schoolmarm
3. Empty output due to context window → reduce prompt size, retry

### Legacy Error Recovery Still Needed For:
- Free-form prompting (story pipeline chapters without per-chapter grammar)
- Tool call parsing from unstructured text
- Network timeouts in gateway/server crates
- Tokenizer mismatches between model and grammar vocabulary

### Pattern for Non-Grammar Stages
When no grammar is available yet (temporary state during development):
```
Prompt → generate → detect contamination/heuristics → strip artifacts → validate →
  if valid: return
  if partial: truncate to last complete segment → return best-effort
  if empty: retry with lower temperature (+ stronger anti-think prompt) → fallback threshold
```
The `has_meta_contamination()` heuristic and `strip_think_blocks()` stripper are **interim measures**.
They signal where a proper grammar should be added.
