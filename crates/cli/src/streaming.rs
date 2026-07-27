//! Incremental (streaming) terminal rendering for model output.
//!
//! Before this module the CLI *claimed* to stream: `interact.rs` and
//! `router.rs` both attached an `on_token` callback that appended every token
//! into an `Arc<Mutex<String>>` which was then **never read**. The user stared
//! at a `...` spinner for the whole generation and the full response appeared
//! at once. This module makes the token callback actually drive the terminal.
//!
//! [`StreamPrinter`] is deliberately *idempotent and monotonic*: it keeps the
//! raw token stream, re-renders the visible text after each delta, and writes
//! only the bytes it has not written yet. That makes it safe to use with
//! backends that
//!
//! - stream token-by-token (`RemoteBackend` over SSE),
//! - emit one giant callback with the whole body (`MockBackend`),
//! - or never call `on_token` at all (any non-streaming backend) — in which
//!   case [`StreamPrinter::finish`] renders the final text instead.
//!
//! While rendering it also:
//!
//! - hides `<think>…</think>` reasoning blocks from the transcript,
//! - holds back a trailing partial marker so a half-received `<thi` never
//!   flashes on screen,
//! - cuts the response at a hallucinated next turn (`\nUser:`), which is the
//!   single most common failure mode of a raw completion model in a chat loop,
//! - and buffers (rather than dribbles out) a `{"result": …}` envelope from
//!   `MockBackend`, which is only meaningful once complete.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use crate::rich_output as r;

// ═════════════════════════════════════════════════════════════════════════════
// Markers
// ═════════════════════════════════════════════════════════════════════════════

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

/// Sequences meaning "the model started writing the *next* turn for us".
/// Everything from here on is hallucinated dialogue and must be discarded.
pub const STOP_SEQUENCES: &[&str] = &["\nUser:", "\nUSER:", "\nHuman:", "\nuser:"];

/// Markers we must never emit a partial prefix of.
const HOLDBACK_MARKERS: &[&str] = &[
    THINK_OPEN,
    THINK_CLOSE,
    "\nUser:",
    "\nUSER:",
    "\nHuman:",
    "\nuser:",
    "\nAssistant:",
];

// ═════════════════════════════════════════════════════════════════════════════
// Pure rendering helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Length of the longest suffix of `s` that is a *proper* prefix of `marker`.
fn partial_suffix_len(s: &str, marker: &str) -> usize {
    let max = marker.len().saturating_sub(1).min(s.len());
    for len in (1..=max).rev() {
        // Only consider char-boundary splits so we never slice mid-UTF-8.
        let Some(tail) = s.get(s.len() - len..) else {
            continue;
        };
        if marker.starts_with(tail) {
            return len;
        }
    }
    0
}

/// Trim any trailing partial marker so a half-arrived `</thi` is not printed.
fn hold_back_partial(mut s: String) -> String {
    let mut cut = 0;
    for marker in HOLDBACK_MARKERS {
        cut = cut.max(partial_suffix_len(&s, marker));
    }
    if cut > 0 {
        let keep = s.len() - cut;
        if s.is_char_boundary(keep) {
            s.truncate(keep);
        }
    }
    s
}

