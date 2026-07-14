//! BNF grammar-constrained decoding via `bnf_sampler`.
//!
//! Provides [`BnfConstraint`] — wraps a `bnf_sampler` `Sampler` + `Grammar`
//! for use in inference pipelines. Built once per request from a GBNF string
//! + the model's tokenizer.

use std::sync::Arc;

use bnf_sampler::grammar::Grammar;
use bnf_sampler::sampler::{AcceptTokenResult, PossibleTokensResult, Sampler};
use bnf_sampler::utils::U8ArrayWrapper;
use bnf_sampler::vocabulary::Vocabulary;
use bit_set::BitSet;
use qp_trie::Trie;
use rustc_hash::FxHashMap;
use web_rwkv::tokenizer::Tokenizer;

/// Wraps a bnf_sampler `Sampler` + `Grammar` for use in inference.
///
/// Each sample step:
/// 1. `allowed_tokens()` → `BitSet` of valid next token IDs
/// 2. Mask disallowed logits to `f32::MIN`
/// 3. Sample → `accept_token(id)` to advance the grammar state
pub struct BnfConstraint {
    sampler: Sampler,
    current_token_ids: BitSet<u32>,
}

impl BnfConstraint {
    pub fn new(grammar: &str, tokenizer: &Tokenizer) -> anyhow::Result<Self> {
        if grammar.trim().is_empty() {
            anyhow::bail!("empty grammar");
        }

        let vocab = build_vocabulary(tokenizer)?;
        let vocab = Arc::new(vocab);
        let bnf = gbnf_to_bnf(grammar);

        let grammar_arc = Grammar::new(&bnf, vocab.clone(), 1024)
            .map_err(|e| anyhow::anyhow!("BNF grammar parse error: {e:?}"))?;

        let mut sampler = Sampler::new(
            grammar_arc, "root".to_string(), vocab, 1024, false,
        )
        .map_err(|e| anyhow::anyhow!("BNF sampler init error: {e:?}"))?;

        let current_token_ids = match sampler.all_possible_next_tokens(None)? {
            PossibleTokensResult::Continue(ids) => ids.clone(),
            PossibleTokensResult::End => BitSet::new(),
            PossibleTokensResult::InputTokenRejected => {
                anyhow::bail!("initial token set rejected by grammar");
            }
        };

        Ok(Self { sampler, current_token_ids })
    }

    pub fn allowed_tokens(&self) -> &BitSet<u32> {
        &self.current_token_ids
    }

    pub fn accept_token(&mut self, token_id: u32) -> anyhow::Result<bool> {
        match self.sampler.accept_a_token(Some(token_id))? {
            AcceptTokenResult::Continue => {
                self.current_token_ids = match self.sampler.all_possible_next_tokens(None)? {
                    PossibleTokensResult::Continue(ids) => ids.clone(),
                    PossibleTokensResult::End => BitSet::new(),
                    PossibleTokensResult::InputTokenRejected => {
                        anyhow::bail!("token {token_id} rejected after acceptance");
                    }
                };
                Ok(true)
            }
            AcceptTokenResult::End => {
                self.current_token_ids = BitSet::new();
                Ok(false)
            }
            AcceptTokenResult::Failed => {
                anyhow::bail!("token {token_id} rejected by BNF grammar");
            }
        }
    }

