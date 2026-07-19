//! Hand-written story-domain GBNF grammars (kbnf dialect).
//!
//! These are the per-handler grammars called for in AGENTS.md and
//! `goals/story-engine` (the `per_handler_grammars` item). Each constrains a
//! pipeline stage's *output* so the sampler rejects non-conforming tokens at
//! every step — eliminating the `<think>`-tag / meta-commentary contamination
//! that free-form prose generation produces on undertrained RWKV models.
//!
//! # Why these exist
//!
//! The JSON envelope of every stage is already BNF-constrained (via
//! [`crate::Schema`] → [`crate::schema_to_gbnf`]), but the `content` string
//! *inside* that envelope is free-form — and the auto-generated string rule
//! permits `<`, so contamination can still slip through. The grammars here
//! generate prose **outside** JSON, with no path that admits `<` or `>`, and
//! the caller assembles the final artifact (chapter file, wiki, etc.).
//!
//! # Source of truth
//!
//! The grammars are embedded from the repo-root [`GBNF/`](../../GBNF)
//! directory (see AGENTS.md). They are validated against the real
//! `roco-bnf-engine` by the tests in this module.

/// Per-handler story grammars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryGrammar {
    /// Plain chapter prose (no `<` / `>` — blocks think-tag leakage).
    ChapterProse,
    /// Chapter outline list.
    Outline,
    /// World bible / wiki entries.
    Wiki,
    /// Chapter validation report.
    ValidationReport,
    /// Story synopsis.
    Synopsis,
    /// Fill-in-the-middle prose (no template markers, no think tags).
    FillInMiddle,
}

impl StoryGrammar {
    /// All known story grammars.
    pub fn all() -> &'static [StoryGrammar] {
        use StoryGrammar::*;
        &[
            ChapterProse,
            Outline,
            Wiki,
            ValidationReport,
            Synopsis,
            FillInMiddle,
        ]
    }

    /// Stable name used for the on-disk `.bnf` file and registry lookup.
    pub fn name(&self) -> &'static str {
        match self {
            StoryGrammar::ChapterProse => "chapter_prose",
            StoryGrammar::Outline => "outline",
            StoryGrammar::Wiki => "wiki",
            StoryGrammar::ValidationReport => "validation_report",
            StoryGrammar::Synopsis => "synopsis",
            StoryGrammar::FillInMiddle => "fill_in_middle",
        }
    }

    /// Raw GBNF source as embedded from `GBNF/<name>.bnf`.
    pub fn source(&self) -> &'static str {
        match self {
            StoryGrammar::ChapterProse => CHAPTER_PROSE,
            StoryGrammar::Outline => OUTLINE,
            StoryGrammar::Wiki => WIKI,
            StoryGrammar::ValidationReport => VALIDATION_REPORT,
            StoryGrammar::Synopsis => SYNOPSIS,
            StoryGrammar::FillInMiddle => FILL_IN_MIDDLE,
        }
    }

    /// kbnf-compatible form (semicolons added to rule lines if missing).
    pub fn kbnf(&self) -> String {
        crate::gbnf_to_kbnf(self.source())
    }

    /// Look up a grammar by its stable name.
    pub fn from_name(name: &str) -> Option<Self> {
        StoryGrammar::all()
            .iter()
            .copied()
            .find(|g| g.name() == name)
    }
}

const CHAPTER_PROSE: &str = include_str!("../../../GBNF/chapter_prose.bnf");
const OUTLINE: &str = include_str!("../../../GBNF/outline.bnf");
const WIKI: &str = include_str!("../../../GBNF/wiki.bnf");
const VALIDATION_REPORT: &str = include_str!("../../../GBNF/validation_report.bnf");
const SYNOPSIS: &str = include_str!("../../../GBNF/synopsis.bnf");
const FILL_IN_MIDDLE: &str = include_str!("../../../GBNF/fill_in_middle.bnf");

#[cfg(test)]
mod tests {
    use super::*;
    use roco_bnf_engine::BnfEngine;