/// Remove `<think>…</think>` blocks. An unterminated block swallows the rest.
fn strip_think(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    loop {
        match rest.find(THINK_OPEN) {
            Some(i) => {
                out.push_str(&rest[..i]);
                let after = &rest[i + THINK_OPEN.len()..];
                match after.find(THINK_CLOSE) {
                    Some(j) => rest = &after[j + THINK_CLOSE.len()..],
                    // Still inside an open think block — hide the remainder.
                    None => return out,
                }
            }
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

/// Cut at the earliest stop sequence. Returns `(text, hit_a_stop)`.
fn cut_at_stop(s: &str) -> (&str, bool) {
    let mut best: Option<usize> = None;
    for seq in STOP_SEQUENCES {
        if let Some(i) = s.find(seq) {
            best = Some(best.map_or(i, |b: usize| b.min(i)));
        }
    }
    match best {
        Some(i) => (&s[..i], true),
        None => (s, false),
    }
}

/// Does this look like a `MockBackend` JSON envelope rather than prose?
fn looks_like_json_envelope(raw: &str) -> bool {
    raw.trim_start().starts_with('{')
}

// ═════════════════════════════════════════════════════════════════════════════
// StreamPrinter
// ═════════════════════════════════════════════════════════════════════════════

/// Renders a token stream to the terminal, incrementally and exactly once.
pub struct StreamPrinter {
    prefix: String,
    raw: String,
    /// Byte count of the rendered text already written to stdout.
    printed: usize,
    started: bool,
    hit_stop: bool,
    quiet: bool,
}

impl StreamPrinter {
    /// A printer that writes to stdout, emitting `prefix` before the first token.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            raw: String::new(),
            printed: 0,
            started: false,
            hit_stop: false,
            quiet: false,
        }
    }

    /// A printer that renders but never writes — for one-shot / scripted calls.
    pub fn quiet() -> Self {
        Self {
            prefix: String::new(),
            raw: String::new(),
            printed: 0,
            started: false,
            hit_stop: false,
            quiet: true,
        }
    }

    /// Wrap in the `Arc<Mutex<…>>` an `on_token` callback needs.
    pub fn shared(self) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(self))
    }

    /// Whether anything has been written to the terminal yet.
    pub fn has_output(&self) -> bool {
        self.started
    }

    /// Feed one streamed delta.
    pub fn push(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        self.raw.push_str(delta);
        self.flush(false);
    }

    /// Finish the stream. `full_text` is the backend's final text, used when
    /// the backend never streamed (or streamed a strict prefix of the result).
    /// Returns the cleaned text suitable for the saved transcript.
    pub fn finish(&mut self, full_text: &str) -> String {
        if self.raw.is_empty() {
            self.raw.push_str(full_text);
        } else if full_text.len() > self.raw.len() && full_text.starts_with(&self.raw) {
            // Backend streamed a prefix then returned the whole body.
            self.raw = full_text.to_string();
        }

        self.flush(true);

        if self.started && !self.quiet {
            println!();
            io::stdout().flush().ok();
        }

        let rendered = self.render(true);
        if rendered.trim().is_empty() {
            // Nothing survived rendering (e.g. a pure think block). Fall back
            // to the backend's own text so the transcript is never empty.
            r::clean_response(full_text)
        } else {
            rendered
        }
    }

    /// Render the currently-visible text without touching the terminal.
    pub fn rendered(&self) -> String {
        self.render(true)
    }

    // ── internals ────────────────────────────────────────────────────────

    fn render(&self, finished: bool) -> String {
        if self.raw.trim().is_empty() {
            return String::new();
        }
        // A JSON envelope only becomes meaningful once it is complete.
        if looks_like_json_envelope(&self.raw) {
            return if finished {
                r::clean_response(&self.raw)
            } else {
                String::new()
            };
        }

        let stripped = strip_think(&self.raw);
        let (cut, _) = cut_at_stop(&stripped);
        let cut = cut.to_string();

        if finished {
            cut.trim().to_string()
        } else {
            hold_back_partial(cut).trim_start().to_string()
        }
    }

    fn flush(&mut self, finished: bool) {
        if self.hit_stop && !finished {
            return;
        }
        let visible = self.render(finished);

        // Sticky: once the model starts a hallucinated turn nothing more is
        // ever shown, even if more tokens keep arriving.
        if !self.hit_stop {
            let stripped = strip_think(&self.raw);
            self.hit_stop = cut_at_stop(&stripped).1;
        }

        if visible.len() <= self.printed {
            return;
        }
        let Some(new) = visible.get(self.printed..) else {
            // Defensive: never slice mid-char, just wait for more input.
            return;
        };
        if new.is_empty() {
            return;
        }

        if !self.quiet {
            if !self.started {
                // Erase the "thinking" hint before the first real output.
                print!("\r\x1b[K{}", self.prefix);
            }
            print!("{new}");
            io::stdout().flush().ok();
        }
        self.started = true;
        self.printed = visible.len();
    }
}

/// Print the transient "waiting for the first token" hint.
///
/// [`StreamPrinter`] erases it (`\r\x1b[K`) as soon as it emits anything, so
/// the hint never survives into the transcript.
pub fn thinking_hint() {
    print!(
        "{}{}  ...{}\r",
        r::Colors::DIM,
        r::Colors::CYAN,
        r::Colors::RESET
    );
    io::stdout().flush().ok();
}

/// Erase the current terminal line (used when a turn fails before any output).
pub fn clear_line() {
    print!("\r\x1b[K");
    io::stdout().flush().ok();
}

