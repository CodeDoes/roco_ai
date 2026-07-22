//! RoCo Engine — core trait definitions, data types, and eval framework.
//!
//! This crate defines the [`ModelBackend`] trait that every inference backend
//! implements, the [`CompletionRequest`]/[`CompletionResponse`] types that
//! flow through the pipeline, and the eval suite for benchmarking backends.

pub mod backend;
pub mod cases;
pub mod eval;
pub mod story_evals;
pub mod types;
pub mod util;

pub use backend::*;
pub use cases::*;
pub use eval::*;
pub use types::*;

pub use roco_grammar::Schema;
pub use types::BnfMask;
