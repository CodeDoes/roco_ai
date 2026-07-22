//! Token-level BNF grammar engine wrapping kbnf.
//!
//! This crate isolates `kbnf` behind a simple API so its complex generic
//! types (specifically `string-interner`'s recursive `StringInterner`) never
//! enter the same compilation unit as `web-rwkv`'s `TokioRuntime`, which
//! would trigger `error[E0275]` (type recursion overflow).
//!
//! Usage:
//! ```ignore
//! use roco_bnf_engine::BnfEngine;
//! use roco_engine::BnfMask;
//!
//! let engine = BnfEngine::new(grammar, &vocab_bytes)?;
//! // As a trait object:
//! let mask: Box<dyn BnfMask> = Box::new(engine);
//! ```

use ahash::AHashMap;
use kbnf::{
    engine_like::AcceptTokenError, AcceptTokenResult, Config, Engine, EngineLike, Token, Vocabulary,
};
use roco_engine::BnfMask;

/// Create a `Box<dyn BnfMask>` from a kbnf-format GBNF grammar and vocabulary bytes.
///
/// This is the recommended entry point for application code that needs to pass
/// a grammar constraint to the inference engine. The returned `BnfMask` is
/// opaque and contains no kbnf types visible to the caller.
///
/// # Errors
/// Returns `BnfError` if the grammar string is malformed or the vocabulary is
/// incompatible.
pub fn create_bnf_mask(
    grammar: &str,
    vocab_bytes: &[Vec<u8>],
) -> Result<Box<dyn BnfMask>, BnfError> {
    BnfEngine::new(grammar, vocab_bytes).map(|e| Box::new(e) as Box<dyn BnfMask>)
}

/// Error type for BNF engine operations.
#[derive(Debug, thiserror::Error)]
pub enum BnfError {
    #[error("kbnf vocabulary error: {0}")]
    Vocab(String),
    #[error("kbnf engine init error: {0}")]
    Init(String),
    #[error("kbnf runtime error: {0}")]
    Runtime(String),
}

/// Token-level BNF grammar engine.
///
/// Wraps `kbnf::Engine` and exposes only the API needed for inference:
/// masking logits, accepting tokens, and resetting.
pub struct BnfEngine {
    engine: Engine,
}

impl BnfEngine {
    /// Default start rule name used in schema-generated GBNF grammars.
    pub const DEFAULT_START: &'static str = "root";

    /// Create a new BNF engine from a grammar string and vocabulary.
    ///
    /// Uses `"root"` as the start nonterminal (matching `schema_to_gbnf()`'s
    /// convention). Use [`with_config`](Self::with_config) for custom settings.
    pub fn new(grammar: &str, vocab: &[Vec<u8>]) -> Result<Self, BnfError> {
        let config = Config {
            start_nonterminal: Self::DEFAULT_START.to_string(),
            ..Config::default()
        };
        Self::with_config(grammar, vocab, config)
    }

    /// Create a new BNF engine with a custom kbnf config.
    ///
    /// Allows setting `start_nonterminal` (default `"start"`) and other
    /// kbnf-level options.
    pub fn with_config(grammar: &str, vocab: &[Vec<u8>], config: Config) -> Result<Self, BnfError> {
        let id_to_token: AHashMap<u32, Token> = vocab
            .iter()
            .enumerate()
            .filter(|(_, bytes)| !bytes.is_empty())
            .map(|(id, bytes)| (id as u32, Token(bytes.clone().into_boxed_slice())))
            .collect();

        let id_to_token_string: AHashMap<u32, String> = vocab
            .iter()
            .enumerate()
            .filter(|(_, bytes)| !bytes.is_empty())
            .map(|(id, bytes)| (id as u32, String::from_utf8_lossy(bytes).into_owned()))
            .collect();

        let vocab_obj = Vocabulary::new(id_to_token, id_to_token_string)
            .map_err(|e| BnfError::Vocab(format!("{e:?}")))?;

        let mut engine = Engine::with_config(grammar, vocab_obj, config)
            .map_err(|e| BnfError::Init(format!("{e:?}")))?;
        // kbnf initializes Earley sets in reset() but does NOT populate
        // `allowed_token_ids` — we must do that before the first mask_logits.
        engine.compute_allowed_token_ids();

        let allowed = engine
            .allowed_token_ids_from_last_computation()
            .count_ones(..);
        if allowed == 0 {
            eprintln!("[bnf-engine WARN] compute_allowed_token_ids returned 0 allowed tokens");
        } else if allowed < 10 {
            let first_ones: Vec<usize> = engine
                .allowed_token_ids_from_last_computation()
                .ones()
                .take(5)
                .collect();
            eprintln!(
                "[bnf-engine] allowed={}: first few token_ids={:?}",
                allowed, first_ones
            );
        } else {
            eprintln!("[bnf-engine] allowed={} tokens", allowed);
        }

        Ok(Self { engine })
    }

