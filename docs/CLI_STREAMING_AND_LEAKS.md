# `roco` CLI — Streaming, Conversation, Identity, and Resource Audit

Covers the rework of the `roco` CLI (the `scripts.roco` / `scripts.chat`
entry points in `devenv.nix`) plus a resource-leak audit of the code paths
those commands touch.

---

## 1. Streaming

### What was wrong

`interact.rs` and `router.rs` both *looked* like they streamed:

```rust
let streamed = Arc::new(Mutex::new(String::new()));
let streamed_cb = Arc::clone(&streamed);
on_token: Some(Box::new(move |token| {
    if let Ok(mut buf) = streamed_cb.lock() { buf.push_str(token); }
})),
```

The callback appended into a buffer **that was never read**. Setting
`on_token` also flips `RemoteBackend` into SSE mode, so the CLI paid the cost
of streaming and threw the result away — the user watched a `...` spinner for
the whole generation, then the entire answer appeared at once.

`coder.rs` did stream, but printed the response **twice**: once dribbled out
by the callback, then again in full via `println!("\n{text}")` after the call
returned. It also emitted raw tokens with no filtering.

### What it does now

New module [`crates/cli/src/streaming.rs`](../crates/cli/src/streaming.rs).
`StreamPrinter` keeps the raw token stream, re-renders the visible text after
each delta, and writes only the bytes not yet written. It is *monotonic* — the
rendered prefix never shrinks — which is the invariant that makes incremental
printing safe (there is a test asserting exactly this).

It works with three backend shapes without the caller caring:

| Backend behaviour | Handling |
|---|---|
| Token-by-token SSE (`RemoteBackend`) | Renders each delta live |
| One callback with the whole body (`MockBackend`) | Renders once |
| Never calls `on_token` | `finish()` renders the returned text |

While rendering it also:

- hides `<think>…</think>` blocks,
- holds back trailing partial markers, so a half-received `<thi` never flashes,
- cuts at a hallucinated next turn (`\nUser:`) — the most common failure mode
  of a raw completion model in a chat loop,
- buffers a `{"result": …}` `MockBackend` envelope, which is only meaningful
  once complete.

### SSE reassembly bug (`crates/infer-client`)

Separately, the SSE reader called `split('\n')` on **each HTTP chunk
independently**. HTTP chunk boundaries have nothing to do with SSE event
boundaries, so any `data: {...}` line straddling two chunks was parsed as two
invalid fragments and silently dropped — tokens vanished mid-stream.

Fixed with a line buffer carried across chunks. The test splits a two-event
payload at *every* byte offset and asserts nothing is lost; the old code
loses data at **73 of 78** split points.

---

## 2. Natural conversation

### Context was being destroyed to fit

`build_chat_context` kept the last 8 messages but truncated **each one to 300
characters**. Any reply longer than a tweet was mutilated before being fed
back, so the model could not follow up on its own output. This was the single
biggest cause of "it forgot what we were talking about".

New [`crates/cli/src/conversation.rs`](../crates/cli/src/conversation.rs)
(`ChatSession`) includes messages **whole or not at all**: turns are walked
newest-first and admitted while they fit a character budget
(`MAX_CONTEXT_CHARS = 8000`, `MAX_CONTEXT_TURNS = 16`). Turns evicted from the
verbatim window are folded into a bounded rolling summary, so long
conversations degrade gracefully instead of forgetting abruptly.

`ChatSession` is now shared by `interact` (all three modes), and the same
budgeting was applied to `coder.rs`, whose 6-turn × 2048-token history could
overflow the context window outright.

### Other conversation fixes

- **`roco interact --pace rolling` treated `--pace` as the opening prompt.**
  `extra.first()` was used unconditionally. Now parsed positionally
  (`first_positional`), which also fixes the same bug in `roco code --lang rust`.
- **`:lang` in coder mode did nothing.** It printed a confirmation, but the
  system prompt had been built once before the loop. It is now rebuilt.
- **`--list-sessions` started the daemon chain**, waiting ~25s for a model load
  just to read a directory. It now returns immediately.
- **Resume replayed the entire transcript** into the scrollback; now shows the
  last 12 with a count of what was omitted.

---

## 3. Who Am I questions

A 2.9B local model has no idea it is called RoCo, which subcommands exist, or
what the user's name is. Asked "who are you?", it confidently invents an
answer. New [`crates/cli/src/identity.rs`](../crates/cli/src/identity.rs)
handles this in two layers.

**Deterministic fast path.** `identity::detect` recognises identity questions
and answers from real program facts — crate version, live backend name,
the actual command table, the stored profile. No tokens, no latency, no
hallucination. Recognised:

