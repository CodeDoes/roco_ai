//! RoCo Grammar — JSON Schema → GBNF conversion, Schema builder, output strategies.
//!
//! Provides [`schema_to_gbnf`] for converting JSON Schema into GBNF grammars,
//! a [`Schema`] builder for constructing JSON Schemas programmatically,
//! [`gbnf_to_kbnf`] for converting GBNF to kbnf-compatible format, and
//! output strategies for structured decoding ([`SchemaStrategy`],
//! [`LooseJsonStrategy`], [`RawGbnfStrategy`], [`StateTunedStrategy`]).
//!
//! Grammar enforcement is done through `roco-bnf-engine` (wrapping kbnf),
//! not through this crate.

pub mod bnf;
pub mod grammar_library;
pub mod json_schema;
pub mod kbnf_compat;
pub mod schema;
pub mod strategies;

pub use bnf::gbnf_to_kbnf;
pub use grammar_library::StoryGrammar;
pub use json_schema::{schema_to_gbnf, GbnfError};
pub use schema::Schema;
pub use strategies::{
    evaluate_all_strategies, LooseJsonStrategy, OutputParser, OutputStrategy, RawGbnfStrategy,
    SchemaStrategy, StateTunedStrategy, StrategyEvalResult, StrategyKind, StrategySelector,
};