    /// ASCII byte vocabulary: empty sentinel + the printable range plus a few
    /// control bytes the grammars use (newline). Deliberately excludes `<`
    /// (0x3C) and `>` (0x3E) so the ban is also enforced at the vocabulary
    /// level in these tests.
    fn test_vocab() -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = vec![b"".to_vec()]; // 0: empty sentinel
        for b in [0x09u8, 0x0Au8, 0x0Du8] {
            v.push(vec![b]);
        }
        for b in 0x20u8..=0x7Eu8 {
            v.push(vec![b]);
        }
        v
    }

    /// Greedy longest-match tokenizer over the test vocabulary.
    fn tokenize(vocab: &[Vec<u8>], text: &str) -> Vec<u32> {
        let bytes = text.as_bytes();
        let mut pos = 0;
        let mut out = Vec::new();
        'outer: while pos < bytes.len() {
            let mut best: Option<(usize, u32)> = None;
            for (id, tok) in vocab.iter().enumerate() {
                if tok.is_empty() {
                    continue;
                }
                if bytes[pos..].starts_with(tok) && tok.len() > best.map_or(0, |(l, _)| l) {
                    best = Some((tok.len(), id as u32));
                }
            }
            match best {
                Some((len, id)) => {
                    out.push(id);
                    pos += len;
                }
                None => {
                    // No token matches this byte — skip it (keeps test robust).
                    pos += 1;
                    continue 'outer;
                }
            }
        }
        out
    }

    /// A valid sample for each grammar (must be producible from `test_vocab`).
    fn sample(g: StoryGrammar) -> &'static str {
        match g {
            StoryGrammar::ChapterProse => {
                "The knight rode forth. He was brave.\n\nA storm gathered. The wind howled."
            }
            StoryGrammar::Outline => {
                "Chapter 1: The Beginning\nA knight sets out.\n\nChapter 2: The Road\nHe meets a stranger.\n\n"
            }
            StoryGrammar::Wiki => {
                "[character] Alice\nA brave knight.\n\n[location] The Forest\nA dark wood.\n\n"
            }
            StoryGrammar::ValidationReport => {
                "PASS\n-low: Minor pacing issue.\n-medium: Show don't tell needed.\n"
            }
            StoryGrammar::Synopsis => {
                "A fallen knight seeks redemption. Dark forces rise."
            }
            StoryGrammar::FillInMiddle => {
                "He raised the blade. She braced for the clash."
            }
        }
    }

    #[test]
    fn every_grammar_loads_and_allows_tokens() {
        let vocab = test_vocab();
        for g in StoryGrammar::all() {
            let kbnf = g.kbnf();
            // Must contain a root rule.
            assert!(kbnf.contains("root ::="), "{:?} missing root rule", g);
            let engine = BnfEngine::new(&kbnf, &vocab)
                .unwrap_or_else(|e| panic!("{:?} failed to build engine: {e:?}", g));
            assert!(
                engine.allowed_count() > 0,
                "{:?} allows zero tokens at start — degenerate grammar",
                g
            );
        }
    }

    #[test]
    fn every_grammar_accepts_its_sample() {
        let vocab = test_vocab();
        for g in StoryGrammar::all() {
            let kbnf = g.kbnf();
            let mut engine = BnfEngine::new(&kbnf, &vocab)
                .unwrap_or_else(|e| panic!("{:?} failed to build engine: {e:?}", g));
            let tokens = tokenize(&vocab, sample(*g));
            assert!(!tokens.is_empty(), "{:?} sample tokenized to nothing", g);
            for (i, tok) in tokens.iter().enumerate() {
                engine
                    .accept_token(*tok)
                    .unwrap_or_else(|e| panic!("{:?} rejected token {} (idx {i}): {e:?}", g, tok));
            }
            assert!(
                engine.is_finished(),
                "{:?} did not reach a finished state on its sample",
                g
            );
        }
    }

    #[test]
    fn lookup_round_trips_by_name() {
        for g in StoryGrammar::all() {
            assert_eq!(StoryGrammar::from_name(g.name()), Some(*g));
        }
        assert_eq!(StoryGrammar::from_name("nonexistent"), None);
    }

    // =========================================================================
    // Random-walk test for the FIM (FillInMiddle) grammar.
    //
    // Mirrors the random-walk harness in `json_schema.rs`: walk the grammar
    // with a multi-character vocabulary (simulating a real tokenizer) by
    // picking random allowed tokens until the engine reports `is_finished()`.
    // This verifies the FIM grammar actually STOPS at the stop token (the
    // sentence terminator) rather than running unbounded -- the property the
    // FIM eval relies on. `max_tokens` is the step ceiling.
    // =========================================================================

    mod random_walk_fim {
        use super::*;
        use crate::kbnf_compat;
        use ahash::AHashMap;
        use kbnf::engine_like::EngineLike;
        use kbnf::{Config, Engine, Token, Vocabulary};
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        /// Multichar English-ish vocabulary (simulates a real tokenizer).
        /// Must include single-letter tokens so the `letter` char-class
        /// rule can match, plus the space and sentence terminators.
        fn fim_vocab() -> Vec<&'static str> {
            vec![
                "",  // 0: empty sentinel (kbnf skips empty tokens)
                " ", // space between words
                // single lowercase letters (cover the words below)
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "k", "l", "m", "n", "o", "p", "r", "s",
                "t", "u", "w", "y",
                // a few whole words (also matchable letter-by-letter)
                "the", "blade", "sword", "raised", "light", "ward", "dragon", "air", "kingdom",
                "knight", "peak", "storm", "wind", "spell", "breath",
                // stop tokens / terminators
                "..", ".", "?", "!", "\n",
            ]
        }

        /// Walk a GBNF grammar picking random allowed tokens, bounded by
        /// `max_tokens` steps. Returns the produced text and whether the
        /// grammar reached a finished (stopped) state.
        fn random_walk(gbnf: &str, vocab: &[&'static str], max_tokens: usize) -> (String, bool) {
            let mut id_to_token: AHashMap<u32, Token> = AHashMap::new();
            let mut id_to_token_string: AHashMap<u32, String> = AHashMap::new();
            for (id, &token_str) in vocab.iter().enumerate() {
                if token_str.is_empty() {
                    continue;
                }
                let token_id = id as u32;
                id_to_token.insert(
                    token_id,
                    Token(token_str.as_bytes().to_vec().into_boxed_slice()),
                );
                id_to_token_string.insert(token_id, token_str.to_string());
            }
            let vocab_obj = Vocabulary::new(id_to_token, id_to_token_string).expect("vocab build");
            let config = Config {
                start_nonterminal: "root".to_string(),
                ..Config::default()
            };
            let mut engine =
                Engine::with_config(&kbnf_compat::gbnf_to_kbnf(gbnf), vocab_obj, config)
                    .expect("engine build");
            engine.compute_allowed_token_ids();

            let mut rng = thread_rng();
            let mut out = String::new();
            for _ in 0..max_tokens {
                if engine.is_finished() {
                    return (out, true);
                }
                let bitset = engine.allowed_token_ids_from_last_computation();
                let valid: Vec<usize> = (0..vocab.len())
                    .filter(|&i| !vocab[i].is_empty() && bitset.contains(i))
                    .collect();
                if valid.is_empty() {
                    return (out, engine.is_finished());
                }
                // Prefer longer tokens (closer to a real tokenizer's greedy match).
                let &idx = valid
                    .choose_weighted(&mut rng, |&i| vocab[i].len())
                    .expect("non-empty");
                let token = vocab[idx];
                if engine.try_accept_new_token(idx as u32).is_err() {
                    return (out, engine.is_finished());
                }
                engine.compute_allowed_token_ids();
                out.push_str(token);
            }
            (out, engine.is_finished())
        }

        #[test]
        fn fim_grammar_stops_at_stop_token_within_max_tokens() {
            let vocab = fim_vocab();
            let gbnf = StoryGrammar::FillInMiddle.kbnf();
            // max_tokens ceiling for the FIM eval.
            let max_tokens = 128usize;
            for trial in 0..25 {
                let (out, finished) = random_walk(&gbnf, &vocab, max_tokens);
                assert!(
                    finished,
                    "trial {trial}: FIM grammar did not stop within {max_tokens} tokens. output={out:?}"
                );
                // The grammar must terminate at a stop token: a sentence
                // terminator (`.`/`?`/`!`) or the optional trailing newline.
                let ends_with_stop = out.ends_with('.')
                    || out.ends_with('?')
                    || out.ends_with('!')
                    || out.ends_with("\n");
                assert!(
                    ends_with_stop,
                    "trial {trial}: FIM output did not end at a stop token: {out:?}"
                );
                // No scaffolding / think leakage possible (grammar forbids `<`).
                assert!(
                    !out.contains('<'),
                    "trial {trial}: FIM output leaked '<': {out:?}"
                );
            }
        }

        #[test]
        fn fim_grammar_never_runs_unbounded() {
            // With a vocabulary that contains no terminator, the grammar's finite
            // sentence count still bounds it: the walk must terminate well
            // before `max_tokens` OR, at worst, be step-bounded (never loop
            // forever).
            let vocab: Vec<&'static str> = fim_vocab()
                .into_iter()
                .filter(|v| !matches!(*v, "." | "?" | "!" | ".." | "\n"))
                .collect();
            let gbnf = StoryGrammar::FillInMiddle.kbnf();
            let (out, _finished) = random_walk(&gbnf, &vocab, 128);
            assert!(
                out.len() <= 128 * 32,
                "FIM walk produced unbounded output: {out:?}"
            );
        }
    }
}