| Category | Examples |
|---|---|
| `WhoAreYou` | "who are you", "introduce yourself", "what's your name" |
| `WhatCanYouDo` | "what can you do", "how can you help" |
| `WhatModel` | "what model are you", "are you GPT-4", "what's under the hood" |
| `WhoAmI` | "who am I", "what do you know about me" |
| `SetName` | "my name is Ada", "call me Ada" |
| `Remember` | "remember that I write sci-fi" |
| `ForgetMe` | "forget me", "clear my profile" |

**Model grounding.** `identity_preamble` injects the same facts into every
system prompt (chat, router, coder), including an explicit instruction never
to claim to be another assistant — so identity raised *mid-sentence* is also
grounded.

Matching is deliberately conservative: a false negative costs one model
round-trip, a false positive hijacks the user's message. "I'm confused" does
not set the user's name to "confused" (there is a stopword guard and a test).

**User memory** is a `UserProfile` at `.roco/profile.json`, written only from
explicit statements — matching the consent requirement in RFC 0010. It is
bounded (64 facts, 280 chars each) and saved via write-to-temp + rename, so an
interrupted write cannot truncate it. A corrupt profile degrades to empty
rather than being fatal.

Available as `roco whoami` (`--json`, `--set-name`, `--forget`) and as
`:whoami` / `:whois` / `:name` / `:remember` / `:forget` inside every REPL.
`roco whoami` deliberately does not start the daemon: asking who you are
should never cost a 25-second model load.

### Bug found while testing this

The name-extraction offset arithmetic computed `raw.len() - normalized_rest.len()`,
but `normalize()` lowercases, collapses whitespace *and* strips trailing
punctuation — so the offset was wrong whenever any of those applied.
`"You can call me Sam."` extracted the name **"am"**. Fixed with a
case-insensitive prefix strip against the raw string, which is also UTF-8 safe
(`get` rather than `split_at`, so a multi-byte char at the boundary returns
`None` instead of panicking).

---

## 4. Resource audit

### Memory

| Issue | Location | Fix |
|---|---|---|
| Conversation history grew forever in a long REPL | `interact.rs` | `MAX_HISTORY_MESSAGES = 400`, oldest folded into a bounded summary |
| Coder history unbounded across `:clear`-less sessions | `cmd/coder.rs` | `trim_history` to 20 messages |
| `SmartCache` bounded entry *count* only | `engine/cache.rs` | Optional byte budget via new `Weighed` trait |
| Fresh `String` allocated per REPL iteration | 3 REPLs | One reusable buffer, `clear()` per turn |

### Cache

`SmartCache` had three defects that made it a slow leak rather than a cache:

1. **It was FIFO, not LRU.** `get` never touched the recency queue, so eviction
   followed insertion order — a hot key inserted early was evicted while cold
   keys inserted later survived, exactly backwards. There is now a test that
   hammers one key through 10 rounds of churn and asserts it survives.
2. **Re-inserting an existing key** never refreshed recency and pushed no queue
   entry, so the map and the queue could disagree on length.
3. **No byte bound and no TTL.** For the intended payload — serialized RWKV
   recurrent states, megabytes each — a 128-entry cache is a multi-gigabyte
   cache. A daemon also kept state for sessions that ended hours ago: live,
   unreachable, and never evicted because never least-recently-used.

Added `with_max_bytes`, `with_ttl`, `purge_expired`, and correct promote-on-access.
A single oversized entry is still retained (evicting to empty would make the
cache useless for large states).

### Storage

| Issue | Fix |
|---|---|
| `.roco/sessions/` grew forever — one JSON per `roco interact` run | `prune_old_sessions`, keep 100 |
| `.roco/agent-journal.md` append-only, written by every command | Rotates at 8 MiB (`$ROCO_JOURNAL_MAX_MB`), keeps 1 generation |
| One runaway entry could add megabytes | Entries clamped to 4000 chars |
| Daemon logs in `/tmp/roco/` never truncated | Rotate at 16 MiB before each spawn |

Session transcripts were also being rewritten in full after *every* turn — with
unbounded history that is quadratic disk I/O, now bounded by the history cap.

### File descriptors / processes

| Issue | Fix |
|---|---|
| `reqwest::Client::new()` with **no timeout at all** | `connect_timeout` 30s; a wedged daemon no longer hangs the REPL forever |
| Idle connections retained indefinitely | `pool_idle_timeout` 90s, `pool_max_idle_per_host` 4 |
| New `Client` (+ pool + DNS resolver) built per health probe | Process-wide `OnceLock` client |
| `std::mem::forget(child)` leaked the handle and its pipe FDs for the parent's lifetime | `drop(child)` — dropping `Child` does not kill the process |
| Zombies reported as running (`/proc/<pid>` exists for zombies) | Parse process state, treat `Z`/`X` as dead |

