//! RoCo Engine — core trait definitions, data types, and eval framework.
//!
//! This crate defines the [`ModelBackend`] trait that every inference backend
//! implements, the [`CompletionRequest`]/[`CompletionResponse`] types that
//! flow through the pipeline, and the eval suite for benchmarking backends.
//!
//! # Shrunk public API
//!
//! Eval/cases are `pub(crate)` — they are only used internally. External
//! consumers rely on: `ModelBackend`, `CompletionRequest`/`Response`,
//! `EngineError`, `TokenUsage`, `BnfMask`, `MockBackend`, `OnToken`,
//! `TokenCounter`.

pub mod backend;
pub mod types;

pub(crate) mod cases;
/// Evaluation framework — internal to this crate.
/// Use `cargo test -p roco-engine` to run eval tests.
pub(crate) mod eval;

pub use backend::*;
pub use types::*;

// Crate-internal re-exports — test modules and sibling modules use these
// via `use crate::*`. Not re-exported publicly.
pub(crate) use cases::*;
pub(crate) use eval::*;
