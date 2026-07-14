//! RoCo Grammar — BNF-constrained decoding and JSON Schema → GBNF conversion.
//!
//! Provides [`BnfConstraint`] (wrapping `bnf_sampler` with vocabulary from
//! `web-rwkv`'s tokenizer) and schoolmarm fallback for GBNF features that
//! `bnf_sampler` can't parse (character classes, quantifiers). Also provides
//! [`schema_to_gbnf`] for converting JSON Schema into GBNF grammars.

pub mod bnf;
pub mod json_schema;

pub use bnf::BnfConstraint;
pub use json_schema::{schema_to_gbnf, GbnfError};
