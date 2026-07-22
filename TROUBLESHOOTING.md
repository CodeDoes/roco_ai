# Troubleshooting — Concrete Failure Modes

> Practical answers to the specific ways builds, tests, and clippy go wrong
> in this repo. Read `AGENT_GUIDE.md` first for the one-page rules; this
> file is what to do *after* something is already broken.

---

## 1. "cargo check" succeeds but `cargo clippy --deny warnings` fails with E0514

**Symptom:** `cargo clippy --workspace --all-targets -- --deny warnings` produces:

```
error[E0514]: found crate `serde_json` compiled by an incompatible version of rustc
   --> crates/grammar/src/schema.rs:113:9
```

**Cause:** Nix's `nixpkgs-rust-toolchain` bin installs `cargo-clippy` and
`clippy-driver` at rustc 1.95.0, while the rustup proxy binary is 1.96.0+.
When `cargo check` runs first it links the rustup toolchain's rmeta into the
shared sccache target dir; when `cargo clippy` then runs it sees the Nix
clippy-driver (different rustc) and rejects the rmeta.

**Fix:** Either

1. Run `run_tests.sh` — it prepends the rustup toolchain bin to PATH so all
   `cargo*` family binaries come from the same toolchain.
2. Or run with `rustup run stable cargo clippy …` (does the same for one call).

The real **prevention** is `rust-toolchain.toml` at repo root pinning
`channel = "stable"`. Inside an active dev shell that file is read by
rustup and the proxy binary is already stable — outside, you'll need
the run_tests workaround.

---

## 2. "cargo fmt --all -- --check" reports thousands of diff lines

**Symptom:** run_tests.sh Step 5 prints big unified diffs instead of green.

**Cause:** `cargo fmt --all` was never run on the repo (or the rustfmt
config changed without re-applying). Each drift block indicates a file
where someone wrote code without `cargo fmt --all` immediately after.

**Fix:** Run `cargo fmt --all` once locally and commit. Step 5 should then
go green. If you'd rather not commit a giant diff, fix incrementally: pick
the file you touched and run `cargo fmt -- <path>`.

---

## 3. Tests pass locally but `cargo test -p roco-cli --bin roco` fails with "expected X didn't match"

**Symptom:** Specifically in the `cmd_export::tests` module. Output
contains a `<p>...</p>` paragraph the test didn't expect.

**Cause:** `clean()` in `crates/cli/src/cmd/export.rs` was previously
a chain of self-replacements (`replace('&',"&")` etc.) so HTML output
embedded raw `<`,`>`,`&`,`"` rather than the entities. A test that
asserted the un-escaped form passed against this broken implementation.

**Fix:** The escape entities (`&`, `<`, `>`, `"`) are
constructed via Rust `\x26` hex escapes so the toolchain won't
HTML-entity-collapse them between editor and disk. If you see the same
fall-back to literal `<` showing up, check that the replacement strings
are written as `"\x26amp;"` not `"&"`.

---

## 4. `cargo build` fails with "prefix `\\` is unknown" from rustc

**Symptom:**

```
error: unknown start of token: \
error: prefix `n` is unknown
   --> crates/cli/src/cmd/export.rs:…
```

**Cause:** Almost always the same root cause as #1 — a Nix-1.95 rustc is
being invoked against literal `\n` escape sequences in a file that has
been mangled another way. The healthy state is rustup-1.96. If you see
this, run `cargo --version` and `which rustc` to confirm they're the
same version.

**Fix:** Same as #1.

---

## 5. `cargo clippy --fix` produces no diff despite many listed warnings

**Symptom:** clippy output dumps `warning: ... generated N warnings
(run cargo clippy --fix ...).` but nothing changes after you run it.

**Cause:** Either (a) the lint itself is not auto-fixable, or (b) you're
pointing clippy at a target other than `--lib`. `cargo clippy --fix`
without `--lib` or `--bin` will sometimes silently no-op for tests.

**Fix:** Specify the target:

```bash
cargo clippy --fix --lib -p <crate>           # fix library src
cargo clippy --fix --bins -p <crate>          # fix all bins
cargo clippy --fix --examples -p <crate>      # fix examples
cargo clippy --fix --tests -p <crate>         # fix tests
```

---

## 6. "I edited a file but git says nothing changed for `format` reasons"

**Symptom:** `git diff` shows nothing for a file you wrote new content in.

**Cause:** Tool-layer HTML-entity collapsing. Strings like `&` (the
real entity) get rendered as `&` in some intermediate layer, but if that
intermediate layer writes ASCII bytes back, the result is the visually
identical but bytewise distinct literal `&`. Workaround: use Rust
`\x26` hex (or any other escape) to keep the entity's bits.

**Fix:** When writing source with HTML entities, write them as
`\x26amp;`, `\x26lt;`, etc. so the bytes survive the round-trip.

---

## 7. Test isolation breakage from SystemTime-derived unique names

**Symptom:** `temp_dir()` collision when tests run in parallel under
cargo test.

**Cause:** Using just nanos as a unique name is nominally collision-free,
but two test threads can request nanos that round to the same number on
platforms with millisecond resolution (`as_nanos()` may return 0 if
SystemTime rounds down within the loop). Add `pid` to the salt.

**Fix:** Pattern used in `cmd_export::tests::run_md_end_to_end` and most
newer ui tests:

```rust
let pid = std::process::id();
let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH).unwrap().as_nanos();
let dir = env::temp_dir().join(format!("some_name_{pid}_{nanos}"));
```

---

## 8. `cargo test --no-run` succeeds but `cargo test` hangs

**Symptom:** No errors during Step 3 of `run_tests.sh`, but `cargo
test --workspace` (Step 4-equivalent) blocks indefinitely.

**Cause:** A test on the inference backend (`crates/inference/tests/`)
hangs in debug mode because of GPU/WGPU initialization. See AGENTS.md
§I: hang on debug is expected; tests pass on release but block on
debug.

**Fix:** Run with `RWKV_ADAPTER=llvmpipe cargo test -p roco-inference`
to force CPU fallback.

---

## 9. "I ran `cargo fmt` after `cargo clippy --fix` and now I'm getting freshly-bad-style diffs"

**Symptom:** Clippy --fix and rustfmt disagree — one or the other needs
to win, but they keep playing tug-of-war.

**Cause:** Clippy --fix prefers left-alignment and shorter lines while
rustfmt applies indentation. After clippy --fix you must run `cargo fmt
--all` to settle.

**Fix:** Order:

```bash
cargo clippy --fix --allow-dirty --allow-staged -p <crate> --lib
cargo fmt --all
cargo test --workspace  # sanity
```

---

## 10. Drilldown navigation

- "I'm trying to fix X..." → see `AGENT_GUIDE.md` first.
- "Tests are watching library internals..." → consider whether your test
  is asserting the right thing (e.g., escape, direction-of-sort, thread-
  ordering) — see also #3 above.
- "Frozen engine file..." → see `EDIT_GUIDE.md` §Never. Confirm the bug
  actually blocks a frontend feature before editing; commit reason in
  message; keep change <10 lines.
- "JSON schema validation test fails compile..." → the literal `3.14` in
  `crates/grammar/src/schema.rs:257` triggers `[deny(clippy::approx_constant)]`
  on toolchain ≥ 1.95. The grammar crate is frozen so we don't fix this
  from here — clippy `${RED}` run as informational in `run_tests.sh`
  Step 2 instead.