/// Build an `on_token` callback that drives `printer`.
pub fn on_token_for(printer: &Arc<Mutex<StreamPrinter>>) -> Box<dyn Fn(&str) + Send + Sync> {
    let printer = Arc::clone(printer);
    Box::new(move |token: &str| {
        if let Ok(mut p) = printer.lock() {
            p.push(token);
        }
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet_stream(chunks: &[&str]) -> String {
        let mut p = StreamPrinter::quiet();
        for c in chunks {
            p.push(c);
        }
        let full: String = chunks.concat();
        p.finish(&full)
    }

    #[test]
    fn plain_text_streams_through_unchanged() {
        assert_eq!(quiet_stream(&["Hello", ", ", "world!"]), "Hello, world!");
    }

    #[test]
    fn think_block_is_hidden() {
        assert_eq!(
            quiet_stream(&["<think>", "secret plan", "</think>", "Answer."]),
            "Answer."
        );
    }

    #[test]
    fn unterminated_think_block_hides_everything_after_it() {
        assert_eq!(quiet_stream(&["Hi. <think>", "still musing"]), "Hi.");
    }

    #[test]
    fn hallucinated_next_turn_is_cut() {
        assert_eq!(
            quiet_stream(&["The answer is 4.", "\nUser: and 3+3?", "\nAssistant: 6"]),
            "The answer is 4."
        );
    }

    #[test]
    fn mock_json_envelope_is_unwrapped_not_dribbled() {
        let out = quiet_stream(&[r#"{"result": "[mock-3b] hello there"}"#]);
        assert!(!out.contains('{'), "envelope should be unwrapped: {out}");
        assert!(out.contains("hello there"), "got {out}");
    }

    #[test]
    fn partial_marker_is_held_back_until_resolved() {
        let mut p = StreamPrinter::quiet();
        p.push("Done");
        assert_eq!(p.rendered(), "Done");
        // "<thi" is a partial "<think>" — must not be visible yet.
        p.push("<thi");
        let mid = p.render(false);
        assert_eq!(mid, "Done");
        p.push("nk>hidden</think> Fin.");
        assert_eq!(p.finish("Done<think>hidden</think> Fin."), "Done Fin.");
    }

    #[test]
    fn non_streaming_backend_still_renders_via_finish() {
        let mut p = StreamPrinter::quiet();
        assert_eq!(p.finish("<think>x</think>Final answer."), "Final answer.");
    }

    #[test]
    fn streamed_prefix_is_upgraded_to_full_text() {
        let mut p = StreamPrinter::quiet();
        p.push("Par");
        assert_eq!(p.finish("Partial then whole"), "Partial then whole");
    }

    #[test]
    fn visible_text_is_monotonic_across_deltas() {
        // The printer relies on the rendered text only ever growing.
        let deltas = ["He", "llo\n", "wor", "ld <thi", "nk>z</think>", "!\nUser: x"];
        let mut p = StreamPrinter::quiet();
        let mut prev = String::new();
        for d in deltas {
            p.push(d);
            let now = p.render(false);
            assert!(
                now.starts_with(&prev),
                "render shrank: {prev:?} -> {now:?} after {d:?}"
            );
            prev = now;
        }
    }

    #[test]
    fn empty_and_whitespace_only_streams_are_safe() {
        let mut p = StreamPrinter::quiet();
        p.push("");
        p.push("   ");
        // Falls back to clean_response of the final text.
        assert_eq!(p.finish("   ").trim(), "");
    }

    #[test]
    fn multibyte_deltas_never_panic() {
        let out = quiet_stream(&["héllo ", "wörld ", "—", " 🎉"]);
        assert!(out.contains("🎉"), "got {out}");
    }

    #[test]
    fn partial_suffix_len_finds_longest_prefix() {
        assert_eq!(partial_suffix_len("abc<thi", THINK_OPEN), 4);
        assert_eq!(partial_suffix_len("abc", THINK_OPEN), 0);
        // A complete marker is not a *partial* suffix.
        assert_eq!(partial_suffix_len("<think>", THINK_OPEN), 0);
    }

    #[test]
    fn cut_at_stop_takes_the_earliest_match() {
        let (cut, hit) = cut_at_stop("a\nHuman: x\nUser: y");
        assert!(hit);
        assert_eq!(cut, "a");
    }
}