    /// The vocabulary size reported by kbnf.
    pub fn vocab_size(&self) -> usize {
        self.engine.vocab().vocab_size()
    }

    /// Mask disallowed logits to `f32::NEG_INFINITY`.
    ///
    /// `logits` must have length >= `vocab_size()`. Only the first
    /// `vocab_size()` elements are modified.
    pub fn mask_logits(&self, logits: &mut [f32]) -> Result<(), BnfError> {
        let size = self.vocab_size();
        if logits.len() < size {
            return Err(BnfError::Runtime(format!(
                "logits too short: {} < {}",
                logits.len(),
                size
            )));
        }
        self.engine
            .mask_logits(&mut logits[..size])
            .map_err(|e| BnfError::Runtime(format!("mask_logits: {e:?}")))?;
        Ok(())
    }

    /// Accept a token and advance the grammar state.
    ///
    /// Returns `true` if the grammar is still active, `false` if it's finished
    /// (no more tokens expected).
    pub fn accept_token(&mut self, token: u32) -> Result<bool, BnfError> {
        let finished = match self.engine.try_accept_new_token(token) {
            Ok(AcceptTokenResult::Finished) | Err(AcceptTokenError::Finished) => true,
            Ok(AcceptTokenResult::Ongoing) => false,
            Err(e) => {
                return Err(BnfError::Runtime(format!("accept_token({token}): {e:?}")));
            }
        };
        self.engine.compute_allowed_token_ids();
        Ok(!finished)
    }

    /// Reset the engine to its initial state.
    pub fn reset(&mut self) {
        self.engine.reset();
        self.engine.compute_allowed_token_ids();
    }

    /// Check if the grammar has been fully satisfied.
    pub fn is_finished(&self) -> bool {
        self.engine.is_finished()
    }

    /// Number of tokens allowed by the grammar in its current state.
    /// Returns 0 if no tokens are allowed (grammar is blocked or finished).
    pub fn allowed_count(&self) -> usize {
        self.engine
            .allowed_token_ids_from_last_computation()
            .count_ones(..)
    }
}

impl BnfMask for BnfEngine {
    fn mask(&mut self, logits: &mut [f32]) {
        // ignore errors: if logits is too short, it's a caller bug we can't fix
        let _ = self.mask_logits(logits);
    }