    pub fn update(&mut self, prompt: &[u16]) -> anyhow::Result<bool> {
        for &token_id in prompt {
            match self.sampler.accept_a_token(Some(token_id as u32))? {
                AcceptTokenResult::Continue => {}
                AcceptTokenResult::End => {
                    self.current_token_ids = BitSet::new();
                    return Ok(false);
                }
                AcceptTokenResult::Failed => {
                    anyhow::bail!("prompt token {token_id} rejected by BNF grammar");
                }
            }
        }
        self.current_token_ids = match self.sampler.all_possible_next_tokens(None)? {
            PossibleTokensResult::Continue(ids) => ids.clone(),
            PossibleTokensResult::End => BitSet::new(),
            PossibleTokensResult::InputTokenRejected => {
                anyhow::bail!("prompt ended in rejected state");
            }
        };
        Ok(true)
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        self.sampler.reset();
        self.current_token_ids = match self.sampler.all_possible_next_tokens(None)? {
            PossibleTokensResult::Continue(ids) => ids.clone(),
            PossibleTokensResult::End => BitSet::new(),
            PossibleTokensResult::InputTokenRejected => {
                anyhow::bail!("reset produced rejected state");
            }
        };
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GBNF → bnf format conversion
// ---------------------------------------------------------------------------

fn gbnf_to_bnf(gbnf: &str) -> String {
    let mut out = String::with_capacity(gbnf.len() + gbnf.len() / 8);
    for raw in gbnf.lines() {
        let line = raw.trim();
        if line.is_empty() || !line.contains("::=") {
            out.push_str(raw);
            out.push('\n');
            continue;
        }
        if let Some(eq) = line.find("::=") {
            let lhs = line[..eq].trim();
            let rhs = &line[eq..];
            if lhs.starts_with('<') {
                out.push_str(raw);
            } else {
                out.push('<');
                out.push_str(lhs);
                out.push('>');
                out.push_str(rhs);
            }
        } else {
            out.push_str(raw);
        }
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Vocabulary construction from web_rwkv::Tokenizer
// ---------------------------------------------------------------------------

fn build_vocabulary(tokenizer: &Tokenizer) -> anyhow::Result<Vocabulary> {
    let bytes_to_id = tokenizer.bytes_to_token_index();
    let id_to_bytes = tokenizer.token_index_to_bytes();

    let token_to_id: Trie<U8ArrayWrapper, u32> = bytes_to_id
        .iter()
        .map(|(bytes, &id)| (U8ArrayWrapper(bytes.clone().into_boxed_slice()), id))
        .collect();

    let mut id_to_token = FxHashMap::default();
    let mut id_to_token_string = FxHashMap::default();

    for (id, bytes) in id_to_bytes.iter().enumerate() {
        let id = id as u32;
        id_to_token.insert(id, bytes.clone());
        let s = bytes.iter()
            .map(|&b| if b < 0x80 { b as char } else { char::from_u32(0xE000 + (b as u32 - 0x80)).unwrap_or('\u{FFFD}') })
            .collect::<String>();
        id_to_token_string.insert(id, s);
    }

    Ok(Vocabulary { token_to_id, id_to_token, id_to_token_string })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tokenizer() -> Tokenizer {
        let vocab = serde_json::json!({
            "0": "hello", "1": " world", "2": "yes", "3": "no",
            "4": "{", "5": "}", "6": ":", "7": "\"", "8": "answer",
            "9": "\n", "10": " ", "11": "System:", "12": "User:",
            "13": "Assistant:", "14": "<think>", "15": "</think>",
            "16": "<tool_call>", "17": "", "18": "<tools>", "19": "</tools>",
            "20": "<tool_result>", "21": "</tool_result>", "22": "a",
            "23": "b", "24": "c", "25": "Hi",
        });
        Tokenizer::new(&vocab.to_string()).unwrap()
    }

    const MESSAGE_FORMAT_GBNF: &str = r#"sys ::= "System: " txt
user ::= "User: " txt
reply ::= "Assistant: " txt
txt ::= "a" txt | "b" txt | "c" txt | " " txt | "\n" txt | ""
root ::= sys "\n\n" user "\n\n" reply"#;

    #[test]
    fn bnf_message_format_falls_back_to_schoolmarm() {
        let tok = test_tokenizer();
        let result = BnfConstraint::new(MESSAGE_FORMAT_GBNF, &tok);
        assert!(result.is_err(), "message format GBNF should be rejected by bnf_sampler (uses char classes / quantifiers)");
    }

    #[test]
    fn bnf_message_format_parses_with_schoolmarm() {
        use schoolmarm::Grammar;
        let result = Grammar::new(MESSAGE_FORMAT_GBNF);
        if let Err(e) = &result {
            panic!("message format GBNF should parse with schoolmarm: {:?}\ngrammar:\n{}", e, MESSAGE_FORMAT_GBNF);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn bnf_yes_no_grammar_parses() {
        let tok = test_tokenizer();
        let g = r#"root ::= "yes" | "no""#;
        let constraint = BnfConstraint::new(g, &tok);
        if constraint.is_ok() {
            let c = constraint.unwrap();
            assert!(c.allowed_tokens().contains(2), "token 'yes' (id=2) should be allowed");
            assert!(c.allowed_tokens().contains(3), "token 'no' (id=3) should be allowed");
        }
    }

    #[test]
    fn bnf_empty_grammar_errors() {
        let tok = test_tokenizer();
        assert!(BnfConstraint::new("", &tok).is_err());
        assert!(BnfConstraint::new("   ", &tok).is_err());
    }

    #[test]
    fn bnf_vocab_roundtrip() {
        let tok = test_tokenizer();
        let vocab = build_vocabulary(&tok).unwrap();
        assert_eq!(vocab.token_to_id.get(&U8ArrayWrapper(b"yes".to_vec().into_boxed_slice())), Some(&2));
        assert_eq!(vocab.token_to_id.get(&U8ArrayWrapper(b"no".to_vec().into_boxed_slice())), Some(&3));
        assert_eq!(vocab.id_to_token_string.get(&2), Some(&"yes".to_string()));
    }
}
