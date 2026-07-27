//! Token-level BNF grammar engine wrapping kbnf.

use crate::types::BnfMask;
use ahash::AHashMap;
use kbnf::{
    engine_like::AcceptTokenError, AcceptTokenResult, Config, Engine, EngineLike, Token, Vocabulary,
};

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
    pub fn new(grammar: &str, vocab: &[Vec<u8>]) -> Result<Self, BnfError> {
        let config = Config {
            start_nonterminal: Self::DEFAULT_START.to_string(),
            ..Config::default()
        };
        Self::with_config(grammar, vocab, config)
    }

    /// Create a new BNF engine with a custom kbnf config.
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

        let engine_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Engine::with_config(grammar, vocab_obj, config)
        }));
        let mut engine = match engine_res {
            Ok(Ok(eng)) => eng,
            Ok(Err(e)) => return Err(BnfError::Init(format!("{e:?}"))),
            Err(p) => {
                let msg = p
                    .downcast_ref::<&str>()
                    .copied()
                    .unwrap_or("kbnf syntax parser panic");
                return Err(BnfError::Init(format!("malformed grammar: {msg}")));
            }
        };
        engine.compute_allowed_token_ids();
        Ok(Self { engine })
    }

    /// The vocabulary size reported by kbnf.
    pub fn vocab_size(&self) -> usize {
        self.engine.vocab().vocab_size()
    }

    /// Mask disallowed logits to `f32::NEG_INFINITY`.
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
    pub fn allowed_count(&self) -> usize {
        self.engine
            .allowed_token_ids_from_last_computation()
            .count_ones(..)
    }
}

impl BnfMask for BnfEngine {
    fn mask(&mut self, logits: &mut [f32]) {
        let _ = self.mask_logits(logits);
        if !self.is_finished() && !logits.is_empty() {
            logits[0] = f32::NEG_INFINITY;
        }
    }

    fn accept(&mut self, token_id: u32) -> bool {
        self.accept_token(token_id).unwrap_or(true)
    }
}