    fn accept(&mut self, token_id: u32) -> bool {
        // If accept_token returns an error (token not allowed), the grammar
        // is still alive — the caller just made a bad choice. Return false
        // only on Finished.
        self.accept_token(token_id).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vocab() -> Vec<Vec<u8>> {
        vec![
            b"".to_vec(),    // 0: empty sentinel
            b" ".to_vec(),   // 1: space
            b"yes".to_vec(), // 2: yes
            b"no".to_vec(),  // 3: no
            b"{".to_vec(),   // 4: {
            b"}".to_vec(),   // 5: }
            b":".to_vec(),   // 6: :
            b"\"".to_vec(),  // 7: "
            b",".to_vec(),   // 8: ,
            b"a".to_vec(),   // 9: a
            b"key".to_vec(), // 10: key
        ]
    }

    #[test]
    fn test_yes_no() {
        let vocab = test_vocab();
        let mut engine = BnfEngine::new("root ::= \"yes\" | \"no\";", &vocab).unwrap();

        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        assert!(logits[2].is_finite());
        assert!(logits[3].is_finite());
        assert!(logits[0].is_infinite());
        assert!(logits[1].is_infinite());
        assert!(logits[4].is_infinite());
        assert!(logits[5].is_infinite());

        let active = engine.accept_token(2).unwrap();
        assert!(!active);
    }

    #[test]
    fn test_fixed_string_value() {
        // A grammar that matches a fixed object with a key and string value.
        // No character classes — uses explicit terminals and nonterminals.
        let vocab = test_vocab();
        let grammar = concat!(
            "string ::= \"\\\"a\\\"\";",
            " ",
            "root ::= \"{\" \"\\\"key\\\"\" \":\" string \"}\";",
        );
        let mut engine = BnfEngine::new(grammar, &vocab).unwrap();

        // Initial state: '{' (4) should be the only allowed token
        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        assert!(logits[4].is_finite(), "'{{' should be allowed at start");
        assert!(logits[7].is_infinite(), "'\"' should not be allowed yet");

        // Accept '{'
        let active = engine.accept_token(4).unwrap();
        assert!(active, "grammar should still be active");

        // Now '"' (7) for the key should be allowed
        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        assert!(logits[7].is_finite(), "'\"' should now be allowed");

        // Accept '"', 'key', '"', ':', '"', 'a', '"', '}'
        // Each step: verify the expected next token is allowed
        engine.accept_token(7).unwrap();
        engine.accept_token(10).unwrap();
        engine.accept_token(7).unwrap();
        engine.accept_token(6).unwrap();
        engine.accept_token(7).unwrap();
        engine.accept_token(9).unwrap();
        engine.accept_token(7).unwrap();

        // Final '}'
        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        assert!(logits[5].is_finite(), "'}}' should be allowed for closing");

        engine.accept_token(5).unwrap();
        assert!(engine.is_finished(), "grammar should be finished");
    }

    #[test]
    fn test_vocab_size() {
        let vocab = test_vocab();
        let engine = BnfEngine::new("root ::= \"yes\";", &vocab).unwrap();
        // vocab_size = max token id (10) + 1 = 11, even though
        // token 0 (empty) is filtered out from the internal vocabulary.
        assert_eq!(engine.vocab_size(), 11);
    }

    #[test]
    fn test_reset() {
        let vocab = test_vocab();
        let mut engine = BnfEngine::new("root ::= \"yes\";", &vocab).unwrap();

        engine.accept_token(2).unwrap();
        assert!(engine.is_finished());

        engine.reset();
        assert!(!engine.is_finished());

        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        assert!(logits[2].is_finite());
    }

    #[test]
    fn test_schema_like_grammar() {
        // Grammar similar to what schema_to_gbnf produces with kbnf-native primitives
        let grammar = concat!(
            r##"string ::= "\"" {char | escape} "\"";"##,
            r##"char ::= #'[ -~]';"##,
            r##"escape ::= "\\" ("\"" | "\\" | "/" | "b" | "f" | "n" | "r" | "t");"##,
            r##"integer ::= ["-"] ("0" | nonzero {digit});"##,
            r##"number ::= integer ["." {digit}] [("e" | "E") ["+" | "-"] {digit}];"##,
            r##"digit ::= "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";"##,
            r##"nonzero ::= "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";"##,
            r##"boolean ::= "true" | "false";"##,
            r##"null ::= "null";"##,
            r##"root_obj ::= "{" "\"key\"" ":" string "}";"##,
            r##"root ::= root_obj;"##,
        );
        // Build vocab with single-byte tokens like RWKV tokenizer
        let mut vocab: Vec<Vec<u8>> = (0u8..128).map(|b| vec![b]).collect();
        // Add a few multi-byte tokens for common words
        let tokens: Vec<&[u8]> = vec![b"true", b"false", b"null", b"key", b" "];
        for t in tokens {
            if !vocab.contains(&t.to_vec()) {
                vocab.push(t.to_vec());
            }
        }

        let engine = BnfEngine::new(grammar, &vocab).unwrap();
        eprintln!(
            "  allowed_count after init: {} / {}",
            engine.allowed_count(),
            engine.vocab_size()
        );
        assert!(
            engine.allowed_count() > 0,
            "grammar should allow at least one token at start"
        );

        // Check that '{' (byte 0x7b) is allowed
        let mut logits = vec![0.0f32; vocab.len()];
        engine.mask_logits(&mut logits).unwrap();
        let open_brace = 0x7b_usize;
        assert!(
            logits[open_brace].is_finite(),
            "'{{' (token {}) should be allowed at start",
            open_brace
        );
    }

    #[test]
    fn test_rejects_nonmatching() {
        let vocab = test_vocab();
        let mut engine = BnfEngine::new("root ::= \"yes\";", &vocab).unwrap();

        // Trying to accept "no" (3) should error since grammar only allows "yes" (2)
        match engine.accept_token(3) {
            Err(_) => {} // expected
            Ok(active) => {
                panic!("expected error accepting token 'no' (3), got active={active}");
            }
        }
    }
}