The zombie check mattered: a crashed daemon looked alive, so `roco` kept
talking to a dead port and never restarted it.

Deliberately **not** added: a total-request timeout. Generation is open-ended
and a long story can legitimately stream for minutes; the per-request deadline
belongs to `CompletionRequest::deadline_ms`, which the server enforces.

---

## 5. Verification

No Rust toolchain or network access was available in the environment where
this was written, so `cargo build` / `cargo test` could not be run. To
compensate, the pure logic was extracted and executed as reference
implementations:

- **Identity detection** — 31 cases (all categories, negatives, UTF-8): pass.
- **Prefix/offset handling** — 11 phrasings all recover `"Ada"`: pass.
  Also reproduced the original `"am"` bug before fixing it.
- **Stream rendering** — 5 table cases + 10 assertions, including the
  monotonicity invariant and multi-byte deltas: pass.
- **SSE reassembly** — split at all 78 byte offsets: 0 failures new,
  73 failures old.
- **LRU / byte budget** — promote-on-access, budgets, oversized entries: pass.
- **Context budgeting** — verbatim retention, char budget, summary bounds: pass.

Balanced-delimiter checks were run across all 13 touched files. **`cargo test
-p roco-cli -p roco-app -p roco-engine -p roco-infer-client` and `cargo clippy
--workspace --all-targets -- --deny warnings` still need to be run** on a
machine with the toolchain.

### CI is broken — root-caused, but the fix needs a maintainer to apply

CI was expected to provide the first real compile. It does not run at all:
every workflow run in this repository — on `main`, going back well before this
branch — completes in seconds with `total_count: 0` jobs.

**Root cause: `.github/workflows/ci.yml` is not valid YAML.** Actions reports

```
Invalid workflow file — You have an error in your yaml syntax on line 56
```

Line 56 is an unquoted scalar containing `": "`:

```yaml
      - name: Install system deps (egui: libgl, libxcb, …)
```

The `egui: ` is parsed as the start of a nested mapping in a scalar context —
a hard parse error (`mapping values are not allowed here`, line 56 column 40).
Every other multi-word `name:` in the file is quoted; this one is not. Because
the file never parses, **no jobs are ever created**, which is why the run
"fails" instantly with an empty job list and no logs to inspect.

#### The fix (two lines)

1. Quote line 56:

   ```yaml
   - name: "Install system deps (egui: libgl, libxcb, …)"
   ```

2. Then `test-core` will start running — and immediately fail, because it
   names packages that do not exist:

   ```yaml
   run: cargo test -p roco-agent-core -p roco-agent-story -p roco-engine
   ```

   Neither `roco-agent-core` nor `roco-agent-story` is a workspace member; the
   agent crate is a single `roco-agent`. Suggested replacement, which also
   covers the crates this branch touches:

   ```yaml
   - name: "cargo test — agent + engine + app + infer-client + cli"
     env:
       ROCO_USE_MOCK_BACKEND: "1"
     run: |
       cargo test --no-fail-fast \
         -p roco-agent -p roco-engine -p roco-app \
         -p roco-infer-client -p roco-cli
   ```

Both changes are prepared and verified locally (the corrected file parses, and
all 7 jobs materialise), but **could not be pushed**: the GitHub App backing
this session lacks the `workflows` permission —

```
refusing to allow a GitHub App to create or update workflow
`.github/workflows/ci.yml` without `workflows` permission
```

So a maintainer needs to apply them. Until then the local
reference-implementation results above are the strongest available evidence,
and cargo should be run directly.

Roughly 90 tests were added across the touched crates. Notable regression
guards:

- `visible_text_is_monotonic_across_deltas` — the incremental printer's core invariant
- `events_split_across_chunk_boundaries_are_not_dropped` — the SSE bug
- `get_promotes_so_hot_keys_survive` — the FIFO-masquerading-as-LRU bug
- `context_keeps_whole_messages_not_truncated_ones` — the 300-char mutilation
- `does_not_mistake_feelings_for_names` — identity false positives
- `is_pid_alive_reports_zombies_as_dead` — spawns a real zombie
- `journal_rotates_instead_of_growing_forever`
- `prune_removes_only_the_oldest_beyond_the_cap`

One pre-existing broken test was fixed: `test_cli_help_subcommands` asserted
the help output contains `"Subcommands:"`, which the CLI has never printed
(it prints `"Commands:"`).
